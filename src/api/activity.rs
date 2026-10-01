//! `GET /api/v1/place-activity` — geocoded places with arrival / departure
//! counts under the same filters as `/flows`, for the map's place view.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{validate_year_range, Page, PageParams, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

/// Places are small rows and a map wants all of them at once, so this endpoint
/// has its own, larger cap instead of the global page size.
const MAX_LIMIT: i64 = 5000;

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub person: Option<i64>,
    /// Alias for `person`.
    pub person_id: Option<i64>,
    pub place: Option<i64>,
    /// Alias for `place`.
    pub place_id: Option<i64>,
    pub degree: Option<String>,
    pub position_type: Option<String>,
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
    Query(q): Query<ActivityQuery>,
) -> ApiResult<Json<Page<PlaceActivity>>> {
    validate_year_range(q.year_from, q.year_to)?;

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
    apply_filters(&mut qb, &q);
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

fn apply_filters<'a>(qb: &mut QueryBuilder<'a, Postgres>, q: &'a ActivityQuery) {
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
    if let Some(person) = q.person.or(q.person_id) {
        clause(qb);
        qb.push("(a.old_person_id = ")
            .push_bind(person)
            .push(" OR a.new_person_id = ")
            .push_bind(person)
            .push(")");
    }
    if let Some(place) = q.place.or(q.place_id) {
        clause(qb);
        qb.push("(a.origin_place_id = ")
            .push_bind(place)
            .push(" OR a.destination_place_id = ")
            .push_bind(place)
            .push(")");
    }
    if let Some(degree) = &q.degree {
        clause(qb);
        qb.push("a.degree = ").push_bind(degree.as_str());
    }
    if let Some(position) = &q.position_type {
        clause(qb);
        qb.push("a.position_type = ").push_bind(position.as_str());
    }
}
