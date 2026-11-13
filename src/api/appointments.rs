//! `GET /api/v1/appointments` — the central event list with filters.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::filters::{Clauses, FilterParams, Filters};
use super::{resolve_page, Page, PersonRef, PlaceRef, Query};
use crate::app::AppState;
use crate::calendar::gregorian_label;
use crate::error::{ApiError, ApiResult};

/// Endpoint-specific parameters; the filters themselves are [`FilterParams`].
#[derive(Debug, Deserialize)]
pub struct AppointmentQuery {
    /// `year` (default), `year_desc`, `origin`, `destination`, `old_kadi`,
    /// `new_kadi`, `degree`, `position_type`.
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `ORDER BY` for each `sort` value. Fixed strings, never user input.
fn order_by(sort: Option<&str>) -> ApiResult<&'static str> {
    Ok(match sort.unwrap_or("year") {
        "year" => " ORDER BY a.year_numeric NULLS LAST, a.id",
        "year_desc" => " ORDER BY a.year_numeric DESC NULLS LAST, a.id DESC",
        "origin" => {
            " ORDER BY lower(coalesce(o.canonical_name, a.raw_old_place)) NULLS LAST, a.year_numeric NULLS LAST, a.id"
        }
        "destination" => {
            " ORDER BY lower(coalesce(d.canonical_name, a.raw_new_place)) NULLS LAST, a.year_numeric NULLS LAST, a.id"
        }
        "old_kadi" => {
            " ORDER BY lower(coalesce(po.canonical_name, a.raw_old_kadi)) NULLS LAST, a.year_numeric NULLS LAST, a.id"
        }
        "new_kadi" => {
            " ORDER BY lower(coalesce(pn.canonical_name, a.raw_new_kadi)) NULLS LAST, a.year_numeric NULLS LAST, a.id"
        }
        "degree" => " ORDER BY a.degree NULLS LAST, a.year_numeric NULLS LAST, a.id",
        "position_type" => " ORDER BY a.position_type NULLS LAST, a.year_numeric NULLS LAST, a.id",
        _ => {
            return Err(ApiError::bad_request(
                "`sort` must be one of: year, year_desc, origin, destination, old_kadi, new_kadi, degree, position_type",
            ))
        }
    })
}

#[derive(Debug, Serialize)]
pub struct Appointment {
    pub id: i64,
    pub source_record_id: i64,
    pub source_doc_id: String,
    pub varak_no: Option<String>,

    pub year_original: Option<String>,
    pub year_numeric: Option<i32>,
    pub calendar: String,

    pub degree: Option<String>,
    pub position_type: Option<String>,
    pub period: Option<String>,
    pub salary: Option<String>,
    pub old_salary: Option<String>,
    pub asitane: Option<String>,
    pub infisal: Option<String>,
    pub region_raw: Option<String>,

    pub raw_old_kadi: Option<String>,
    pub raw_new_kadi: Option<String>,
    pub raw_old_place: Option<String>,
    pub raw_new_place: Option<String>,

    pub old_person: Option<PersonRef>,
    pub new_person: Option<PersonRef>,
    pub origin: Option<PlaceRef>,
    pub destination: Option<PlaceRef>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    source_record_id: i64,
    source_doc_id: String,
    varak_no: Option<String>,
    year_original: Option<String>,
    year_numeric: Option<i32>,
    calendar: String,
    degree: Option<String>,
    position_type: Option<String>,
    period: Option<String>,
    salary: Option<String>,
    old_salary: Option<String>,
    asitane: Option<String>,
    infisal: Option<String>,
    region_raw: Option<String>,
    raw_old_kadi: Option<String>,
    raw_new_kadi: Option<String>,
    raw_old_place: Option<String>,
    raw_new_place: Option<String>,
    old_person_id: Option<i64>,
    old_person_name: Option<String>,
    old_person_status: Option<String>,
    new_person_id: Option<i64>,
    new_person_name: Option<String>,
    new_person_status: Option<String>,
    origin_id: Option<i64>,
    origin_name: Option<String>,
    origin_lat: Option<f64>,
    origin_lon: Option<f64>,
    dest_id: Option<i64>,
    dest_name: Option<String>,
    dest_lat: Option<f64>,
    dest_lon: Option<f64>,
}

