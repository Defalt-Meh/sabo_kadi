//! `GET /api/v1/places` and `GET /api/v1/places/{id}`.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{resolve_page, Page, PersonRef, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::normalize::normalize_key;

#[derive(Debug, Deserialize)]
pub struct PlaceQuery {
    pub query: Option<String>,
    /// Alias for `query`.
    pub q: Option<String>,
    pub has_coordinates: Option<bool>,
    pub has_wikidata: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlaceListItem {
    pub id: i64,
    pub canonical_name: String,
    pub normalized_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// `source_row` | `gazetteer`; `None` when the place has no coordinates.
    pub coordinate_source: Option<String>,
    pub wikidata_qid: Option<String>,
    pub inflow_count: i64,
    pub outflow_count: i64,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<PlaceQuery>,
) -> ApiResult<Json<Page<PlaceListItem>>> {
    let page = resolve_page(q.limit, q.offset, &state)?;
    let name_needle = q
        .query
        .as_deref()
        .or(q.q.as_deref())
        .and_then(normalize_key);

    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT count(*) FROM places p");
    apply_filters(&mut count_qb, &q, name_needle.as_deref());
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT p.id, p.canonical_name, p.normalized_name, p.latitude, p.longitude, \
         p.coordinate_source, p.wikidata_qid, \
         (SELECT count(*) FROM appointments a WHERE a.destination_place_id = p.id) AS inflow_count, \
         (SELECT count(*) FROM appointments a WHERE a.origin_place_id = p.id) AS outflow_count \
         FROM places p",
    );
    apply_filters(&mut qb, &q, name_needle.as_deref());
    qb.push(" ORDER BY p.canonical_name, p.id LIMIT ");
    qb.push_bind(page.limit);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset);

    let items = qb
        .build_query_as::<PlaceListItem>()
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(Page::new(items, total, page)))
}

fn apply_filters<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    q: &'a PlaceQuery,
    name_needle: Option<&'a str>,
) {
    let mut first = true;
    let mut clause = |qb: &mut QueryBuilder<'a, Postgres>| {
        qb.push(if first { " WHERE " } else { " AND " });
        first = false;
    };

    if let Some(needle) = name_needle {
        clause(qb);
        qb.push("p.normalized_name LIKE '%' || ")
            .push_bind(needle)
            .push(" || '%'");
    }
    match q.has_coordinates {
        Some(true) => {
            clause(qb);
            qb.push("p.geom IS NOT NULL");
        }
        Some(false) => {
            clause(qb);
            qb.push("p.geom IS NULL");
        }
        None => {}
    }
    match q.has_wikidata {
        Some(true) => {
            clause(qb);
            qb.push("p.wikidata_qid IS NOT NULL");
        }
        Some(false) => {
            clause(qb);
            qb.push("p.wikidata_qid IS NULL");
        }
        None => {}
    }
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PlaceDetail {
    pub id: i64,
    pub canonical_name: String,
    pub normalized_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub coordinate_source: Option<String>,
    pub wikidata_qid: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// The gazetteer row delivered for this name, with the importer's verdict
    /// (explains why a place has — or lacks — coordinates).
    pub gazetteer: Option<super::gazetteer::GazetteerEntry>,
    pub names: Vec<PlaceName>,
    pub stats: PlaceStats,
    /// Persons connected to this place, most active first (capped).
    pub related_persons: Vec<RelatedPerson>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlaceName {
    pub name: String,
    pub valid_from: Option<i32>,
    pub valid_to: Option<i32>,
    pub calendar: String,
    pub language: Option<String>,
    pub source_reference: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlaceStats {
    pub inbound_appointments: i64,
    pub outbound_appointments: i64,
    pub distinct_persons: i64,
}

#[derive(Debug, Serialize)]
pub struct RelatedPerson {
    #[serde(flatten)]
    pub person: PersonRef,
    pub arrived_here: i64,
    pub left_here: i64,
}

#[derive(sqlx::FromRow)]
struct PlaceRow {
    id: i64,
    canonical_name: String,
    normalized_name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    coordinate_source: Option<String>,
    wikidata_qid: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct StatsRow {
    inbound_appointments: i64,
    outbound_appointments: i64,
    distinct_persons: i64,
}

#[derive(sqlx::FromRow)]
struct RelatedPersonRow {
    id: i64,
    canonical_name: String,
    resolution_status: String,
    arrived_here: i64,
    left_here: i64,
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PlaceDetail>> {
    let place = sqlx::query_as::<_, PlaceRow>(
        "SELECT id, canonical_name, normalized_name, latitude, longitude, coordinate_source, \
         wikidata_qid, created_at, updated_at FROM places WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found(format!("place {id} not found")))?;

    let gazetteer = super::gazetteer::for_name(&state, &place.normalized_name).await?;

    let names = sqlx::query_as::<_, PlaceName>(
        "SELECT name, valid_from, valid_to, calendar::text AS calendar, language, source_reference \
         FROM place_names WHERE place_id = $1 \
         ORDER BY valid_from NULLS FIRST, name",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let stats = sqlx::query_as::<_, StatsRow>(
        r#"
        SELECT
            (SELECT count(*) FROM appointments WHERE destination_place_id = $1) AS inbound_appointments,
            (SELECT count(*) FROM appointments WHERE origin_place_id = $1)      AS outbound_appointments,
            (SELECT count(DISTINCT pid) FROM (
                SELECT new_person_id AS pid FROM appointments
                    WHERE destination_place_id = $1 AND new_person_id IS NOT NULL
                UNION
                SELECT old_person_id AS pid FROM appointments
                    WHERE origin_place_id = $1 AND old_person_id IS NOT NULL
            ) u) AS distinct_persons
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let related = sqlx::query_as::<_, RelatedPersonRow>(
        r#"
        SELECT pr.id, pr.canonical_name, pr.resolution_status,
               count(*) FILTER (WHERE a.destination_place_id = $1 AND a.new_person_id = pr.id) AS arrived_here,
               count(*) FILTER (WHERE a.origin_place_id = $1      AND a.old_person_id = pr.id) AS left_here
        FROM persons pr
        JOIN appointments a
          ON (a.destination_place_id = $1 AND a.new_person_id = pr.id)
          OR (a.origin_place_id = $1      AND a.old_person_id = pr.id)
        GROUP BY pr.id, pr.canonical_name, pr.resolution_status
        ORDER BY (count(*)) DESC, pr.canonical_name
        LIMIT 50
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let related_persons = related
        .into_iter()
        .map(|r| RelatedPerson {
            person: PersonRef {
                id: r.id,
                canonical_name: r.canonical_name,
                resolution_status: r.resolution_status,
            },
            arrived_here: r.arrived_here,
            left_here: r.left_here,
        })
        .collect();

    Ok(Json(PlaceDetail {
        id: place.id,
        canonical_name: place.canonical_name,
        normalized_name: place.normalized_name,
        latitude: place.latitude,
        longitude: place.longitude,
        coordinate_source: place.coordinate_source,
        wikidata_qid: place.wikidata_qid,
        created_at: place.created_at,
        updated_at: place.updated_at,
        gazetteer,
        names,
        stats: PlaceStats {
            inbound_appointments: stats.inbound_appointments,
            outbound_appointments: stats.outbound_appointments,
            distinct_persons: stats.distinct_persons,
        },
        related_persons,
    }))
}
