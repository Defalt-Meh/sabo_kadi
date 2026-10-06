//! Gazetteer sheet import and place-coordinate synchronisation.
//!
//! A gazetteer sheet maps a historical place name to a modern entity and a
//! coordinate pair (`original_name, matched_name, wikidata_id, ..., lat, lon`).
//! Its coordinates are third-party matches and are validated row by row (see
//! [`crate::geo`]). Every row is stored verbatim in `gazetteer_entries` with
//! its verdict; [`sync_places`] then copies **accepted** points onto `places`
//! (matched by normalized name) and withdraws gazetteer points that are no
//! longer accepted. Coordinates that came from appointment rows are never
//! overwritten — a disagreement is reported instead.

use std::collections::HashMap;

use anyhow::{Context, Result};
use calamine::Data;
use serde_json::Value;
use sqlx::PgConnection;

use crate::geo::{self, issue, CoordinateStatus};
use crate::importer::{HeaderMap, ImportReport};
use crate::normalize::normalize_key;

/// Two candidate points for the same name further apart than this disagree.
const SAME_PLACE_KM: f64 = 5.0;

pub(crate) fn column_aliases() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "name",
            &[
                "original_name",
                "name",
                "place",
                "place_name",
                "kaza",
                "kaza_adi",
                "yer",
                "yer_adi",
            ],
        ),
        (
            "matched_name",
            &["matched_name", "matched", "modern_name", "guncel_ad"],
        ),
        (
            "wikidata",
            &["wikidata_id", "wikidata", "wikidata_qid", "qid"],
        ),
        ("wikipedia_url", &["wikipedia_url", "wikipedia"]),
        ("country", &["country", "ulke"]),
        ("latitude", &["lat", "latitude", "enlem"]),
        ("longitude", &["lon", "lng", "long", "longitude", "boylam"]),
    ]
}

/// Does this header row describe a gazetteer?
pub(crate) fn looks_like_gazetteer(headers: &HeaderMap) -> bool {
    headers.has("name") && headers.has("latitude")
}

/// The delivered sheet labels only `lat`; the longitude sits in the unlabelled
/// column right after it. Accept that layout, loudly.
pub(crate) fn repair_missing_longitude(headers: &mut HeaderMap, warnings: &mut Vec<String>) {
    if headers.has("longitude") {
        return;
    }
    let Some(lat_idx) = headers.index("latitude") else {
        return;
    };
    let lon_idx = lat_idx + 1;
    if headers.is_blank(lon_idx) {
        headers.assign("longitude", lon_idx, "longitude (unlabelled column)");
        warnings.push(format!(
            "gazetteer: no longitude header; reading longitude from the unlabelled column {} \
             right after `{}`",
            column_letter(lon_idx),
            headers.raw_header(lat_idx),
        ));
    } else {
        warnings
            .push("gazetteer: no longitude column found; every entry will be rejected".to_string());
    }
}

struct Entry {
    excel_row: usize,
    normalized_name: String,
    original_name: String,
    matched_name: Option<String>,
    wikidata_qid: Option<String>,
    wikipedia_url: Option<String>,
    country: Option<String>,
    raw_latitude: Option<String>,
    raw_longitude: Option<String>,
    assessment: geo::Assessment,
    raw: Value,
}

