//! `GET /api/v1/gazetteer` — every delivered place → coordinate row with the
//! importer's verdict, for reviewing the third-party coordinates.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{resolve_page, Page, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::normalize::normalize_key;

const STATUSES: &[&str] = &["accepted", "needs_review", "rejected"];

#[derive(Debug, Deserialize)]
pub struct GazetteerQuery {
    /// `accepted` | `needs_review` | `rejected`
    pub status: Option<String>,
    /// Only entries carrying this issue (e.g. `outside_study_region`).
    pub issue: Option<String>,
    pub query: Option<String>,
    /// Alias for `query`.
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GazetteerEntry {
    pub id: i64,
    pub original_name: String,
    pub matched_name: Option<String>,
    pub wikidata_qid: Option<String>,
    pub wikipedia_url: Option<String>,
    pub country: Option<String>,
    pub raw_latitude: Option<String>,
    pub raw_longitude: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub coordinate_status: String,
    pub issues: Vec<String>,
    /// The place this name resolves to, if the corpus mentions it.
    pub place_id: Option<i64>,
    pub source_file: String,
    pub source_sheet: String,
    pub source_row: i32,
}

const SELECT: &str =
    "SELECT g.id, g.original_name, g.matched_name, g.wikidata_qid, g.wikipedia_url, \
     g.country, g.raw_latitude, g.raw_longitude, g.latitude, g.longitude, g.coordinate_status, \
     g.issues, p.id AS place_id, g.source_file, g.source_sheet, g.source_row \
     FROM gazetteer_entries g LEFT JOIN places p ON p.normalized_name = g.normalized_name";

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<GazetteerQuery>,
) -> ApiResult<Json<Page<GazetteerEntry>>> {
    let page = resolve_page(q.limit, q.offset, &state)?;
    if let Some(status) = q.status.as_deref() {
        if !STATUSES.contains(&status) {
            return Err(ApiError::bad_request(format!(
                "`status` must be one of {}",
                STATUSES.join(", ")
            )));
        }
    }
    let needle = q
        .query
        .as_deref()
        .or(q.q.as_deref())
        .and_then(normalize_key);

    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT count(*) FROM gazetteer_entries g");
    apply_filters(&mut count_qb, &q, needle.as_deref());
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(SELECT);
    apply_filters(&mut qb, &q, needle.as_deref());
    qb.push(" ORDER BY g.original_name, g.id LIMIT ");
    qb.push_bind(page.limit);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset);
    let items = qb
        .build_query_as::<GazetteerEntry>()
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(Page::new(items, total, page)))
}

fn apply_filters<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    q: &'a GazetteerQuery,
    needle: Option<&'a str>,
) {
    let mut first = true;
    let mut clause = |qb: &mut QueryBuilder<'a, Postgres>| {
        qb.push(if first { " WHERE " } else { " AND " });
        first = false;
    };
    if let Some(status) = q.status.as_deref() {
        clause(qb);
        qb.push("g.coordinate_status = ").push_bind(status);
    }
    if let Some(issue) = q.issue.as_deref() {
        clause(qb);
        qb.push("").push_bind(issue).push(" = ANY (g.issues)");
    }
    if let Some(needle) = needle {
        clause(qb);
        qb.push("g.normalized_name LIKE '%' || ")
            .push_bind(needle)
            .push(" || '%'");
    }
}

/// The gazetteer entry for a place's normalized name, if one was delivered.
pub async fn for_name(
    state: &AppState,
    normalized_name: &str,
) -> ApiResult<Option<GazetteerEntry>> {
    let sql = format!("{SELECT} WHERE g.normalized_name = $1");
    Ok(sqlx::query_as::<_, GazetteerEntry>(&sql)
        .bind(normalized_name)
        .fetch_optional(&state.pool)
        .await?)
}
