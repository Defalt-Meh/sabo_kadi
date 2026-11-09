//! XLSX importer.
//!
//! Invoked through the `import_xlsx` binary. There is deliberately **no HTTP
//! upload path** — loading data is an offline, transactional operation.
//!
//! A workbook may hold two kinds of sheets, recognised by their headers:
//!  * **appointment** sheets (`doc_id, date, old_kadi, new_kadi, old_place,
//!    new_place, text, ...`) — one register entry per row;
//!  * a **gazetteer** sheet (`original_name, matched_name, wikidata_id, ...,
//!    lat, lon`) — coordinates per place name, validated before use (see
//!    [`crate::gazetteer`] and [`crate::geo`]).
//!
//! Guarantees:
//!  * the whole import runs in a single transaction (all-or-nothing);
//!  * it is idempotent on `source_records.doc_id` (and on the gazetteer's
//!    normalized place name) — re-importing updates rows in place;
//!  * the original spreadsheet row is stored verbatim (`source_records.raw`,
//!    `gazetteer_entries.raw`);
//!  * empty cells become SQL `NULL`; `"-"`-style placeholders and `ERROR`
//!    become `NULL` in the normalized columns while the raw row keeps them;
//!  * persons/places are created and linked; only validated coordinates reach
//!    `places` (via a DB trigger to PostGIS); places are still created when
//!    coordinates are absent;
//!  * the same register entry transcribed in two documents is kept but marked
//!    `duplicate_of`, so it is not counted twice.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{bail, Context, Result};
use calamine::{open_workbook_auto, Data, Range, Reader};
use serde_json::{Map, Value};
use sqlx::{PgConnection, PgPool};

use crate::gazetteer;
use crate::geo::in_study_region;
use crate::normalize::{
    clean_opt, extract_year_numeric, fold_term, normalize_key, parse_coordinate, PLAUSIBLE_YEARS,
};

/// Advisory-lock key so two concurrent imports serialize instead of racing.
const IMPORT_ADVISORY_LOCK: i64 = 0x4B_41_44_49; // "KADI"

/// Texts shorter than this (normalized) are too formulaic to prove that two
/// rows are the same register entry.
const MIN_FINGERPRINT_CHARS: usize = 40;

/// Warnings printed in the summary before eliding the rest.
const MAX_PRINTED_WARNINGS: usize = 40;

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Import only this worksheet; by default every recognised sheet is read.
    pub sheet: Option<String>,
    /// Parse and validate everything, then roll back instead of committing.
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetKind {
    Appointments,
    Gazetteer,
}

#[derive(Debug, Default)]
pub struct GazetteerReport {
    pub rows: usize,
    pub inserted: u64,
    pub updated: u64,
    pub accepted: u64,
    pub needs_review: u64,
    pub rejected: u64,
    pub name_mismatches: u64,
    /// One line per entry whose coordinates are not used.
    pub not_used: Vec<String>,
    pub coordinates_applied: u64,
    pub coordinates_withdrawn: u64,
    pub conflicts: u64,
    pub entries_linked: i64,
    pub entries_unlinked: i64,
}

#[derive(Debug, Default)]
pub struct Coverage {
    pub appointments: i64,
    pub duplicates_hidden: i64,
    pub with_destination: i64,
    pub destination_geocoded: i64,
    pub with_both_places: i64,
    pub both_geocoded: i64,
    pub places: i64,
    pub places_geocoded: i64,
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub sheets: Vec<(String, SheetKind)>,
    pub rows_total: usize,
    pub rows_imported: usize,
    pub rows_skipped_empty: usize,
    pub source_records_inserted: u64,
    pub source_records_updated: u64,
    pub appointments_inserted: u64,
    pub appointments_updated: u64,
    pub persons_created: u64,
    pub places_created: u64,
    pub place_coordinates_set: u64,
    /// Place cells holding an extraction-failure marker (`ERROR`).
    pub place_cells_error: u64,
    /// `year_original` -> rows whose year fell outside [`PLAUSIBLE_YEARS`].
    pub implausible_years: BTreeMap<String, u64>,
    pub gazetteer: GazetteerReport,
    pub coverage: Coverage,
    pub warnings: Vec<String>,
    pub committed: bool,
}

