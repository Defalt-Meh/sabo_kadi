//! `/api/v1` router and shared request/response plumbing.

mod activity;
mod appointments;
mod flows;
mod health;
mod meta;
mod persons;
mod places;
mod sources;

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::request::Parts;
use axum::routing::get;
use axum::Router;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/meta", get(meta::meta))
        .route("/persons", get(persons::list))
        .route("/persons/{id}", get(persons::detail))
        .route("/places", get(places::list))
        .route("/places/{id}", get(places::detail))
        .route("/appointments", get(appointments::list))
        .route("/flows", get(flows::list))
        .route("/place-activity", get(activity::places))
        .route("/sources", get(sources::list))
        .route("/sources/{id}", get(sources::detail))
}

/// Query-string extractor that reports failures as our uniform JSON 400 instead
/// of axum's default plain-text rejection.
pub struct Query<T>(pub T);

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = parts.uri.query().unwrap_or_default();
        let value = serde_urlencoded::from_str(raw)
            .map_err(|e| ApiError::bad_request(format!("invalid query parameters: {e}")))?;
        Ok(Query(value))
    }
}

impl<T, S> OptionalFromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        <Self as FromRequestParts<S>>::from_request_parts(parts, state)
            .await
            .map(Some)
    }
}

/// Validated, clamped pagination values.
///
/// NOTE: `serde_urlencoded` does not support `#[serde(flatten)]`, so each query
/// struct declares its own `limit` / `offset` fields and calls [`resolve_page`].
#[derive(Debug, Clone, Copy)]
pub struct PageParams {
    pub limit: i64,
    pub offset: i64,
}

pub fn resolve_page(
    limit: Option<i64>,
    offset: Option<i64>,
    state: &AppState,
) -> ApiResult<PageParams> {
    let limit = match limit {
        None => state.default_page_size,
        Some(l) if (1..=state.max_page_size).contains(&l) => l,
        Some(_) => {
            return Err(ApiError::bad_request(format!(
                "`limit` must be between 1 and {}",
                state.max_page_size
            )))
        }
    };
    let offset = match offset {
        None => 0,
        Some(o) if (0..=10_000_000).contains(&o) => o,
        Some(_) => {
            return Err(ApiError::bad_request(
                "`offset` must be between 0 and 10000000",
            ))
        }
    };
    Ok(PageParams { limit, offset })
}

/// Validate an inclusive integer year window.
///
/// Year filters are matched against `appointments.year_numeric`, a
/// calendar-agnostic best-effort integer (see architecture.md) — not
/// authoritative Gregorian years.
pub fn validate_year_range(year_from: Option<i32>, year_to: Option<i32>) -> ApiResult<()> {
    if let (Some(from), Some(to)) = (year_from, year_to) {
        if from > to {
            return Err(ApiError::bad_request(
                "`year_from` must not be greater than `year_to`",
            ));
        }
    }
    Ok(())
}

/// Lightweight place reference embedded in other responses. Coordinates are
/// `None` until a historian supplies them.
#[derive(Debug, Serialize)]
pub struct PlaceRef {
    pub id: i64,
    pub canonical_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

impl PlaceRef {
    /// Build from an optional (left-joined) id + columns.
    pub fn from_parts(
        id: Option<i64>,
        name: Option<String>,
        lat: Option<f64>,
        lon: Option<f64>,
    ) -> Option<Self> {
        Some(Self {
            id: id?,
            canonical_name: name.unwrap_or_default(),
            latitude: lat,
            longitude: lon,
        })
    }
}

/// Lightweight person reference embedded in other responses.
#[derive(Debug, Serialize)]
pub struct PersonRef {
    pub id: i64,
    pub canonical_name: String,
    pub resolution_status: String,
}

impl PersonRef {
    pub fn from_parts(
        id: Option<i64>,
        name: Option<String>,
        status: Option<String>,
    ) -> Option<Self> {
        Some(Self {
            id: id?,
            canonical_name: name.unwrap_or_default(),
            resolution_status: status.unwrap_or_default(),
        })
    }
}

/// Standard list envelope.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, total: i64, params: PageParams) -> Self {
        Self {
            items,
            total,
            limit: params.limit,
            offset: params.offset,
        }
    }
}
