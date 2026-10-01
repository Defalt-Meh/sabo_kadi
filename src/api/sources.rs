//! `GET /api/v1/sources` (list + full-text search) and `GET /api/v1/sources/{id}`.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{resolve_page, Page, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Deserialize)]
pub struct SourceQuery {
    /// Full-text search over `source_text` (PostgreSQL `websearch_to_tsquery`).
    pub query: Option<String>,
    /// Alias for `query`.
    pub q: Option<String>,
    pub doc_id: Option<String>,
    /// Alias for `doc_id`.
    pub external_id: Option<String>,
    /// Filter to a single record by its numeric id.
    pub source_record_id: Option<i64>,
    pub varak_no: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl SourceQuery {
    fn doc_id_filter(&self) -> Option<&str> {
        self.doc_id.as_deref().or(self.external_id.as_deref())
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SourceListItem {
    pub id: i64,
    pub doc_id: String,
    pub varak_no: Option<String>,
    pub certificate: Option<String>,
    /// Highlighted fragment when searching, otherwise a leading excerpt.
    pub snippet: Option<String>,
    /// `ts_rank` relevance; `None` when not searching.
    pub rank: Option<f32>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<SourceQuery>,
) -> ApiResult<Json<Page<SourceListItem>>> {
    let page = resolve_page(q.limit, q.offset, &state)?;
    let search = q
        .query
        .as_deref()
        .or(q.q.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // ----- count -----
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT count(*) FROM source_records s");
    push_filters(&mut count_qb, search, &q);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    // ----- page -----
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT s.id, s.doc_id, s.varak_no, s.certificate, ");
    match search {
        Some(term) => {
            qb.push(
                "ts_headline('simple', coalesce(s.source_text, ''), websearch_to_tsquery('simple', ",
            )
            .push_bind(term)
            .push("), 'MaxFragments=2,MinWords=5,MaxWords=18,StartSel=<mark>,StopSel=</mark>') AS snippet, ")
            .push("ts_rank(s.source_text_tsv, websearch_to_tsquery('simple', ")
            .push_bind(term)
            .push(")) AS rank FROM source_records s");
        }
        None => {
            qb.push(
                "left(s.source_text, 240) AS snippet, NULL::real AS rank FROM source_records s",
            );
        }
    }
    push_filters(&mut qb, search, &q);
    match search {
        Some(_) => qb.push(" ORDER BY rank DESC, s.doc_id"),
        None => qb.push(" ORDER BY s.doc_id"),
    };
    qb.push(" LIMIT ").push_bind(page.limit);
    qb.push(" OFFSET ").push_bind(page.offset);

    let items = qb
        .build_query_as::<SourceListItem>()
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(Page::new(items, total, page)))
}

fn push_filters<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    search: Option<&'a str>,
    q: &'a SourceQuery,
) {
    let mut first = true;
    let mut clause = |qb: &mut QueryBuilder<'a, Postgres>| {
        qb.push(if first { " WHERE " } else { " AND " });
        first = false;
    };

    if let Some(term) = search {
        clause(qb);
        qb.push("s.source_text_tsv @@ websearch_to_tsquery('simple', ")
            .push_bind(term)
            .push(")");
    }
    if let Some(doc_id) = q.doc_id_filter() {
        clause(qb);
        qb.push("s.doc_id = ").push_bind(doc_id);
    }
    if let Some(id) = q.source_record_id {
        clause(qb);
        qb.push("s.id = ").push_bind(id);
    }
    if let Some(varak_no) = &q.varak_no {
        clause(qb);
        qb.push("s.varak_no = ").push_bind(varak_no.as_str());
    }
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SourceDetail {
    pub id: i64,
    pub doc_id: String,
    pub varak_no: Option<String>,
    pub certificate: Option<String>,
    pub source_text: Option<String>,
    /// The original spreadsheet row, preserved verbatim.
    pub raw: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Appointment event(s) derived from this record (normally exactly one).
    pub appointment_ids: Vec<i64>,
}

#[derive(sqlx::FromRow)]
struct DetailRow {
    id: i64,
    doc_id: String,
    varak_no: Option<String>,
    certificate: Option<String>,
    source_text: Option<String>,
    raw: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<SourceDetail>> {
    let row = sqlx::query_as::<_, DetailRow>(
        "SELECT id, doc_id, varak_no, certificate, source_text, raw, created_at, updated_at \
         FROM source_records WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found(format!("source {id} not found")))?;

    let appointment_ids = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM appointments WHERE source_record_id = $1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(SourceDetail {
        id: row.id,
        doc_id: row.doc_id,
        varak_no: row.varak_no,
        certificate: row.certificate,
        source_text: row.source_text,
        raw: row.raw,
        created_at: row.created_at,
        updated_at: row.updated_at,
        appointment_ids,
    }))
}
