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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    async fn render(err: ApiError) -> (StatusCode, Value) {
        let response = err.into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn client_errors_carry_their_message() {
        let (status, body) = render(ApiError::not_found("person 42 not found")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body,
            json!({
                "error": { "code": "not_found", "message": "person 42 not found" },
                "code": "not_found",
                "message": "person 42 not found",
            })
        );

        let (status, body) = render(ApiError::bad_request("bad limit")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "bad_request");
        assert_eq!(body["error"]["message"], "bad limit");
    }

    #[tokio::test]
    async fn internal_details_are_never_exposed() {
        let secret = "password authentication failed for user kadi";
        let (status, body) = render(ApiError::Internal(anyhow::anyhow!(secret))).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "internal");
        assert_eq!(body["message"], "an internal error occurred");
        assert!(!body.to_string().contains("password"));
    }

    #[tokio::test]
    async fn unavailable_is_503() {
        let (status, body) = render(ApiError::Unavailable).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "service_unavailable");
    }

    #[test]
    fn sqlx_errors_map_to_api_errors() {
        assert!(matches!(
            ApiError::from(sqlx::Error::RowNotFound),
            ApiError::NotFound(_)
        ));
        assert!(matches!(
            ApiError::from(sqlx::Error::PoolTimedOut),
            ApiError::Unavailable
        ));
        assert!(matches!(
            ApiError::from(sqlx::Error::PoolClosed),
            ApiError::Unavailable
        ));
        assert!(matches!(
            ApiError::from(sqlx::Error::Protocol("boom".into())),
            ApiError::Internal(_)
        ));
    }
}
