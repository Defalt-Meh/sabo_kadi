//! HTTP application assembly: shared state, middleware stack, router.

use std::time::Duration;

use axum::extract::MatchedPath;
use axum::http::{header, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use axum::Router;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::api;
use crate::config::Config;

/// Initialize the global tracing subscriber from configuration.
///
/// Safe to call once per process; a second call is a no-op.
pub fn init_tracing(format: crate::config::LogFormat) {
    use tracing_subscriber::prelude::*;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,kadi_atlas=info,tower_http=info,sqlx=warn")
    });

    let registry = tracing_subscriber::registry().with(filter);
    match format {
        crate::config::LogFormat::Json => {
            let _ = registry
                .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
                .try_init();
        }
        crate::config::LogFormat::Pretty => {
            let _ = registry
                .with(tracing_subscriber::fmt::layer().with_target(true))
                .try_init();
        }
    }
}

/// Cheap-to-clone application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub default_page_size: i64,
    pub max_page_size: i64,
}

impl AppState {
    pub fn new(pool: PgPool, cfg: &Config) -> Self {
        Self {
            pool,
            default_page_size: cfg.default_page_size,
            max_page_size: cfg.max_page_size,
        }
    }
}

/// Build the full application router with the middleware stack applied.
pub fn build_router(state: AppState, cfg: &Config) -> Router {
    let request_timeout = cfg.request_timeout;

    let security_headers = ServiceBuilder::new()
        .layer(static_header(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(static_header(
            HeaderName::from_static("x-frame-options"),
            "DENY",
        ))
        // Pages served from `web_dir` set their own, looser policy first; this
        // one only fills in when absent, i.e. for API and error responses.
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(API_CSP),
        ))
        .layer(static_header(
            HeaderName::from_static("referrer-policy"),
            "no-referrer",
        ))
        .layer(static_header(
            HeaderName::from_static("permissions-policy"),
            "geolocation=(), camera=(), microphone=(), interest-cohort=()",
        ))
        .layer(static_header(
            HeaderName::from_static("cross-origin-resource-policy"),
            "same-site",
        ));

    let middleware = ServiceBuilder::new()
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
                let path = req
                    .extensions()
                    .get::<MatchedPath>()
                    .map(MatchedPath::as_str)
                    .unwrap_or_else(|| req.uri().path());
                tracing::info_span!("http", method = %req.method(), path = %path)
            }),
        )
        .layer(security_headers)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            request_timeout,
        ))
        .option_layer(cors_layer(cfg));

    let router = Router::new()
        .nest("/api/v1", api::router())
        // unknown API paths stay JSON 404s even when the frontend is served
        .route("/api", axum::routing::any(not_found))
        .route("/api/{*rest}", axum::routing::any(not_found))
        .method_not_allowed_fallback(method_not_allowed);

    let router = match &cfg.web_dir {
        Some(dir) => {
            let pages = ServiceBuilder::new()
                .layer(static_header(
                    HeaderName::from_static("content-security-policy"),
                    WEB_CSP,
                ))
                .service(ServeDir::new(dir).append_index_html_on_directories(true));
            router.fallback_service(pages)
        }
        None => router.fallback(not_found),
    };

    router.layer(middleware).with_state(state)
}

const API_CSP: &str =
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// Policy for the bundled frontend: own scripts only, Google Fonts for type,
/// API calls to the same origin.
const WEB_CSP: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' https://fonts.googleapis.com; style-src-attr 'unsafe-inline'; \
     font-src 'self' https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; \
     object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

fn static_header(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(name, HeaderValue::from_static(value))
}

/// Same-origin by default. A CORS layer is added only when an explicit origin is
/// configured, and it permits read-only methods only.
fn cors_layer(cfg: &Config) -> Option<CorsLayer> {
    let origin = cfg.cors_allow_origin.as_ref()?;
    let value = HeaderValue::from_str(origin).ok()?;
    Some(
        CorsLayer::new()
            .allow_origin(value)
            .allow_methods([Method::GET, Method::HEAD])
            .allow_headers([header::CONTENT_TYPE]),
    )
}

fn json_error(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({
            "error": {"code": code, "message": message},
            "code": code,
            "message": message,
        })),
    )
        .into_response()
}

async fn not_found() -> impl IntoResponse {
    json_error(StatusCode::NOT_FOUND, "not_found", "no such endpoint")
}

async fn method_not_allowed() -> impl IntoResponse {
    json_error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method not allowed",
    )
}

fn handle_panic(err: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    let detail = err
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string());
    tracing::error!(panic = %detail, "handler panicked");
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "an internal error occurred",
    )
}

/// Small helper used by tests and `main` to keep the pool warm.
pub async fn warm_pool(pool: &PgPool) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), crate::db::ping(pool)).await??;
    Ok(())
}
