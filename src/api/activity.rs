//! `GET /api/v1/place-activity` — geocoded places with arrival / departure
//! counts under the same filters as `/flows`, for the map's place view.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::filters::{Clauses, FilterParams};
use super::{Page, PageParams, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

/// Places are small rows and a map wants all of them at once, so this endpoint
/// has its own, larger cap instead of the global page size.
const MAX_LIMIT: i64 = 5000;

/// Endpoint-specific parameters; the filters are [`FilterParams`].
#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlaceActivity {
    pub id: i64,
    pub canonical_name: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Appointments with this place as destination (`new_place`).
    pub arrivals: i64,
    /// Appointments with this place as origin (`old_place`).
    pub departures: i64,
    #[serde(skip)]
    pub total_count: i64,
}

pub async fn places(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
    Query(q): Query<ActivityQuery>,
) -> ApiResult<Json<Page<PlaceActivity>>> {
    let filters = filter.resolve()?;

    let limit = match q.limit {
        None => MAX_LIMIT,
        Some(l) if (1..=MAX_LIMIT).contains(&l) => l,
        Some(_) => {
            return Err(ApiError::bad_request(format!(
                "`limit` must be between 1 and {MAX_LIMIT}"
            )))
        }
    };
    let offset = match q.offset {
        None => 0,
        Some(o) if (0..=10_000_000).contains(&o) => o,
        Some(_) => {
            return Err(ApiError::bad_request(
                "`offset` must be between 0 and 10000000",
            ))
        }
    };

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "WITH a AS (SELECT a.origin_place_id, a.destination_place_id FROM appointments a",
    );
    filters.apply(&mut qb, &mut Clauses::new());
    qb.push(
        "), visits AS ( \
             SELECT destination_place_id AS place_id, 1 AS arrived, 0 AS departed FROM a \
             WHERE destination_place_id IS NOT NULL \
             UNION ALL \
             SELECT origin_place_id, 0, 1 FROM a WHERE origin_place_id IS NOT NULL \
         ) \
         SELECT p.id, p.canonical_name, p.latitude, p.longitude, \
                sum(v.arrived)::bigint AS arrivals, sum(v.departed)::bigint AS departures, \
                count(*) OVER () AS total_count \
         FROM visits v JOIN places p ON p.id = v.place_id \
         WHERE p.latitude IS NOT NULL AND p.longitude IS NOT NULL \
         GROUP BY p.id, p.canonical_name, p.latitude, p.longitude \
         ORDER BY count(*) DESC, p.canonical_name, p.id LIMIT ",
    );
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    let items = qb
        .build_query_as::<PlaceActivity>()
        .fetch_all(&state.pool)
        .await?;
    let total = items.first().map_or(0, |p| p.total_count);

    Ok(Json(Page::new(items, total, PageParams { limit, offset })))
}