const SELECT: &str = r#"
    a.id, a.source_record_id, s.doc_id AS source_doc_id, s.varak_no,
    a.year_original, a.year_numeric, a.calendar::text AS calendar,
    a.degree, a.position_type, a.period, a.salary, a.old_salary,
    a.asitane, a.infisal, a.region_raw,
    a.raw_old_kadi, a.raw_new_kadi, a.raw_old_place, a.raw_new_place,
    po.id AS old_person_id, po.canonical_name AS old_person_name, po.resolution_status AS old_person_status,
    pn.id AS new_person_id, pn.canonical_name AS new_person_name, pn.resolution_status AS new_person_status,
    o.id AS origin_id, o.canonical_name AS origin_name, o.latitude AS origin_lat, o.longitude AS origin_lon,
    d.id AS dest_id,   d.canonical_name AS dest_name,   d.latitude AS dest_lat,   d.longitude AS dest_lon
"#;

const FROM: &str = r#"
    FROM appointments a
    JOIN source_records s ON s.id = a.source_record_id
    LEFT JOIN persons po ON po.id = a.old_person_id
    LEFT JOIN persons pn ON pn.id = a.new_person_id
    LEFT JOIN places  o  ON o.id  = a.origin_place_id
    LEFT JOIN places  d  ON d.id  = a.destination_place_id
"#;

pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
    Query(q): Query<AppointmentQuery>,
) -> ApiResult<Json<Page<Appointment>>> {
    let filters = filter.resolve()?;
    let order = order_by(q.sort.as_deref())?;
    let page = resolve_page(q.limit, q.offset, &state)?;

    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT count(*) FROM appointments a");
    filters.apply(&mut count_qb, &mut Clauses::new());
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb = select(&filters);
    qb.push(order);
    qb.push(" LIMIT ");
    qb.push_bind(page.limit);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset);

    let rows = qb.build_query_as::<Row>().fetch_all(&state.pool).await?;
    let items = rows.into_iter().map(map_row).collect();

    Ok(Json(Page::new(items, total, page)))
}

fn select(filters: &Filters) -> QueryBuilder<'static, Postgres> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!("SELECT {SELECT} {FROM}"));
    filters.apply(&mut qb, &mut Clauses::new());
    qb
}

// ---------------------------------------------------------------------------
// Matching ids
// ---------------------------------------------------------------------------

/// Cap for `/appointments/ids`; far above any one person's record count.
const MAX_IDS: i64 = 20_000;

#[derive(Debug, Serialize)]
pub struct IdList {
    pub ids: Vec<i64>,
    /// Number of matching records (may exceed `ids.len()` at the cap).
    pub total: i64,
}

/// `GET /api/v1/appointments/ids` — just the ids of the matching records.
/// The map uses it to mark which steps of a kadı's itinerary match the
/// sidebar filters, without downloading every record again.
pub async fn ids(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
) -> ApiResult<Json<IdList>> {
    let filters = filter.resolve()?;

    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT count(*) FROM appointments a");
    filters.apply(&mut count_qb, &mut Clauses::new());
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT a.id FROM appointments a");
    filters.apply(&mut qb, &mut Clauses::new());
    qb.push(" ORDER BY a.id LIMIT ").push_bind(MAX_IDS);
    let ids = qb.build_query_scalar().fetch_all(&state.pool).await?;

    Ok(Json(IdList { ids, total }))
}

// ---------------------------------------------------------------------------
// CSV export
// ---------------------------------------------------------------------------

/// Upper bound for one export (the whole corpus is expected to stay below it).
const MAX_EXPORT_ROWS: i64 = 150_000;

const CSV_HEADER: &[&str] = &[
    "appointment_id",
    "doc_id",
    "varak_no",
    "date_original",
    "year_numeric",
    "gregorian_approx",
    "old_kadi_raw",
    "old_kadi",
    "new_kadi_raw",
    "new_kadi",
    "old_place_raw",
    "old_place",
    "old_place_lat",
    "old_place_lon",
    "new_place_raw",
    "new_place",
    "new_place_lat",
    "new_place_lon",
    "degree",
    "position_type",
    "period",
    "salary",
    "old_salary",
    "asitane",
    "infisal",
    "region",
];

