//! `GET /api/v1/appointments` — the central event list with filters.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{resolve_page, validate_year_range, Page, PersonRef, PlaceRef, Query};
use crate::app::AppState;
use crate::error::ApiResult;
use crate::normalize::fold_term;

#[derive(Debug, Deserialize)]
pub struct AppointmentQuery {
    /// Person id; matches either the outgoing or incoming kadı.
    pub person: Option<i64>,
    /// Alias for `person`.
    pub person_id: Option<i64>,
    /// Place id; matches either origin or destination.
    pub place: Option<i64>,
    /// Alias for `place`.
    pub place_id: Option<i64>,
    pub origin_place: Option<i64>,
    pub destination_place: Option<i64>,
    /// The appointment(s) derived from one source record.
    pub source_record_id: Option<i64>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub degree: Option<String>,
    pub position_type: Option<String>,
    /// Substring match against the raw region string.
    pub region: Option<String>,
    /// When true, only rows where both origin and destination have coordinates.
    pub has_coordinates: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AppointmentQuery {
    fn person_filter(&self) -> Option<i64> {
        self.person.or(self.person_id)
    }
    fn place_filter(&self) -> Option<i64> {
        self.place.or(self.place_id)
    }
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
    Query(q): Query<AppointmentQuery>,
) -> ApiResult<Json<Page<Appointment>>> {
    validate_year_range(q.year_from, q.year_to)?;
    let page = resolve_page(q.limit, q.offset, &state)?;

    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new(format!("SELECT count(*) {FROM}"));
    apply_filters(&mut count_qb, &q);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!("SELECT {SELECT} {FROM}"));
    apply_filters(&mut qb, &q);
    qb.push(" ORDER BY a.year_numeric NULLS LAST, a.id LIMIT ");
    qb.push_bind(page.limit);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset);

    let rows = qb.build_query_as::<Row>().fetch_all(&state.pool).await?;
    let items = rows.into_iter().map(map_row).collect();

    Ok(Json(Page::new(items, total, page)))
}

fn apply_filters<'a>(qb: &mut QueryBuilder<'a, Postgres>, q: &'a AppointmentQuery) {
    let mut first = true;
    let mut clause = |qb: &mut QueryBuilder<'a, Postgres>| {
        qb.push(if first { " WHERE " } else { " AND " });
        first = false;
    };

    if let Some(person) = q.person_filter() {
        clause(qb);
        qb.push("(a.old_person_id = ")
            .push_bind(person)
            .push(" OR a.new_person_id = ")
            .push_bind(person)
            .push(")");
    }
    if let Some(place) = q.place_filter() {
        clause(qb);
        qb.push("(a.origin_place_id = ")
            .push_bind(place)
            .push(" OR a.destination_place_id = ")
            .push_bind(place)
            .push(")");
    }
    if let Some(origin) = q.origin_place {
        clause(qb);
        qb.push("a.origin_place_id = ").push_bind(origin);
    }
    if let Some(dest) = q.destination_place {
        clause(qb);
        qb.push("a.destination_place_id = ").push_bind(dest);
    }
    if let Some(source) = q.source_record_id {
        clause(qb);
        qb.push("a.source_record_id = ").push_bind(source);
    }
    if let Some(year_from) = q.year_from {
        clause(qb);
        qb.push("a.year_numeric >= ").push_bind(year_from);
    }
    if let Some(year_to) = q.year_to {
        clause(qb);
        qb.push("a.year_numeric <= ").push_bind(year_to);
    }
    if let Some(degree) = &q.degree {
        clause(qb);
        qb.push("a.degree = ")
            .push_bind(fold_term(degree).unwrap_or_default());
    }
    if let Some(position) = &q.position_type {
        clause(qb);
        qb.push("a.position_type = ")
            .push_bind(fold_term(position).unwrap_or_default());
    }
    if let Some(region) = &q.region {
        clause(qb);
        qb.push("a.region_raw ILIKE '%' || ")
            .push_bind(region.as_str())
            .push(" || '%'");
    }
    if q.has_coordinates == Some(true) {
        clause(qb);
        qb.push("o.geom IS NOT NULL AND d.geom IS NOT NULL");
    } else if q.has_coordinates == Some(false) {
        clause(qb);
        qb.push("(o.geom IS NULL OR d.geom IS NULL)");
    }
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
