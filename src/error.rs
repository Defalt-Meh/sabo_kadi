//! Uniform API error type.
//!
//! Every error returned to a client has the shape:
//! ```json
//! { "error": { "code": "not_found", "message": "person 42 not found" } }
//! ```
//! Internal details (SQL, connection errors, panics) are logged with `tracing`
//! and never forwarded to the client.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotFound(String),

    /// The database is unreachable / not ready.
    #[error("service unavailable")]
    Unavailable,

    /// Any unexpected internal failure. The inner error is logged, not exposed.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::NotFound(_) => "not_found",
            Self::Unavailable => "service_unavailable",
            Self::Internal(_) => "internal",
        }
    }

    fn client_message(&self) -> String {
        match self {
            Self::BadRequest(m) | Self::NotFound(m) => m.clone(),
            Self::Unavailable => "the service is temporarily unavailable".to_string(),
            Self::Internal(_) => "an internal error occurred".to_string(),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound("resource not found".to_string()),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => Self::Unavailable,
            other => Self::Internal(anyhow::Error::new(other)),
        }
    }
}

/// Error payload. The nested `error` object is canonical; `code` and `message`
/// are also mirrored at the top level so simpler clients can read them directly.
#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        match &self {
            ApiError::Internal(err) => {
                tracing::error!(error = ?err, "internal error while handling request");
            }
            ApiError::Unavailable => {
                tracing::warn!("request failed: service unavailable");
            }
            _ => {
                tracing::debug!(status = %status, error = %self, "request rejected");
            }
        }

        let code = self.code();
        let message = self.client_message();
        let body = ErrorBody {
            error: ErrorDetail {
                code,
                message: message.clone(),
            },
            code,
            message,
        };

        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