impl ImportReport {
    pub fn print_summary(&self) {
        println!("\n─── import summary ───────────────────────────────");
        for (name, kind) in &self.sheets {
            println!("  sheet                     {name} ({kind:?})");
        }
        if self
            .sheets
            .iter()
            .any(|(_, k)| *k == SheetKind::Appointments)
        {
            println!("  data rows                 {}", self.rows_total);
            println!("  imported                  {}", self.rows_imported);
            println!("  skipped (empty rows)      {}", self.rows_skipped_empty);
            println!(
                "  source_records            {} new / {} updated",
                self.source_records_inserted, self.source_records_updated
            );
            println!(
                "  appointments              {} new / {} updated",
                self.appointments_inserted, self.appointments_updated
            );
            println!("  persons created           {}", self.persons_created);
            println!("  places created            {}", self.places_created);
            println!("  row coordinates set       {}", self.place_coordinates_set);
            println!("  `ERROR` place cells       {}", self.place_cells_error);
            for (year, rows) in &self.implausible_years {
                println!(
                    "  implausible year          `{year}` on {rows} rows (kept as text, not filterable; \
                     plausible: {}–{})",
                    PLAUSIBLE_YEARS.start(),
                    PLAUSIBLE_YEARS.end()
                );
            }
        }

        let g = &self.gazetteer;
        if self.sheets.iter().any(|(_, k)| *k == SheetKind::Gazetteer) {
            println!("  ── gazetteer");
            println!(
                "  entries                   {} new / {} updated",
                g.inserted, g.updated
            );
            println!(
                "  coordinates               {} accepted / {} needs review / {} rejected",
                g.accepted, g.needs_review, g.rejected
            );
            println!(
                "  accepted, renamed match   {} (accepted; listed via /api/v1/gazetteer?issue=name_mismatch)",
                g.name_mismatches
            );
            if !g.not_used.is_empty() {
                println!("  not used for the map:");
                for line in &g.not_used {
                    println!("    • {line}");
                }
            }
        }
        println!("  ── places");
        println!(
            "  gazetteer points applied  {} (withdrawn {}, conflicts {})",
            g.coordinates_applied, g.coordinates_withdrawn, g.conflicts
        );
        println!(
            "  gazetteer names matched   {} of {} (no place with that name yet: {})",
            g.entries_linked,
            g.entries_linked + g.entries_unlinked,
            g.entries_unlinked
        );

        let c = &self.coverage;
        println!("  ── corpus after import");
        println!(
            "  appointments              {} (+{} duplicate transcriptions hidden)",
            c.appointments, c.duplicates_hidden
        );
        println!(
            "  places geocoded           {} of {} ({})",
            c.places_geocoded,
            c.places,
            pct(c.places_geocoded, c.places)
        );
        println!(
            "  destination geocoded      {} of {} ({})",
            c.destination_geocoded,
            c.with_destination,
            pct(c.destination_geocoded, c.with_destination)
        );
        println!(
            "  mappable movements        {} of {} ({})",
            c.both_geocoded,
            c.with_both_places,
            pct(c.both_geocoded, c.with_both_places)
        );

        println!("  warnings                  {}", self.warnings.len());
        for w in self.warnings.iter().take(MAX_PRINTED_WARNINGS) {
            println!("    • {w}");
        }
        if self.warnings.len() > MAX_PRINTED_WARNINGS {
            println!(
                "    … and {} more",
                self.warnings.len() - MAX_PRINTED_WARNINGS
            );
        }
        println!(
            "  transaction               {}",
            if self.committed {
                "COMMITTED"
            } else {
                "ROLLED BACK"
            }
        );
        println!("─────────────────────────────────────────────────");
    }
}

fn pct(part: i64, whole: i64) -> String {
    if whole == 0 {
        "–".to_string()
    } else {
        format!("{:.1}%", 100.0 * part as f64 / whole as f64)
    }
}

/// Known logical columns and the header spellings that map to them.
/// Header cells are matched after normalization (lower-case, non-alphanumerics
/// collapsed to `_`).
fn column_aliases() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "doc_id",
            &[
                "doc_id",
                "docid",
                "document_id",
                "id",
                "kayit_no",
                "kayit_id",
            ],
        ),
        ("degree", &["degree", "derece", "paye"]),
        (
            "position_type",
            &[
                "position_type",
                "position",
                "gorev",
                "gorev_turu",
                "atama_turu",
            ],
        ),
        (
            "period",
            &["period", "period_type", "sure", "mudde", "mueddet"],
        ),
        ("salary", &["salary", "maas", "yevmiye", "akce"]),
        ("old_salary", &["old_salary", "eski_maas", "onceki_maas"]),
        ("asitane", &["asitane", "asitane_mi"]),
        ("infisal", &["infisal", "infisal_mi"]),
        (
            "old_kadi",
            &["old_kadi", "eski_kadi", "onceki_kadi", "selef"],
        ),
        ("new_kadi", &["new_kadi", "yeni_kadi", "atanan", "halef"]),
        (
            "old_place",
            &[
                "old_place",
                "eski_yer",
                "eski_kaza",
                "onceki_yer",
                "mahall_i_sabik",
            ],
        ),
        (
            "new_place",
            &["new_place", "yeni_yer", "yeni_kaza", "mahall_i_lahik"],
        ),
        ("tarih", &["tarih", "date", "sene", "yil", "year"]),
        (
            "varak_no",
            &["varak_no", "varak", "varak_numarasi", "vr_no", "folio"],
        ),
        ("region", &["region", "bolge", "eyalet", "vilayet"]),
        ("certificate", &["certificate", "sicil", "belge", "berat"]),
        (
            "text",
            &["text", "metin", "source_text", "kayit_metni", "hukum"],
        ),
        ("old_latitude", &["old_latitude", "old_lat", "eski_enlem"]),
        (
            "old_longitude",
            &["old_longitude", "old_lng", "old_lon", "eski_boylam"],
        ),
        ("new_latitude", &["new_latitude", "new_lat", "yeni_enlem"]),
        (
            "new_longitude",
            &["new_longitude", "new_lng", "new_lon", "yeni_boylam"],
        ),
    ]
}