/// `GET /api/v1/export/appointments.csv` — every record matching the filters
/// (same parameters and `sort` as `/appointments`), as UTF-8 CSV with a BOM so
/// spreadsheet programs read the Turkish letters correctly. Source texts are
/// never included.
pub async fn export_csv(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
    Query(q): Query<AppointmentQuery>,
) -> ApiResult<Response> {
    let filters = filter.resolve()?;
    let order = order_by(q.sort.as_deref())?;

    let mut qb = select(&filters);
    qb.push(order);
    qb.push(" LIMIT ").push_bind(MAX_EXPORT_ROWS);
    let rows = qb.build_query_as::<Row>().fetch_all(&state.pool).await?;

    let mut out = String::with_capacity(64 + rows.len() * 320);
    out.push('\u{feff}');
    push_csv_line(&mut out, CSV_HEADER.iter().map(|h| Some((*h).to_string())));

    for r in rows {
        let num = |v: Option<f64>| v.map(|x| x.to_string());
        push_csv_line(
            &mut out,
            [
                Some(r.id.to_string()),
                Some(r.source_doc_id),
                r.varak_no,
                r.year_original,
                r.year_numeric.map(|y| y.to_string()),
                r.year_numeric.map(gregorian_label),
                r.raw_old_kadi,
                r.old_person_name,
                r.raw_new_kadi,
                r.new_person_name,
                r.raw_old_place,
                r.origin_name,
                num(r.origin_lat),
                num(r.origin_lon),
                r.raw_new_place,
                r.dest_name,
                num(r.dest_lat),
                num(r.dest_lon),
                r.degree,
                r.position_type,
                r.period,
                r.salary,
                r.old_salary,
                r.asitane,
                r.infisal,
                r.region_raw,
            ]
            .into_iter(),
        );
    }

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"kadi-atlas-atamalar.csv\"",
            ),
        ],
        out,
    )
        .into_response())
}

/// One RFC 4180 line. Fields that a spreadsheet would read as a formula
/// (`=`, `+`, `-`, `@` first) get a leading apostrophe.
fn push_csv_line(out: &mut String, fields: impl Iterator<Item = Option<String>>) {
    for (i, field) in fields.enumerate() {
        if i > 0 {
            out.push(',');
        }
        let Some(value) = field else { continue };
        let guarded = if value.starts_with(['=', '+', '-', '@']) && value.parse::<f64>().is_err() {
            format!("'{value}")
        } else {
            value
        };
        if guarded.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&guarded.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(&guarded);
        }
    }
    out.push_str("\r\n");
}

fn map_row(r: Row) -> Appointment {
    Appointment {
        id: r.id,
        source_record_id: r.source_record_id,
        source_doc_id: r.source_doc_id,
        varak_no: r.varak_no,
        year_original: r.year_original,
        year_numeric: r.year_numeric,
        calendar: r.calendar,
        degree: r.degree,
        position_type: r.position_type,
        period: r.period,
        salary: r.salary,
        old_salary: r.old_salary,
        asitane: r.asitane,
        infisal: r.infisal,
        region_raw: r.region_raw,
        raw_old_kadi: r.raw_old_kadi,
        raw_new_kadi: r.raw_new_kadi,
        raw_old_place: r.raw_old_place,
        raw_new_place: r.raw_new_place,
        old_person: PersonRef::from_parts(r.old_person_id, r.old_person_name, r.old_person_status),
        new_person: PersonRef::from_parts(r.new_person_id, r.new_person_name, r.new_person_status),
        origin: PlaceRef::from_parts(r.origin_id, r.origin_name, r.origin_lat, r.origin_lon),
        destination: PlaceRef::from_parts(r.dest_id, r.dest_name, r.dest_lat, r.dest_lon),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: &[Option<&str>]) -> String {
        let mut out = String::new();
        push_csv_line(&mut out, fields.iter().map(|f| f.map(str::to_string)));
        out
    }

    #[test]
    fn csv_quoting() {
        assert_eq!(line(&[Some("a"), None, Some("b")]), "a,,b\r\n");
        assert_eq!(
            line(&[Some("x, y"), Some("say \"hi\"")]),
            "\"x, y\",\"say \"\"hi\"\"\"\r\n"
        );
        assert_eq!(line(&[Some("=cmd()"), Some("-12.5")]), "'=cmd(),-12.5\r\n");
    }

    #[test]
    fn sort_whitelist() {
        assert!(order_by(None).unwrap().contains("year_numeric NULLS LAST"));
        assert!(order_by(Some("destination")).is_ok());
        assert!(order_by(Some("id; DROP TABLE x")).is_err());
    }
}
