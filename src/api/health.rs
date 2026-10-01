//! `GET /api/v1/health` — liveness + database reachability.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::app::AppState;
use crate::db;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    database: &'static str,
}

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    match db::ping(&state.pool).await {
        Ok(()) => (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                database: "ok",
            }),
        ),
        Err(err) => {
            tracing::warn!(error = ?err, "health check: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    database: "unreachable",
                }),
            )
        }
    }
}