fn normalize_header(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_us = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    out.trim_matches('_').to_string()
}

pub(crate) struct HeaderMap {
    /// Raw header text per column index (for the verbatim `raw` JSON).
    raw_headers: Vec<String>,
    /// Whether the header cell was empty.
    blank: Vec<bool>,
    /// Logical column name -> column index.
    logical: HashMap<&'static str, usize>,
}

impl HeaderMap {
    pub(crate) fn build(
        header_cells: &[Data],
        aliases: &[(&'static str, &'static [&'static str])],
    ) -> Result<Self> {
        if header_cells.is_empty() {
            bail!("the worksheet has no header row");
        }
        let mut blank = Vec::with_capacity(header_cells.len());
        let raw_headers: Vec<String> = header_cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let s = cell_to_string(c);
                blank.push(s.trim().is_empty());
                if s.trim().is_empty() {
                    format!("column_{}", i + 1)
                } else {
                    s.trim().to_string()
                }
            })
            .collect();

        let normalized: Vec<String> = raw_headers.iter().map(|h| normalize_header(h)).collect();

        let mut logical = HashMap::new();
        for (logical_name, aliases) in aliases {
            if let Some(idx) = normalized
                .iter()
                .position(|h| aliases.contains(&h.as_str()))
            {
                logical.insert(*logical_name, idx);
            }
        }

        Ok(Self {
            raw_headers,
            blank,
            logical,
        })
    }

    pub(crate) fn has(&self, logical: &str) -> bool {
        self.logical.contains_key(logical)
    }

    pub(crate) fn index(&self, logical: &str) -> Option<usize> {
        self.logical.get(logical).copied()
    }

    pub(crate) fn is_blank(&self, idx: usize) -> bool {
        self.blank.get(idx).copied().unwrap_or(false)
    }

    pub(crate) fn raw_header(&self, idx: usize) -> &str {
        &self.raw_headers[idx]
    }

    /// Map a logical column onto an unlabelled one, naming it in `raw`.
    pub(crate) fn assign(&mut self, logical: &'static str, idx: usize, raw_name: &str) {
        self.logical.insert(logical, idx);
        self.raw_headers[idx] = raw_name.to_string();
        self.blank[idx] = false;
    }

    fn get<'a>(&self, row: &'a [Data], logical: &str) -> Option<&'a Data> {
        let idx = *self.logical.get(logical)?;
        row.get(idx)
    }

    /// Raw string for a logical column: trimmed, `None` when absent/blank.
    pub(crate) fn raw_str(&self, row: &[Data], logical: &str) -> Option<String> {
        let s = self
            .get(row, logical)
            .map(cell_to_string)
            .unwrap_or_default();
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }

    /// Normalized-nullable string: `None` for blank OR placeholder ("-", "yok"…).
    pub(crate) fn clean_str(&self, row: &[Data], logical: &str) -> Option<String> {
        self.raw_str(row, logical).and_then(|s| clean_opt(&s))
    }

    /// The row as delivered, keyed by header. Empty cells under an empty
    /// header (trailing spreadsheet padding) are left out.
    pub(crate) fn raw_json(&self, row: &[Data]) -> Value {
        let mut raw = Map::new();
        for (i, cell) in row.iter().enumerate() {
            if matches!(cell, Data::Empty) && self.blank.get(i).copied().unwrap_or(true) {
                continue;
            }
            let key = self
                .raw_headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("column_{}", i + 1));
            raw.insert(key, cell_to_json(cell));
        }
        Value::Object(raw)
    }
}

/// Decide what a sheet holds from its header row.
fn classify(header: &[Data]) -> Result<(SheetKind, HeaderMap)> {
    let gaz = HeaderMap::build(header, gazetteer::column_aliases())?;
    let appt = HeaderMap::build(header, column_aliases())?;
    let is_appt = ["new_place", "old_place", "new_kadi", "old_kadi"]
        .iter()
        .any(|c| appt.has(c));
    if is_appt {
        Ok((SheetKind::Appointments, appt))
    } else if gazetteer::looks_like_gazetteer(&gaz) {
        Ok((SheetKind::Gazetteer, gaz))
    } else {
        bail!("header row matches neither an appointment sheet nor a gazetteer")
    }
}

fn appointment_header_warnings(headers: &HeaderMap, sheet: &str, warnings: &mut Vec<String>) {
    if !headers.has("doc_id") {
        warnings.push(format!(
            "{sheet}: no `doc_id` column detected; per-row identifiers will be synthesized from the \
             sheet name and row number (re-import stays idempotent only if row order is stable)"
        ));
    }
    for required in ["old_place", "new_place"] {
        if !headers.has(required) {
            warnings.push(format!(
                "{sheet}: no `{required}` column detected; movement endpoints will be sparse"
            ));
        }
    }
}