pub(crate) async fn import_sheet(
    tx: &mut PgConnection,
    headers: &HeaderMap,
    rows: &[(usize, &[Data])],
    source_file: &str,
    sheet: &str,
    report: &mut ImportReport,
) -> Result<()> {
    // ---- parse + assess every row ----
    let mut entries: Vec<Entry> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for &(excel_row, row) in rows {
        report.gazetteer.rows += 1;
        let Some(original_name) = headers.clean_str(row, "name") else {
            report
                .warnings
                .push(format!("{sheet} row {excel_row}: no place name, skipped"));
            continue;
        };
        let Some(normalized_name) = normalize_key(&original_name) else {
            continue;
        };
        let matched_name = headers.clean_str(row, "matched_name");
        let country = headers.clean_str(row, "country");
        let raw_latitude = headers.clean_str(row, "latitude");
        let raw_longitude = headers.clean_str(row, "longitude");
        let assessment = geo::assess(
            raw_latitude.as_deref(),
            raw_longitude.as_deref(),
            &original_name,
            matched_name.as_deref(),
            country.as_deref(),
        );

        if let Some(&first) = by_name.get(&normalized_name) {
            // Same name delivered twice: keep the first row, flag disagreement.
            let kept = &mut entries[first];
            let disagree = match (
                kept.assessment.latitude.zip(kept.assessment.longitude),
                assessment.latitude.zip(assessment.longitude),
            ) {
                (Some(a), Some(b)) => geo::distance_km(a, b) > SAME_PLACE_KM,
                (None, None) => false,
                _ => true,
            };
            if disagree {
                kept.assessment
                    .flag_for_review(issue::CONFLICTING_DUPLICATE);
            }
            report.warnings.push(format!(
                "{sheet} row {excel_row}: `{original_name}` repeats row {}{}; kept the first",
                kept.excel_row,
                if disagree {
                    " with different coordinates"
                } else {
                    ""
                },
            ));
            continue;
        }

        by_name.insert(normalized_name.clone(), entries.len());
        entries.push(Entry {
            excel_row,
            normalized_name,
            original_name,
            matched_name,
            wikidata_qid: headers.clean_str(row, "wikidata"),
            wikipedia_url: headers.clean_str(row, "wikipedia_url"),
            country,
            raw_latitude,
            raw_longitude,
            assessment,
            raw: headers.raw_json(row),
        });
    }

    // ---- cross-row signals (informational) ----
    let mut by_point: HashMap<(i64, i64), usize> = HashMap::new();
    let mut by_qid: HashMap<&str, usize> = HashMap::new();
    for e in &entries {
        if let Some(p) = point_key(&e.assessment) {
            *by_point.entry(p).or_default() += 1;
        }
        if let Some(q) = e.wikidata_qid.as_deref() {
            *by_qid.entry(q).or_default() += 1;
        }
    }
    let shared_point: Vec<bool> = entries
        .iter()
        .map(|e| point_key(&e.assessment).is_some_and(|p| by_point[&p] > 1))
        .collect();
    let shared_qid: Vec<bool> = entries
        .iter()
        .map(|e| e.wikidata_qid.as_deref().is_some_and(|q| by_qid[q] > 1))
        .collect();
    for (i, e) in entries.iter_mut().enumerate() {
        if shared_point[i] {
            e.assessment.push_issue(issue::SHARED_COORDINATES);
        }
        if shared_qid[i] {
            e.assessment.push_issue(issue::SHARED_WIKIDATA);
        }
    }

    // ---- store ----
    for e in &entries {
        let issues: Vec<String> = e.assessment.issues.iter().map(|s| s.to_string()).collect();
        let inserted: bool = sqlx::query_scalar(
            r#"
            INSERT INTO gazetteer_entries (
                normalized_name, original_name, matched_name, wikidata_qid, wikipedia_url, country,
                raw_latitude, raw_longitude, latitude, longitude, coordinate_status, issues,
                source_file, source_sheet, source_row, raw
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ON CONFLICT (normalized_name) DO UPDATE SET
                original_name     = EXCLUDED.original_name,
                matched_name      = EXCLUDED.matched_name,
                wikidata_qid      = EXCLUDED.wikidata_qid,
                wikipedia_url     = EXCLUDED.wikipedia_url,
                country           = EXCLUDED.country,
                raw_latitude      = EXCLUDED.raw_latitude,
                raw_longitude     = EXCLUDED.raw_longitude,
                latitude          = EXCLUDED.latitude,
                longitude         = EXCLUDED.longitude,
                coordinate_status = EXCLUDED.coordinate_status,
                issues            = EXCLUDED.issues,
                source_file       = EXCLUDED.source_file,
                source_sheet      = EXCLUDED.source_sheet,
                source_row        = EXCLUDED.source_row,
                raw               = EXCLUDED.raw
            RETURNING (xmax = 0)
            "#,
        )
        .bind(&e.normalized_name)
        .bind(&e.original_name)
        .bind(e.matched_name.as_deref())
        .bind(e.wikidata_qid.as_deref())
        .bind(e.wikipedia_url.as_deref())
        .bind(e.country.as_deref())
        .bind(e.raw_latitude.as_deref())
        .bind(e.raw_longitude.as_deref())
        .bind(e.assessment.latitude)
        .bind(e.assessment.longitude)
        .bind(e.assessment.status.as_str())
        .bind(&issues)
        .bind(source_file)
        .bind(sheet)
        .bind(e.excel_row as i32)
        .bind(sqlx::types::Json(&e.raw))
        .fetch_one(&mut *tx)
        .await
        .with_context(|| format!("{sheet} row {}: upsert gazetteer entry", e.excel_row))?;

        let g = &mut report.gazetteer;
        if inserted {
            g.inserted += 1;
        } else {
            g.updated += 1;
        }
        match e.assessment.status {
            CoordinateStatus::Accepted => g.accepted += 1,
            CoordinateStatus::NeedsReview => g.needs_review += 1,
            CoordinateStatus::Rejected => g.rejected += 1,
        }
        if e.assessment.status != CoordinateStatus::Accepted {
            g.not_used.push(format!(
                "{:<13} {} → {} ({}, {}) [{}]",
                e.assessment.status.as_str(),
                e.original_name,
                e.matched_name.as_deref().unwrap_or("?"),
                e.raw_latitude.as_deref().unwrap_or("-"),
                e.raw_longitude.as_deref().unwrap_or("-"),
                e.assessment.issues.join(", "),
            ));
        }
        if e.assessment.issues.contains(&issue::NAME_MISMATCH) {
            g.name_mismatches += 1;
        }
    }

    Ok(())
}

/// ~11 m grid key for "the same point".
fn point_key(a: &geo::Assessment) -> Option<(i64, i64)> {
    let (lat, lon) = a.latitude.zip(a.longitude)?;
    Some(((lat * 1e4).round() as i64, (lon * 1e4).round() as i64))
}

/// Bring `places` in line with the accepted gazetteer entries. Runs at the end
/// of every import (also appointment-only ones, so newly created places pick up
/// coordinates from an earlier gazetteer import).
pub(crate) async fn sync_places(tx: &mut PgConnection, report: &mut ImportReport) -> Result<()> {
    // 1. withdraw gazetteer points that are no longer accepted
    let withdrawn = sqlx::query(
        r#"
        UPDATE places p
           SET latitude = NULL, longitude = NULL, coordinate_source = NULL
         WHERE p.coordinate_source = 'gazetteer'
           AND NOT EXISTS (
                SELECT 1 FROM gazetteer_entries g
                 WHERE g.normalized_name = p.normalized_name
                   AND g.coordinate_status = 'accepted')
        "#,
    )
    .execute(&mut *tx)
    .await
    .context("withdraw gazetteer coordinates")?
    .rows_affected();

    // 2. apply accepted points (never over coordinates from appointment rows)
    let applied = sqlx::query(
        r#"
        UPDATE places p
           SET latitude = g.latitude, longitude = g.longitude, coordinate_source = 'gazetteer'
          FROM gazetteer_entries g
         WHERE g.normalized_name = p.normalized_name
           AND g.coordinate_status = 'accepted'
           AND (p.coordinate_source IS NULL OR p.coordinate_source = 'gazetteer')
           AND (p.latitude IS DISTINCT FROM g.latitude OR p.longitude IS DISTINCT FROM g.longitude)
        "#,
    )
    .execute(&mut *tx)
    .await
    .context("apply gazetteer coordinates")?
    .rows_affected();

    // 3. wikidata ids from accepted entries fill empty slots only
    sqlx::query(
        r#"
        UPDATE places p
           SET wikidata_qid = g.wikidata_qid
          FROM gazetteer_entries g
         WHERE g.normalized_name = p.normalized_name
           AND g.coordinate_status = 'accepted'
           AND p.wikidata_qid IS NULL
           AND g.wikidata_qid ~ '^Q[1-9][0-9]*$'
        "#,
    )
    .execute(&mut *tx)
    .await
    .context("apply gazetteer wikidata ids")?;

    // 4. report disagreements with coordinates from appointment rows
    let conflicts: Vec<(String, f64)> = sqlx::query_as(
        r#"
        SELECT p.canonical_name,
               ST_DistanceSphere(p.geom, ST_SetSRID(ST_MakePoint(g.longitude, g.latitude), 4326)) / 1000.0
          FROM places p
          JOIN gazetteer_entries g ON g.normalized_name = p.normalized_name
         WHERE g.coordinate_status = 'accepted'
           AND p.coordinate_source = 'source_row'
           AND ST_DistanceSphere(p.geom, ST_SetSRID(ST_MakePoint(g.longitude, g.latitude), 4326))
               > $1 * 1000.0
         ORDER BY p.canonical_name
        "#,
    )
    .bind(SAME_PLACE_KM)
    .fetch_all(&mut *tx)
    .await
    .context("compare gazetteer with row coordinates")?;
    for (name, km) in &conflicts {
        report.warnings.push(format!(
            "place `{name}`: gazetteer point is {km:.0} km from the coordinates on its \
             appointment rows; kept the row coordinates"
        ));
    }

    let (entries_linked, entries_unlinked): (i64, i64) = sqlx::query_as(
        r#"
        SELECT count(*) FILTER (WHERE EXISTS (SELECT 1 FROM places p WHERE p.normalized_name = g.normalized_name)),
               count(*) FILTER (WHERE NOT EXISTS (SELECT 1 FROM places p WHERE p.normalized_name = g.normalized_name))
          FROM gazetteer_entries g
        "#,
    )
    .fetch_one(&mut *tx)
    .await
    .context("count gazetteer links")?;

    let g = &mut report.gazetteer;
    g.coordinates_applied = applied;
    g.coordinates_withdrawn = withdrawn;
    g.conflicts = conflicts.len() as u64;
    g.entries_linked = entries_linked;
    g.entries_unlinked = entries_unlinked;
    Ok(())
}

fn column_letter(mut idx: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (idx % 26) as u8) as char);
        if idx < 26 {
            break;
        }
        idx = idx / 26 - 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::column_letter;

    #[test]
    fn column_letters() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(7), "H");
        assert_eq!(column_letter(26), "AA");
    }
}
