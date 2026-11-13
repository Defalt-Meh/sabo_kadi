//! `GET /api/v1/stats` — grouped counts for charts (top places, top kadıs,
//! records per year / decade, vocabulary distributions) and
//! `GET /api/v1/stats/summary` — headline numbers for a filter set.
//!
//! Both take the shared [`FilterParams`], so a chart always describes exactly
//! the records the map, the table and the CSV export show.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::filters::{Clauses, FilterParams, Filters, GEOCODED_BOTH};
use super::Query;
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

const DEFAULT_LIMIT: i64 = 10;
const MAX_LIMIT: i64 = 200;
/// Timelines are returned whole (one row per year at most).
const MAX_TIME_BUCKETS: i64 = 2_000;

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    /// What to group by: `destination` (new place), `origin` (old place),
    /// `place` (either), `new_kadi` (the appointed kadı), `old_kadi` (the
    /// replaced kadı), `person` (either), `year`, `decade`, `degree`,
    /// `position_type`.
    pub by: Option<String>,
    /// Top-N for the ranking groupings (default 10, max 200). Ignored for
    /// `year` / `decade`, which return every bucket in time order.
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupBy {
    Destination,
    Origin,
    Place,
    NewKadi,
    OldKadi,
    Person,
    Year,
    Decade,
    Degree,
    PositionType,
}

impl GroupBy {
    fn parse(raw: Option<&str>) -> ApiResult<Self> {
        Ok(match raw.unwrap_or("destination") {
            "destination" => Self::Destination,
            "origin" => Self::Origin,
            "place" => Self::Place,
            "new_kadi" => Self::NewKadi,
            "old_kadi" => Self::OldKadi,
            "person" => Self::Person,
            "year" => Self::Year,
            "decade" => Self::Decade,
            "degree" => Self::Degree,
            "position_type" => Self::PositionType,
            _ => {
                return Err(ApiError::bad_request(
                    "`by` must be one of: destination, origin, place, new_kadi, old_kadi, person, \
                     year, decade, degree, position_type",
                ))
            }
        })
    }

    fn is_time(self) -> bool {
        matches!(self, Self::Year | Self::Decade)
    }