pub async fn import_path(pool: &PgPool, path: &Path, opts: &ImportOptions) -> Result<ImportReport> {
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("could not open workbook `{}`", path.display()))?;
    let source_file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let names: Vec<String> = match &opts.sheet {
        Some(name) => {
            if !workbook.sheet_names().iter().any(|s| s == name) {
                bail!(
                    "worksheet `{name}` not found; available: {}",
                    workbook.sheet_names().join(", ")
                );
            }
            vec![name.clone()]
        }
        None => workbook.sheet_names().to_vec(),
    };
    if names.is_empty() {
        bail!("workbook contains no worksheets");
    }

    let mut report = ImportReport::default();

    // Read + classify every sheet up front; gazetteers are imported first so
    // places created by the appointment sheets can be geocoded in this run.
    let mut sheets: Vec<(String, SheetKind, HeaderMap, Range<Data>)> = Vec::new();
    for name in names {
        let range = workbook
            .worksheet_range(&name)
            .with_context(|| format!("could not read worksheet `{name}`"))?;
        let Some(header) = range.rows().next() else {
            report.warnings.push(format!("{name}: worksheet is empty"));
            continue;
        };
        match classify(header) {
            Ok((kind, headers)) => sheets.push((name, kind, headers, range)),
            Err(e) if opts.sheet.is_none() => {
                report.warnings.push(format!("{name}: skipped — {e}"));
            }
            Err(e) => return Err(e.context(format!("worksheet `{name}`"))),
        }
    }
    sheets.sort_by_key(|(_, kind, _, _)| *kind != SheetKind::Gazetteer);

    let mut tx = pool.begin().await.context("failed to open transaction")?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(IMPORT_ADVISORY_LOCK)
        .execute(&mut *tx)
        .await
        .context("failed to take the import advisory lock")?;

    for (sheet_name, kind, mut headers, range) in sheets {
        report.sheets.push((sheet_name.clone(), kind));
        // 1-based Excel row numbers, header included
        let first_row = range.start().map(|(r, _)| r as usize + 1).unwrap_or(1);
        let mut rows: Vec<(usize, &[Data])> = Vec::new();
        for (offset, row) in range.rows().enumerate().skip(1) {
            if row.iter().all(|c| matches!(c, Data::Empty)) {
                if kind == SheetKind::Appointments {
                    report.rows_skipped_empty += 1;
                }
                continue;
            }
            rows.push((first_row + offset, row));
        }

        match kind {
            SheetKind::Gazetteer => {
                gazetteer::repair_missing_longitude(&mut headers, &mut report.warnings);
                gazetteer::import_sheet(
                    &mut tx,
                    &headers,
                    &rows,
                    &source_file,
                    &sheet_name,
                    &mut report,
                )
                .await?;
            }
            SheetKind::Appointments => {
                appointment_header_warnings(&headers, &sheet_name, &mut report.warnings);
                import_appointment_sheet(&mut tx, &headers, &rows, &sheet_name, &mut report)
                    .await?;
            }
        }
    }

    gazetteer::sync_places(&mut tx, &mut report).await?;
    mark_duplicate_transcriptions(&mut tx).await?;
    report.coverage = coverage(&mut tx).await?;

    if opts.dry_run {
        tx.rollback().await.context("rollback failed")?;
        report.committed = false;
    } else {
        tx.commit().await.context("commit failed")?;
        report.committed = true;
    }

    Ok(report)
}

/// Mark every appointment whose source text is identical (after normalization)
/// to an earlier one as `duplicate_of` that one. The kept copy is the first by
/// doc_id, preferring documents without a `(1)`-style copy suffix. Recomputed
/// over the whole table on every import, so it is stable under re-imports.
async fn mark_duplicate_transcriptions(tx: &mut PgConnection) -> Result<()> {
    sqlx::query(
        r#"
        WITH ranked AS (
            SELECT a.id,
                   first_value(a.id) OVER (
                       PARTITION BY s.text_fingerprint
                       ORDER BY (s.doc_id ~ '\(\d+\)'), s.doc_id, a.id
                   ) AS keep_id
              FROM appointment_records a
              JOIN source_records s ON s.id = a.source_record_id
             WHERE s.text_fingerprint IS NOT NULL
        ), target AS (
            SELECT a.id, NULLIF(r.keep_id, a.id) AS dup
              FROM appointment_records a
              LEFT JOIN ranked r ON r.id = a.id
        )
        UPDATE appointment_records a
           SET duplicate_of = t.dup
          FROM target t
         WHERE t.id = a.id
           AND a.duplicate_of IS DISTINCT FROM t.dup
        "#,
    )
    .execute(&mut *tx)
    .await
    .context("mark duplicate transcriptions")?;
    Ok(())
}

async fn coverage(tx: &mut PgConnection) -> Result<Coverage> {
    let row: (i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT count(*),
               count(*) FILTER (WHERE a.destination_place_id IS NOT NULL),
               count(*) FILTER (WHERE d.geom IS NOT NULL),
               count(*) FILTER (WHERE a.origin_place_id IS NOT NULL AND a.destination_place_id IS NOT NULL),
               count(*) FILTER (WHERE o.geom IS NOT NULL AND d.geom IS NOT NULL)
          FROM appointments a
          LEFT JOIN places o ON o.id = a.origin_place_id
          LEFT JOIN places d ON d.id = a.destination_place_id
        "#,
    )
    .fetch_one(&mut *tx)
    .await
    .context("coverage: appointments")?;
    let (duplicates, places, geocoded): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT (SELECT count(*) FROM appointment_records WHERE duplicate_of IS NOT NULL),
               (SELECT count(*) FROM places),
               (SELECT count(*) FROM places WHERE geom IS NOT NULL)
        "#,
    )
    .fetch_one(&mut *tx)
    .await
    .context("coverage: places")?;
    Ok(Coverage {
        appointments: row.0,
        duplicates_hidden: duplicates,
        with_destination: row.1,
        destination_geocoded: row.2,
        with_both_places: row.3,
        both_geocoded: row.4,
        places,
        places_geocoded: geocoded,
    })
}

