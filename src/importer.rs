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

    let mut person_cache: HashMap<String, i64> = HashMap::new();
    let mut place_cache: HashMap<String, PlaceCacheEntry> = HashMap::new();

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
                for (excel_row, row) in rows {
                    report.rows_total += 1;
                    import_row(
                        &mut tx,
                        &headers,
                        row,
                        excel_row,
                        &sheet_name,
                        &mut person_cache,
                        &mut place_cache,
                        &mut report,
                    )
                    .await
                    .with_context(|| format!("{sheet_name} row {excel_row}: failed to import"))?;
                    report.rows_imported += 1;
                }
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

#[derive(Clone, Copy)]
struct PlaceCacheEntry {
    id: i64,
    has_coords: bool,
}

#[allow(clippy::too_many_arguments)]
async fn import_row(
    tx: &mut PgConnection,
    headers: &HeaderMap,
    row: &[Data],
    excel_row: usize,
    sheet_name: &str,
    person_cache: &mut HashMap<String, i64>,
    place_cache: &mut HashMap<String, PlaceCacheEntry>,
    report: &mut ImportReport,
) -> Result<()> {
    // ---- verbatim raw row ----
    let raw = headers.raw_json(row);

    // ---- source identity ----
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

    let varak_no = headers.clean_str(row, "varak_no");
    let certificate = headers.clean_str(row, "certificate");
    let source_text = headers.clean_str(row, "text");
    let text_key = source_text
        .as_deref()
        .and_then(normalize_key)
        .filter(|k| k.chars().count() >= MIN_FINGERPRINT_CHARS);

    let src: (i64, bool) = sqlx::query_as(
        r#"
        INSERT INTO source_records (doc_id, varak_no, certificate, source_text, raw, text_fingerprint)
        VALUES ($1, $2, $3, $4, $5, md5($6))
        ON CONFLICT (doc_id) DO UPDATE SET
            varak_no         = EXCLUDED.varak_no,
            certificate      = EXCLUDED.certificate,
            source_text      = EXCLUDED.source_text,
            raw              = EXCLUDED.raw,
            text_fingerprint = EXCLUDED.text_fingerprint
        RETURNING id, (xmax = 0) AS inserted
        "#,
    )
    .bind(doc_id.as_str())
    .bind(varak_no.as_deref())
    .bind(certificate.as_deref())
    .bind(source_text.as_deref())
    .bind(sqlx::types::Json(&raw))
    .bind(text_key.as_deref())
    .fetch_one(&mut *tx)
    .await
    .context("upsert source_records")?;
    let (source_record_id, source_inserted) = src;
    if source_inserted {
        report.source_records_inserted += 1;
    } else {
        report.source_records_updated += 1;
    }

    // ---- persons ----
    let raw_old_kadi = headers.raw_str(row, "old_kadi");
    let raw_new_kadi = headers.raw_str(row, "new_kadi");
    let old_person_id =
        resolve_person(&mut *tx, raw_old_kadi.as_deref(), person_cache, report).await?;
    let new_person_id =
        resolve_person(&mut *tx, raw_new_kadi.as_deref(), person_cache, report).await?;

    // ---- places (+ optional coordinates) ----
    let raw_old_place = headers.raw_str(row, "old_place");
    let raw_new_place = headers.raw_str(row, "new_place");
    for cell in [&raw_old_place, &raw_new_place].into_iter().flatten() {
        if cell.eq_ignore_ascii_case("error") || cell.eq_ignore_ascii_case("#error") {
            report.place_cells_error += 1;
        }
    }
    let (old_lat, old_lon) = read_coords(
        headers,
        row,
        "old_latitude",
        "old_longitude",
        excel_row,
        "origin",
        report,
    );
    let (new_lat, new_lon) = read_coords(
        headers,
        row,
        "new_latitude",
        "new_longitude",
        excel_row,
        "destination",
        report,
    );

    let origin_place_id = resolve_place(
        &mut *tx,
        raw_old_place.as_deref(),
        old_lat,
        old_lon,
        place_cache,
        report,
    )
    .await?;
    let destination_place_id = resolve_place(
        &mut *tx,
        raw_new_place.as_deref(),
        new_lat,
        new_lon,
        place_cache,
        report,
    )
    .await?;

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

    // ---- appointment event ----
    let appt: (i64, bool) = sqlx::query_as(
        r#"
        INSERT INTO appointment_records (
            source_record_id, old_person_id, new_person_id, origin_place_id, destination_place_id,
            raw_old_kadi, raw_new_kadi, raw_old_place, raw_new_place,
            year_original, year_numeric, calendar,
            degree, position_type, period, salary, old_salary, asitane, infisal, region_raw
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9,
            $10, $11, 'unknown',
            $12, $13, $14, $15, $16, $17, $18, $19
        )
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
        RETURNING id, (xmax = 0) AS inserted
        "#,
    )
    .bind(source_record_id)
    .bind(old_person_id)
    .bind(new_person_id)
    .bind(origin_place_id)
    .bind(destination_place_id)
    .bind(raw_old_kadi.as_deref())
    .bind(raw_new_kadi.as_deref())
    .bind(raw_old_place.as_deref())
    .bind(raw_new_place.as_deref())
    .bind(year_original.as_deref())
    .bind(year_numeric)
    .bind(headers.raw_str(row, "degree").and_then(|s| fold_term(&s)))
    .bind(
        headers
            .raw_str(row, "position_type")
            .and_then(|s| fold_term(&s)),
    )
    .bind(headers.clean_str(row, "period"))
    .bind(headers.clean_str(row, "salary"))
    .bind(headers.clean_str(row, "old_salary"))
    .bind(headers.clean_str(row, "asitane"))
    .bind(headers.clean_str(row, "infisal"))
    .bind(headers.clean_str(row, "region"))
    .fetch_one(&mut *tx)
    .await
    .context("upsert appointments")?;
    if appt.1 {
        report.appointments_inserted += 1;
    } else {
        report.appointments_updated += 1;
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

async fn resolve_person(
    tx: &mut PgConnection,
    raw: Option<&str>,
    cache: &mut HashMap<String, i64>,
    report: &mut ImportReport,
) -> Result<Option<i64>> {
    let Some(raw) = raw else { return Ok(None) };
    let Some(norm) = normalize_key(raw) else {
        return Ok(None);
    };
    if let Some(id) = cache.get(&norm) {
        return Ok(Some(*id));
    }

    let (id, created): (i64, bool) = sqlx::query_as(
        r#"
        WITH ins AS (
            INSERT INTO persons (canonical_name, normalized_name)
            VALUES ($1, $2)
            ON CONFLICT (normalized_name) DO NOTHING
            RETURNING id
        )
        SELECT id, true  FROM ins
        UNION ALL
        SELECT id, false FROM persons
         WHERE normalized_name = $2 AND NOT EXISTS (SELECT 1 FROM ins)
        "#,
    )
    .bind(raw.trim())
    .bind(norm.as_str())
    .fetch_one(&mut *tx)
    .await
    .context("resolve person")?;

    if created {
        report.persons_created += 1;
    }
    cache.insert(norm, id);
    Ok(Some(id))
}

async fn resolve_place(
    tx: &mut PgConnection,
    raw: Option<&str>,
    lat: Option<f64>,
    lon: Option<f64>,
    cache: &mut HashMap<String, PlaceCacheEntry>,
    report: &mut ImportReport,
) -> Result<Option<i64>> {
    let Some(raw) = raw else { return Ok(None) };
    let Some(norm) = normalize_key(raw) else {
        return Ok(None);
    };
    let has_new_coords = lat.is_some() && lon.is_some();

    if let Some(entry) = cache.get(&norm).copied() {
        if has_new_coords && !entry.has_coords {
            let filled = fill_place_coords(&mut *tx, entry.id, lat, lon).await?;
            if filled {
                report.place_coordinates_set += 1;
                cache.insert(
                    norm,
                    PlaceCacheEntry {
                        id: entry.id,
                        has_coords: true,
                    },
                );
            }
        }
        return Ok(Some(entry.id));
    }

    let (id, created): (i64, bool) = sqlx::query_as(
        r#"
        WITH ins AS (
            INSERT INTO places (canonical_name, normalized_name, latitude, longitude, coordinate_source)
            VALUES ($1, $2, $3, $4, CASE WHEN $3::float8 IS NOT NULL THEN 'source_row' END)
            ON CONFLICT (normalized_name) DO NOTHING
            RETURNING id
        )
        SELECT id, true  FROM ins
        UNION ALL
        SELECT id, false FROM places
         WHERE normalized_name = $2 AND NOT EXISTS (SELECT 1 FROM ins)
        "#,
    )
    .bind(raw.trim())
    .bind(norm.as_str())
    .bind(lat)
    .bind(lon)
    .fetch_one(&mut *tx)
    .await
    .context("resolve place")?;

    let mut has_coords = created && has_new_coords;
    if created {
        report.places_created += 1;
        if has_new_coords {
            report.place_coordinates_set += 1;
        }
    } else if has_new_coords {
        let filled = fill_place_coords(&mut *tx, id, lat, lon).await?;
        if filled {
            report.place_coordinates_set += 1;
            has_coords = true;
        }
    }

    // Record the name as attested by this source (minimal place_names row;
    // researcher-curated history with validity ranges is added later).
    sqlx::query(
        "INSERT INTO place_names (place_id, name, normalized_name, calendar) \
         VALUES ($1, $2, $3, 'unknown') ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(raw.trim())
    .bind(norm.as_str())
    .execute(&mut *tx)
    .await
    .context("record attested place name")?;

    cache.insert(norm, PlaceCacheEntry { id, has_coords });
    Ok(Some(id))
}

/// Fill coordinates only when the place currently has none. Existing coordinates
/// are never overwritten silently. Returns whether a row was updated.
async fn fill_place_coords(
    tx: &mut PgConnection,
    id: i64,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<bool> {
    let updated = sqlx::query_scalar::<_, i64>(
        "UPDATE places SET latitude = $2, longitude = $3, coordinate_source = 'source_row' \
         WHERE id = $1 AND latitude IS NULL AND longitude IS NULL \
         RETURNING id",
    )
    .bind(id)
    .bind(lat)
    .bind(lon)
    .fetch_optional(&mut *tx)
    .await
    .context("fill place coordinates")?;
    Ok(updated.is_some())
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