    /// The grouped `SELECT` over the filtered CTE `a`. Fixed SQL only.
    fn sql(self) -> &'static str {
        match self {
            Self::Destination => {
                "SELECT p.id, p.canonical_name AS label, NULL::int AS value, count(*) AS count, \
                 count(*) OVER () AS groups \
                 FROM a JOIN places p ON p.id = a.destination_place_id \
                 GROUP BY p.id, p.canonical_name ORDER BY count(*) DESC, p.canonical_name"
            }
            Self::Origin => {
                "SELECT p.id, p.canonical_name AS label, NULL::int AS value, count(*) AS count, \
                 count(*) OVER () AS groups \
                 FROM a JOIN places p ON p.id = a.origin_place_id \
                 GROUP BY p.id, p.canonical_name ORDER BY count(*) DESC, p.canonical_name"
            }
            // a record counts once per place even when old = new place
            Self::Place => {
                "SELECT p.id, p.canonical_name AS label, NULL::int AS value, \
                 count(DISTINCT a.id) AS count, count(*) OVER () AS groups \
                 FROM a CROSS JOIN LATERAL (VALUES (a.origin_place_id), (a.destination_place_id)) x(pid) \
                 JOIN places p ON p.id = x.pid \
                 GROUP BY p.id, p.canonical_name ORDER BY count(DISTINCT a.id) DESC, p.canonical_name"
            }
            Self::NewKadi => {
                "SELECT p.id, p.canonical_name AS label, NULL::int AS value, count(*) AS count, \
                 count(*) OVER () AS groups \
                 FROM a JOIN persons p ON p.id = a.new_person_id \
                 GROUP BY p.id, p.canonical_name ORDER BY count(*) DESC, p.canonical_name"
            }
            Self::OldKadi => {
                "SELECT p.id, p.canonical_name AS label, NULL::int AS value, count(*) AS count, \
                 count(*) OVER () AS groups \
                 FROM a JOIN persons p ON p.id = a.old_person_id \
                 GROUP BY p.id, p.canonical_name ORDER BY count(*) DESC, p.canonical_name"
            }
            Self::Person => {
                "SELECT p.id, p.canonical_name AS label, NULL::int AS value, \
                 count(DISTINCT a.id) AS count, count(*) OVER () AS groups \
                 FROM a CROSS JOIN LATERAL (VALUES (a.old_person_id), (a.new_person_id)) x(pid) \
                 JOIN persons p ON p.id = x.pid \
                 GROUP BY p.id, p.canonical_name ORDER BY count(DISTINCT a.id) DESC, p.canonical_name"
            }
            Self::Year => {
                "SELECT NULL::bigint AS id, NULL::text AS label, a.year_numeric AS value, \
                 count(*) AS count, count(*) OVER () AS groups \
                 FROM a WHERE a.year_numeric IS NOT NULL \
                 GROUP BY a.year_numeric ORDER BY a.year_numeric"
            }
            Self::Decade => {
                "SELECT NULL::bigint AS id, NULL::text AS label, (a.year_numeric / 10) * 10 AS value, \
                 count(*) AS count, count(*) OVER () AS groups \
                 FROM a WHERE a.year_numeric IS NOT NULL \
                 GROUP BY 3 ORDER BY 3"
            }
            Self::Degree => {
                "SELECT NULL::bigint AS id, a.degree AS label, NULL::int AS value, count(*) AS count, \
                 count(*) OVER () AS groups \
                 FROM a GROUP BY a.degree ORDER BY count(*) DESC, a.degree NULLS LAST"
            }
            Self::PositionType => {
                "SELECT NULL::bigint AS id, a.position_type AS label, NULL::int AS value, \
                 count(*) AS count, count(*) OVER () AS groups \
                 FROM a GROUP BY a.position_type ORDER BY count(*) DESC, a.position_type NULLS LAST"
            }
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StatItem {
    /// Place or person id for the entity groupings.
    pub id: Option<i64>,
    /// Display label; `None` for time buckets and for empty vocabulary values.
    pub label: Option<String>,
    /// Year (or first year of the decade) for time buckets.
    pub value: Option<i32>,
    /// Number of records in the group.
    pub count: i64,
    #[serde(skip)]
    pub groups: i64,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub by: String,
    pub items: Vec<StatItem>,
    /// Number of distinct groups before the top-N cut.
    pub groups: i64,
    /// Records matching the filters.
    pub records: i64,
    /// Matching records without a numeric year (absent from time buckets).
    pub undated: i64,
}

/// `WITH a AS (filtered appointments)` — every stats query reads from it.
fn filtered(filters: &Filters) -> QueryBuilder<'static, Postgres> {
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("WITH a AS (SELECT a.* FROM appointments a");
    filters.apply(&mut qb, &mut Clauses::new());
    qb.push(") ");
    qb
}

pub async fn grouped(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Json<Stats>> {
    let filters = filter.resolve()?;
    let by = GroupBy::parse(q.by.as_deref())?;
    let limit = match q.limit {
        _ if by.is_time() => MAX_TIME_BUCKETS,
        None => DEFAULT_LIMIT,
        Some(l) if (1..=MAX_LIMIT).contains(&l) => l,
        Some(_) => {
            return Err(ApiError::bad_request(format!(
                "`limit` must be between 1 and {MAX_LIMIT}"
            )))
        }
    };

    let mut qb = filtered(&filters);
    qb.push(by.sql()).push(" LIMIT ").push_bind(limit);
    let items = qb
        .build_query_as::<StatItem>()
        .fetch_all(&state.pool)
        .await?;

    let mut totals = filtered(&filters);
    totals.push("SELECT count(*), count(*) FILTER (WHERE a.year_numeric IS NULL) FROM a");
    let (records, undated): (i64, i64) = totals.build_query_as().fetch_one(&state.pool).await?;

    Ok(Json(Stats {
        by: q.by.unwrap_or_else(|| "destination".to_string()),
        groups: items.first().map_or(0, |i| i.groups),
        items,
        records,
        undated,
    }))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Summary {
    /// Records matching the filters.
    pub records: i64,
    /// … of which both places have validated coordinates (drawable as arrows).
    pub mapped: i64,
    /// … of which the old place is known at all.
    pub with_origin: i64,
    /// … without a numeric year.
    pub undated: i64,
    /// Distinct persons (either role) and places (either end) involved.
    pub persons: i64,
    pub places: i64,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
}

pub async fn summary(
    State(state): State<AppState>,
    Query(filter): Query<FilterParams>,
) -> ApiResult<Json<Summary>> {
    let filters = filter.resolve()?;
    let mut qb = filtered(&filters);
    qb.push(format!(
        "SELECT count(*) AS records, \
         count(*) FILTER (WHERE {GEOCODED_BOTH}) AS mapped, \
         count(*) FILTER (WHERE a.origin_place_id IS NOT NULL) AS with_origin, \
         count(*) FILTER (WHERE a.year_numeric IS NULL) AS undated, \
         (SELECT count(DISTINCT x.pid) FROM a, LATERAL (VALUES (a.old_person_id), (a.new_person_id)) x(pid)) AS persons, \
         (SELECT count(DISTINCT x.pid) FROM a, LATERAL (VALUES (a.origin_place_id), (a.destination_place_id)) x(pid)) AS places, \
         min(a.year_numeric) AS year_min, max(a.year_numeric) AS year_max \
         FROM a"
    ));
    let summary = qb
        .build_query_as::<Summary>()
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(summary))
}