// ---------------------------------------------------------------------------
// Appointment sheets — bulk path
//
// The sheet is parsed in memory first; the database then sees a handful of
// set-based statements (`UNNEST` over parallel arrays) per `CHUNK` rows instead
// of ~6 round trips per row. Against a remote database (Neon) that is the
// difference between hours and seconds. Semantics match the per-row importer
// it replaced: first spelling of a name becomes its canonical form, first
// valid coordinates fill an empty place, a repeated doc_id lets the later row
// win.
// ---------------------------------------------------------------------------

/// Rows per bulk statement.
const CHUNK: usize = 200;

struct ParsedRow {
    doc_id: String,
    varak_no: Option<String>,
    certificate: Option<String>,
    source_text: Option<String>,
    text_key: Option<String>,
    raw: String,
    raw_old_kadi: Option<String>,
    raw_new_kadi: Option<String>,
    raw_old_place: Option<String>,
    raw_new_place: Option<String>,
    old_person: Option<String>,
    new_person: Option<String>,
    origin: Option<String>,
    destination: Option<String>,
    year_original: Option<String>,
    year_numeric: Option<i32>,
    degree: Option<String>,
    position_type: Option<String>,
    period: Option<String>,
    salary: Option<String>,
    old_salary: Option<String>,
    asitane: Option<String>,
    infisal: Option<String>,
    region: Option<String>,
}

/// Distinct names in first-seen order: normalized key -> (canonical, coords).
#[derive(Default)]
struct NameSet {
    keys: Vec<String>,
    canonical: Vec<String>,
    lat: Vec<Option<f64>>,
    lon: Vec<Option<f64>>,
    index: HashMap<String, usize>,
}

impl NameSet {
    /// Register a raw mention; returns its normalized key.
    fn add(&mut self, raw: Option<&str>, coords: (Option<f64>, Option<f64>)) -> Option<String> {
        let raw = raw?;
        let key = normalize_key(raw)?;
        match self.index.get(&key) {
            Some(&i) => {
                if self.lat[i].is_none() && coords.0.is_some() {
                    self.lat[i] = coords.0;
                    self.lon[i] = coords.1;
                }
            }
            None => {
                self.index.insert(key.clone(), self.keys.len());
                self.keys.push(key.clone());
                self.canonical.push(raw.trim().to_string());
                self.lat.push(coords.0);
                self.lon.push(coords.1);
            }
        }
        Some(key)
    }
}

fn parse_row(
    headers: &HeaderMap,
    row: &[Data],
    excel_row: usize,
    sheet_name: &str,
    persons: &mut NameSet,
    places: &mut NameSet,
    report: &mut ImportReport,
) -> ParsedRow {
    let doc_id = match headers.clean_str(row, "doc_id") {
        Some(id) => id,
        None => {
            let synthetic = format!("{sheet_name}::row-{excel_row}");
            report.warnings.push(format!(
                "row {excel_row}: missing doc_id, synthesized `{synthetic}`"
            ));
            synthetic
        }
    };

    let source_text = headers.clean_str(row, "text");
    let text_key = source_text
        .as_deref()
        .and_then(normalize_key)
        .filter(|k| k.chars().count() >= MIN_FINGERPRINT_CHARS);

    let raw_old_kadi = headers.raw_str(row, "old_kadi");
    let raw_new_kadi = headers.raw_str(row, "new_kadi");
    let old_person = persons.add(raw_old_kadi.as_deref(), (None, None));
    let new_person = persons.add(raw_new_kadi.as_deref(), (None, None));

    let raw_old_place = headers.raw_str(row, "old_place");
    let raw_new_place = headers.raw_str(row, "new_place");
    for cell in [&raw_old_place, &raw_new_place].into_iter().flatten() {
        if cell.eq_ignore_ascii_case("error") || cell.eq_ignore_ascii_case("#error") {
            report.place_cells_error += 1;
        }
    }
    let old_coords = read_coords(
        headers,
        row,
        "old_latitude",
        "old_longitude",
        excel_row,
        "origin",
        report,
    );
    let new_coords = read_coords(
        headers,
        row,
        "new_latitude",
        "new_longitude",
        excel_row,
        "destination",
        report,
    );
    let origin = places.add(raw_old_place.as_deref(), old_coords);
    let destination = places.add(raw_new_place.as_deref(), new_coords);

    // ---- date (never guess the calendar) ----
    let year_original = headers.raw_str(row, "tarih");
    let year_numeric = match year_original.as_deref().and_then(extract_year_numeric) {
        Some(y) if !PLAUSIBLE_YEARS.contains(&y) => {
            *report
                .implausible_years
                .entry(year_original.clone().unwrap_or_default())
                .or_default() += 1;
            None
        }
        other => other,
    };

    ParsedRow {
        doc_id,
        varak_no: headers.clean_str(row, "varak_no"),
        certificate: headers.clean_str(row, "certificate"),
        source_text,
        text_key,
        raw: headers.raw_json(row).to_string(),
        raw_old_kadi,
        raw_new_kadi,
        raw_old_place,
        raw_new_place,
        old_person,
        new_person,
        origin,
        destination,
        year_original,
        year_numeric,
        degree: headers.raw_str(row, "degree").and_then(|s| fold_term(&s)),
        position_type: headers
            .raw_str(row, "position_type")
            .and_then(|s| fold_term(&s)),
        period: headers.clean_str(row, "period"),
        salary: headers.clean_str(row, "salary"),
        old_salary: headers.clean_str(row, "old_salary"),
        asitane: headers.clean_str(row, "asitane"),
        infisal: headers.clean_str(row, "infisal"),
        region: headers.clean_str(row, "region"),
    }
}

