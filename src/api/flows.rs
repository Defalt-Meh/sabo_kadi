//! `GET /api/v1/flows` — origin -> destination movement aggregation for maps.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::filters::{Clauses, FilterParams, Filters};
use super::{resolve_page, Page, PlaceRef, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

/// Endpoint-specific parameters; the filters are [`FilterParams`].
#[derive(Debug, Deserialize)]
pub struct FlowQuery {
    /// Minimum aggregated count for a flow to be returned (default 1).
    pub min_count: Option<i64>,
    /// Only include flows where both endpoints have coordinates (default true).
    pub require_coordinates: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Flow {
    pub origin: PlaceRef,
    pub destination: PlaceRef,
    pub count: i64,
    /// Latest dated appointment on this flow (within the filters); `None` when
    /// none of its appointments has a numeric year. The map fades old flows.
    pub last_year: Option<i32>,
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
    last_year: Option<i32>,
}

const FROM: &str = r#"
    FROM appointments a
    JOIN places o ON o.id = a.origin_place_id
    JOIN places d ON d.id = a.destination_place_id
"#;

pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
    Query(q): Query<FlowQuery>,
) -> ApiResult<Json<Page<Flow>>> {
    let filters = filter.resolve()?;
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
    apply_filters(&mut count_qb, &filters, require_coords);
    count_qb.push(group_by).push_bind(min_count);
    count_qb.push(") t");
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT o.id AS origin_id, o.canonical_name AS origin_name, \
         o.latitude AS origin_lat, o.longitude AS origin_lon, \
         d.id AS dest_id, d.canonical_name AS dest_name, \
         d.latitude AS dest_lat, d.longitude AS dest_lon, \
         count(*) AS count, max(a.year_numeric) AS last_year {FROM}"
    ));
    apply_filters(&mut qb, &filters, require_coords);
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
            last_year: r.last_year,
        })
        .collect();

    Ok(Json(Page::new(items, total, page)))
}

fn apply_filters(qb: &mut QueryBuilder<'_, Postgres>, filters: &Filters, require_coords: bool) {
    let mut clauses = Clauses::new();
    filters.apply(qb, &mut clauses);
    if require_coords {
        clauses.next(qb);
        qb.push("o.geom IS NOT NULL AND d.geom IS NOT NULL");
    }
}
