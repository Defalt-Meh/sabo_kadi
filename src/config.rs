//! Runtime configuration, sourced entirely from environment variables.
//!
//! A `.env` file in the working directory is loaded first (if present) as a
//! convenience for local development; real deployments set the variables in the
//! process environment.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

/// Prefix for service-specific variables, e.g. `KADI_ATLAS__BIND_ADDR`.
const PREFIX: &str = "KADI_ATLAS__";

#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL/PostGIS connection string.
    pub database_url: String,
    /// Maximum size of the connection pool.
    pub database_max_connections: u32,
    /// How long to wait for a free connection before erroring.
    pub database_acquire_timeout: Duration,
    /// Address the HTTP server binds to.
    pub bind_addr: SocketAddr,
    /// Default page size when a request omits `limit`.
    pub default_page_size: i64,
    /// Hard upper bound for `limit`.
    pub max_page_size: i64,
    /// Per-request timeout for HTTP handlers.
    pub request_timeout: Duration,
    /// Run embedded migrations on startup.
    pub auto_migrate: bool,
    /// `pretty` (default) or `json` structured logs.
    pub log_format: LogFormat,
    /// Optional single allowed CORS origin. When unset the API is same-origin only.
    pub cors_allow_origin: Option<String>,
    /// Optional directory with the static frontend (`web/`). When set, it is
    /// served at `/` on the same origin as the API, so no CORS is needed.
    pub web_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

impl Config {
    /// Load configuration, applying `.env` first when present.
    pub fn load() -> Result<Self> {
        let _ = dotenvy::dotenv();
        Self::from_env()
    }

    pub fn from_env() -> Result<Self> {
        let database_url = var("DATABASE_URL")
            .or_else(|| env::var("DATABASE_URL").ok())
            .context("DATABASE_URL (or KADI_ATLAS__DATABASE_URL) must be set")?;

        let database_max_connections = parse_var("DATABASE_MAX_CONNECTIONS", 10u32)?;
        let database_acquire_timeout =
            Duration::from_secs(parse_var("DATABASE_ACQUIRE_TIMEOUT_SECS", 10u64)?);

        let bind_addr: SocketAddr = var("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:8080".to_string())
            .parse()
            .context("KADI_ATLAS__BIND_ADDR must be a valid socket address, e.g. 127.0.0.1:8080")?;

        let default_page_size = parse_var("DEFAULT_PAGE_SIZE", 50i64)?;
        let max_page_size = parse_var("MAX_PAGE_SIZE", 200i64)?;
        if default_page_size < 1 || max_page_size < 1 {
            return Err(anyhow!("page-size settings must be >= 1"));
        }
        if default_page_size > max_page_size {
            return Err(anyhow!(
                "KADI_ATLAS__DEFAULT_PAGE_SIZE ({default_page_size}) must not exceed KADI_ATLAS__MAX_PAGE_SIZE ({max_page_size})"
            ));
        }

        let request_timeout = Duration::from_secs(parse_var("REQUEST_TIMEOUT_SECS", 30u64)?);
        let auto_migrate = parse_bool_var("AUTO_MIGRATE", true)?;

        let log_format = match var("LOG_FORMAT").as_deref() {
            None | Some("pretty") => LogFormat::Pretty,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(anyhow!(
                    "KADI_ATLAS__LOG_FORMAT must be `pretty` or `json`, got `{other}`"
                ))
            }
        };

        let cors_allow_origin = var("CORS_ALLOW_ORIGIN").filter(|s| !s.is_empty());

        let web_dir = var("WEB_DIR").map(PathBuf::from);
        if let Some(dir) = &web_dir {
            if !dir.join("index.html").is_file() {
                return Err(anyhow!(
                    "KADI_ATLAS__WEB_DIR ({}) must be a directory containing index.html",
                    dir.display()
                ));
            }
        }

        Ok(Self {
            database_url,
            database_max_connections,
            database_acquire_timeout,
            bind_addr,
            default_page_size,
            max_page_size,
            request_timeout,
            auto_migrate,
            log_format,
            cors_allow_origin,
            web_dir,
        })
    }
}

fn var(name: &str) -> Option<String> {
    env::var(format!("{PREFIX}{name}"))
        .ok()
        .filter(|s| !s.is_empty())
}

fn parse_var<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match var(name) {
        None => Ok(default),
        Some(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow!("{PREFIX}{name} is invalid: {e}")),
    }
}

fn parse_bool_var(name: &str, default: bool) -> Result<bool> {
    match var(name).map(|s| s.to_ascii_lowercase()) {
        None => Ok(default),
        Some(s) if ["1", "true", "yes", "on"].contains(&s.as_str()) => Ok(true),
        Some(s) if ["0", "false", "no", "off"].contains(&s.as_str()) => Ok(false),
        Some(other) => Err(anyhow!("{PREFIX}{name} must be a boolean, got `{other}`")),
    }
}