async fn import_appointment_sheet(
    tx: &mut PgConnection,
    headers: &HeaderMap,
    rows: &[(usize, &[Data])],
    sheet_name: &str,
    report: &mut ImportReport,
) -> Result<()> {
    // ---- parse everything first ----
    let mut persons = NameSet::default();
    let mut places = NameSet::default();
    let mut parsed: Vec<ParsedRow> = Vec::with_capacity(rows.len());
    let mut by_doc: HashMap<String, (usize, usize)> = HashMap::new();
    for &(excel_row, row) in rows {
        report.rows_total += 1;
        let p = parse_row(
            headers,
            row,
            excel_row,
            sheet_name,
            &mut persons,
            &mut places,
            report,
        );
        // a repeated doc_id: the later row wins, as a re-import would
        if let Some(&(i, first_row)) = by_doc.get(&p.doc_id) {
            report.warnings.push(format!(
                "{sheet_name} row {excel_row}: doc_id `{}` repeats row {first_row}; the later row wins",
                p.doc_id
            ));
            parsed[i] = p;
            by_doc.insert(parsed[i].doc_id.clone(), (i, excel_row));
        } else {
            by_doc.insert(p.doc_id.clone(), (parsed.len(), excel_row));
            parsed.push(p);
        }
        report.rows_imported += 1;
    }

    let person_ids = upsert_persons(tx, &persons, report).await?;
    let place_ids = upsert_places(tx, &places, report).await?;

    // ---- source records + appointments, chunk by chunk ----
    let total = parsed.len();
    for (n, chunk) in parsed.chunks(CHUNK).enumerate() {
        let source_ids = upsert_source_chunk(tx, chunk, report)
            .await
            .with_context(|| format!("{sheet_name}: source records, chunk {}", n + 1))?;
        upsert_appointment_chunk(tx, chunk, &source_ids, &person_ids, &place_ids, report)
            .await
            .with_context(|| format!("{sheet_name}: appointments, chunk {}", n + 1))?;
        eprintln!(
            "  {sheet_name}: {} / {total} rows",
            (n * CHUNK + chunk.len())
        );
    }
    Ok(())
}

async fn upsert_persons(
    tx: &mut PgConnection,
    persons: &NameSet,
    report: &mut ImportReport,
) -> Result<HashMap<String, i64>> {
    let mut ids = HashMap::with_capacity(persons.keys.len());
    for (keys, canonical) in persons
        .keys
        .chunks(CHUNK)
        .zip(persons.canonical.chunks(CHUNK))
    {
        report.persons_created += sqlx::query(
            "INSERT INTO persons (canonical_name, normalized_name) \
             SELECT * FROM UNNEST($1::text[], $2::text[]) \
             ON CONFLICT (normalized_name) DO NOTHING",
        )
        .bind(canonical)
        .bind(keys)
        .execute(&mut *tx)
        .await
        .context("insert persons")?
        .rows_affected();

        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT normalized_name, id FROM persons WHERE normalized_name = ANY($1::text[])",
        )
        .bind(keys)
        .fetch_all(&mut *tx)
        .await
        .context("look up persons")?;
        ids.extend(rows);
    }
    Ok(ids)
}

