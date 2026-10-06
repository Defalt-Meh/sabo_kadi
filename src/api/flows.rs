//! `GET /api/v1/flows` — origin -> destination movement aggregation for maps.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{resolve_page, validate_year_range, Page, PlaceRef, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::normalize::fold_term;

#[derive(Debug, Deserialize)]
pub struct FlowQuery {
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    /// Restrict to movements involving this person (as outgoing or incoming kadı).
    pub person: Option<i64>,
    /// Alias for `person`.
    pub person_id: Option<i64>,
    /// Restrict to movements touching this place (as origin or destination).
    pub place: Option<i64>,
    /// Alias for `place`.
    pub place_id: Option<i64>,
    pub degree: Option<String>,
    pub position_type: Option<String>,
    /// Minimum aggregated count for a flow to be returned (default 1).
    pub min_count: Option<i64>,
    /// Only include flows where both endpoints have coordinates (default true).
    pub require_coordinates: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl FlowQuery {
    fn person_filter(&self) -> Option<i64> {
        self.person.or(self.person_id)
    }
    fn place_filter(&self) -> Option<i64> {
        self.place.or(self.place_id)
    }
}

#[derive(Debug, Serialize)]
pub struct Flow {
    pub origin: PlaceRef,
    pub destination: PlaceRef,
    pub count: i64,
}

#[derive(sqlx::FromRow)]
struct Row {
    origin_id: i64,
    origin_name: String,
    origin_lat: Option<f64>,
    origin_lon: Option<f64>,
    dest_id: i64,
    dest_name: String,
    dest_lat: Option<f64>,
    dest_lon: Option<f64>,
    count: i64,
}

const FROM: &str = r#"
    FROM appointments a
    JOIN places o ON o.id = a.origin_place_id
    JOIN places d ON d.id = a.destination_place_id
"#;

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<FlowQuery>,
) -> ApiResult<Json<Page<Flow>>> {
    validate_year_range(q.year_from, q.year_to)?;
    let page = resolve_page(q.limit, q.offset, &state)?;

    let min_count = match q.min_count {
        None => 1,
        Some(n) if n >= 1 => n,
        Some(_) => return Err(ApiError::bad_request("`min_count` must be >= 1")),
    };
    let require_coords = q.require_coordinates.unwrap_or(true);

    let group_by = " GROUP BY o.id, o.canonical_name, o.latitude, o.longitude, \
                     d.id, d.canonical_name, d.latitude, d.longitude \
                     HAVING count(*) >= ";

    // total distinct flows
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new(format!("SELECT count(*) FROM (SELECT 1 {FROM}"));
    apply_filters(&mut count_qb, &q, require_coords);
    count_qb.push(group_by).push_bind(min_count);
    count_qb.push(") t");
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT o.id AS origin_id, o.canonical_name AS origin_name, \
         o.latitude AS origin_lat, o.longitude AS origin_lon, \
         d.id AS dest_id, d.canonical_name AS dest_name, \
         d.latitude AS dest_lat, d.longitude AS dest_lon, \
         count(*) AS count {FROM}"
    ));
    apply_filters(&mut qb, &q, require_coords);
    qb.push(group_by).push_bind(min_count);
    qb.push(" ORDER BY count(*) DESC, o.canonical_name, d.canonical_name LIMIT ");
    qb.push_bind(page.limit);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset);

    let rows = qb.build_query_as::<Row>().fetch_all(&state.pool).await?;
    let items = rows
        .into_iter()
        .map(|r| Flow {
            origin: PlaceRef {
                id: r.origin_id,
                canonical_name: r.origin_name,
                latitude: r.origin_lat,
                longitude: r.origin_lon,
            },
            destination: PlaceRef {
                id: r.dest_id,
                canonical_name: r.dest_name,
                latitude: r.dest_lat,
                longitude: r.dest_lon,
            },
            count: r.count,
        })
        .collect();

    Ok(Json(Page::new(items, total, page)))
}

fn apply_filters<'a>(qb: &mut QueryBuilder<'a, Postgres>, q: &'a FlowQuery, require_coords: bool) {
    let mut first = true;
    let mut clause = |qb: &mut QueryBuilder<'a, Postgres>| {
        qb.push(if first { " WHERE " } else { " AND " });
        first = false;
    };

    if let Some(year_from) = q.year_from {
        clause(qb);
        qb.push("a.year_numeric >= ").push_bind(year_from);
    }
    if let Some(year_to) = q.year_to {
        clause(qb);
        qb.push("a.year_numeric <= ").push_bind(year_to);
    }
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
    if require_coords {
        clause(qb);
        qb.push("o.geom IS NOT NULL AND d.geom IS NOT NULL");
    }
}