async fn upsert_places(
    tx: &mut PgConnection,
    places: &NameSet,
    report: &mut ImportReport,
) -> Result<HashMap<String, i64>> {
    let mut ids = HashMap::with_capacity(places.keys.len());
    let n = places.keys.len();
    for start in (0..n).step_by(CHUNK) {
        let end = (start + CHUNK).min(n);
        let (keys, canonical) = (&places.keys[start..end], &places.canonical[start..end]);
        let (lat, lon) = (&places.lat[start..end], &places.lon[start..end]);

        // new places, with their first valid coordinates
        let created: Vec<bool> = sqlx::query_scalar(
            "INSERT INTO places (canonical_name, normalized_name, latitude, longitude, coordinate_source) \
             SELECT c, k, la, lo, CASE WHEN la IS NOT NULL THEN 'source_row' END \
               FROM UNNEST($1::text[], $2::text[], $3::float8[], $4::float8[]) AS u(c, k, la, lo) \
             ON CONFLICT (normalized_name) DO NOTHING \
             RETURNING latitude IS NOT NULL",
        )
        .bind(canonical)
        .bind(keys)
        .bind(lat)
        .bind(lon)
        .fetch_all(&mut *tx)
        .await
        .context("insert places")?;
        report.places_created += created.len() as u64;
        report.place_coordinates_set += created.iter().filter(|c| **c).count() as u64;

        // existing places without coordinates get the first valid pair;
        // existing coordinates are never overwritten
        report.place_coordinates_set += sqlx::query(
            "UPDATE places p \
                SET latitude = u.la, longitude = u.lo, coordinate_source = 'source_row' \
               FROM UNNEST($1::text[], $2::float8[], $3::float8[]) AS u(k, la, lo) \
              WHERE p.normalized_name = u.k AND u.la IS NOT NULL \
                AND p.latitude IS NULL AND p.longitude IS NULL",
        )
        .bind(keys)
        .bind(lat)
        .bind(lon)
        .execute(&mut *tx)
        .await
        .context("fill place coordinates")?
        .rows_affected();

        // the name as attested by this source (minimal place_names row;
        // researcher-curated history with validity ranges is added later)
        sqlx::query(
            "INSERT INTO place_names (place_id, name, normalized_name, calendar) \
             SELECT p.id, u.name, u.k, 'unknown' \
               FROM UNNEST($1::text[], $2::text[]) AS u(name, k) \
               JOIN places p ON p.normalized_name = u.k \
             ON CONFLICT DO NOTHING",
        )
        .bind(canonical)
        .bind(keys)
        .execute(&mut *tx)
        .await
        .context("record attested place names")?;

        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT normalized_name, id FROM places WHERE normalized_name = ANY($1::text[])",
        )
        .bind(keys)
        .fetch_all(&mut *tx)
        .await
        .context("look up places")?;
        ids.extend(rows);
    }
    Ok(ids)
}

/// Upsert one chunk of source records; returns doc_id -> id.
async fn upsert_source_chunk(
    tx: &mut PgConnection,
    chunk: &[ParsedRow],
    report: &mut ImportReport,
) -> Result<HashMap<String, i64>> {
    let col = |f: fn(&ParsedRow) -> Option<&str>| chunk.iter().map(f).collect::<Vec<_>>();
    let rows: Vec<(String, i64, bool)> = sqlx::query_as(
        r#"
        INSERT INTO source_records (doc_id, varak_no, certificate, source_text, raw, text_fingerprint)
        SELECT d, v, c, t, r::jsonb, md5(f)
          FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[])
               AS u(d, v, c, t, r, f)
        ON CONFLICT (doc_id) DO UPDATE SET
            varak_no         = EXCLUDED.varak_no,
            certificate      = EXCLUDED.certificate,
            source_text      = EXCLUDED.source_text,
            raw              = EXCLUDED.raw,
            text_fingerprint = EXCLUDED.text_fingerprint
        RETURNING doc_id, id, (xmax = 0) AS inserted
        "#,
    )
    .bind(col(|r| Some(r.doc_id.as_str())))
    .bind(col(|r| r.varak_no.as_deref()))
    .bind(col(|r| r.certificate.as_deref()))
    .bind(col(|r| r.source_text.as_deref()))
    .bind(col(|r| Some(r.raw.as_str())))
    .bind(col(|r| r.text_key.as_deref()))
    .fetch_all(&mut *tx)
    .await?;

    let mut ids = HashMap::with_capacity(rows.len());
    for (doc_id, id, inserted) in rows {
        if inserted {
            report.source_records_inserted += 1;
        } else {
            report.source_records_updated += 1;
        }
        ids.insert(doc_id, id);
    }
    Ok(ids)
}

async fn upsert_appointment_chunk(
    tx: &mut PgConnection,
    chunk: &[ParsedRow],
    source_ids: &HashMap<String, i64>,
    person_ids: &HashMap<String, i64>,
    place_ids: &HashMap<String, i64>,
    report: &mut ImportReport,
) -> Result<()> {
    let id_of = |map: &HashMap<String, i64>, key: &Option<String>| -> Option<i64> {
        key.as_ref().and_then(|k| map.get(k).copied())
    };
    let text = |f: fn(&ParsedRow) -> Option<&str>| chunk.iter().map(f).collect::<Vec<_>>();
    let source: Vec<i64> = chunk
        .iter()
        .map(|r| {
            source_ids
                .get(&r.doc_id)
                .copied()
                .with_context(|| format!("no source record id for `{}`", r.doc_id))
        })
        .collect::<Result<_>>()?;
    let old_person: Vec<Option<i64>> = chunk
        .iter()
        .map(|r| id_of(person_ids, &r.old_person))
        .collect();
    let new_person: Vec<Option<i64>> = chunk
        .iter()
        .map(|r| id_of(person_ids, &r.new_person))
        .collect();
    let origin: Vec<Option<i64>> = chunk.iter().map(|r| id_of(place_ids, &r.origin)).collect();
    let destination: Vec<Option<i64>> = chunk
        .iter()
        .map(|r| id_of(place_ids, &r.destination))
        .collect();
    let year_numeric: Vec<Option<i32>> = chunk.iter().map(|r| r.year_numeric).collect();

    let inserted: Vec<bool> = sqlx::query_scalar(
        r#"
        INSERT INTO appointment_records (
            source_record_id, old_person_id, new_person_id, origin_place_id, destination_place_id,
            raw_old_kadi, raw_new_kadi, raw_old_place, raw_new_place,
            year_original, year_numeric, calendar,
            degree, position_type, period, salary, old_salary, asitane, infisal, region_raw
        )
        SELECT s, op, np, o, d, rok, rnk, rop, rnp, yo, yn, 'unknown',
               deg, pt, per, sal, osal, asi, inf, reg
          FROM UNNEST(
                $1::int8[], $2::int8[], $3::int8[], $4::int8[], $5::int8[],
                $6::text[], $7::text[], $8::text[], $9::text[],
                $10::text[], $11::int4[],
                $12::text[], $13::text[], $14::text[], $15::text[], $16::text[],
                $17::text[], $18::text[], $19::text[]
               ) AS u(s, op, np, o, d, rok, rnk, rop, rnp, yo, yn,
                      deg, pt, per, sal, osal, asi, inf, reg)
        ON CONFLICT (source_record_id) DO UPDATE SET
            old_person_id        = EXCLUDED.old_person_id,
            new_person_id        = EXCLUDED.new_person_id,
            origin_place_id      = EXCLUDED.origin_place_id,
            destination_place_id = EXCLUDED.destination_place_id,
            raw_old_kadi         = EXCLUDED.raw_old_kadi,
            raw_new_kadi         = EXCLUDED.raw_new_kadi,
            raw_old_place        = EXCLUDED.raw_old_place,
            raw_new_place        = EXCLUDED.raw_new_place,
            year_original        = EXCLUDED.year_original,
            year_numeric         = EXCLUDED.year_numeric,
            degree               = EXCLUDED.degree,
            position_type        = EXCLUDED.position_type,
            period               = EXCLUDED.period,
            salary               = EXCLUDED.salary,
            old_salary           = EXCLUDED.old_salary,
            asitane              = EXCLUDED.asitane,
            infisal              = EXCLUDED.infisal,
            region_raw           = EXCLUDED.region_raw
        RETURNING (xmax = 0)
        "#,
    )
    .bind(&source)
    .bind(&old_person)
    .bind(&new_person)
    .bind(&origin)
    .bind(&destination)
    .bind(text(|r| r.raw_old_kadi.as_deref()))
    .bind(text(|r| r.raw_new_kadi.as_deref()))
    .bind(text(|r| r.raw_old_place.as_deref()))
    .bind(text(|r| r.raw_new_place.as_deref()))
    .bind(text(|r| r.year_original.as_deref()))
    .bind(&year_numeric)
    .bind(text(|r| r.degree.as_deref()))
    .bind(text(|r| r.position_type.as_deref()))
    .bind(text(|r| r.period.as_deref()))
    .bind(text(|r| r.salary.as_deref()))
    .bind(text(|r| r.old_salary.as_deref()))
    .bind(text(|r| r.asitane.as_deref()))
    .bind(text(|r| r.infisal.as_deref()))
    .bind(text(|r| r.region.as_deref()))
    .fetch_all(&mut *tx)
    .await?;

    for i in inserted {
        if i {
            report.appointments_inserted += 1;
        } else {
            report.appointments_updated += 1;
        }
    }
    Ok(())
}

fn read_coords(
    headers: &HeaderMap,
    row: &[Data],
    lat_col: &str,
    lon_col: &str,
    excel_row: usize,
    label: &str,
    report: &mut ImportReport,
) -> (Option<f64>, Option<f64>) {
    let lat_raw = headers.raw_str(row, lat_col);
    let lon_raw = headers.raw_str(row, lon_col);
    if lat_raw.is_none() && lon_raw.is_none() {
        return (None, None);
    }
    let lat = lat_raw.as_deref().and_then(parse_coordinate);
    let lon = lon_raw.as_deref().and_then(parse_coordinate);

    match (lat, lon) {
        (Some(la), Some(lo)) if in_study_region(la, lo) => (Some(la), Some(lo)),
        _ => {
            report.warnings.push(format!(
                "row {excel_row}: ignoring invalid or out-of-region {label} coordinates (lat={:?}, lon={:?})",
                lat_raw, lon_raw
            ));
            (None, None)
        }
    }
}

// ---------------------------------------------------------------------------
// Cell conversions
// ---------------------------------------------------------------------------

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => format_float(*f),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#ERROR({e:?})"),
    }
}

fn cell_to_json(cell: &Data) -> Value {
    match cell {
        Data::Empty => Value::Null,
        Data::String(s) => Value::String(s.clone()),
        Data::Int(i) => Value::Number((*i).into()),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
                Value::Number((*f as i64).into())
            } else {
                serde_json::Number::from_f64(*f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
        }
        Data::Bool(b) => Value::Bool(*b),
        Data::DateTime(dt) => Value::String(dt.to_string()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Value::String(s.clone()),
        Data::Error(e) => Value::String(format!("#ERROR({e:?})")),
    }
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}
