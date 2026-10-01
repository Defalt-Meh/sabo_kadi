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
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Build from an arbitrary variable source (the process environment in
    /// production, a fixed map in tests).
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let database_url = var(&lookup, "DATABASE_URL")
            .or_else(|| lookup("DATABASE_URL"))
            .context("DATABASE_URL (or KADI_ATLAS__DATABASE_URL) must be set")?;

        let database_max_connections = parse_var(&lookup, "DATABASE_MAX_CONNECTIONS", 10u32)?;
        let database_acquire_timeout =
            Duration::from_secs(parse_var(&lookup, "DATABASE_ACQUIRE_TIMEOUT_SECS", 10u64)?);

        let bind_addr: SocketAddr = var(&lookup, "BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:8080".to_string())
            .parse()
            .context("KADI_ATLAS__BIND_ADDR must be a valid socket address, e.g. 127.0.0.1:8080")?;

        let default_page_size = parse_var(&lookup, "DEFAULT_PAGE_SIZE", 50i64)?;
        let max_page_size = parse_var(&lookup, "MAX_PAGE_SIZE", 200i64)?;
        if default_page_size < 1 || max_page_size < 1 {
            return Err(anyhow!("page-size settings must be >= 1"));
        }
        if default_page_size > max_page_size {
            return Err(anyhow!(
                "KADI_ATLAS__DEFAULT_PAGE_SIZE ({default_page_size}) must not exceed KADI_ATLAS__MAX_PAGE_SIZE ({max_page_size})"
            ));
        }

        let request_timeout = Duration::from_secs(parse_var(&lookup, "REQUEST_TIMEOUT_SECS", 30u64)?);
        let auto_migrate = parse_bool_var(&lookup, "AUTO_MIGRATE", true)?;

        let log_format = match var(&lookup, "LOG_FORMAT").as_deref() {
            None | Some("pretty") => LogFormat::Pretty,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(anyhow!(
                    "KADI_ATLAS__LOG_FORMAT must be `pretty` or `json`, got `{other}`"
                ))
            }
        };

        let cors_allow_origin = var(&lookup, "CORS_ALLOW_ORIGIN").filter(|s| !s.is_empty());

        let web_dir = var(&lookup, "WEB_DIR").map(PathBuf::from);
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

fn var(lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    lookup(&format!("{PREFIX}{name}")).filter(|s| !s.is_empty())
}

fn parse_var<T>(lookup: &impl Fn(&str) -> Option<String>, name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match var(lookup, name) {
        None => Ok(default),
        Some(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow!("{PREFIX}{name} is invalid: {e}")),
    }
}

fn parse_bool_var(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &str,
    default: bool,
) -> Result<bool> {
    match var(lookup, name).map(|s| s.to_ascii_lowercase()) {
        None => Ok(default),
        Some(s) if ["1", "true", "yes", "on"].contains(&s.as_str()) => Ok(true),
        Some(s) if ["0", "false", "no", "off"].contains(&s.as_str()) => Ok(false),
        Some(other) => Err(anyhow!("{PREFIX}{name} must be a boolean, got `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn load(pairs: &[(&str, &str)]) -> Result<Config> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|key| map.get(key).cloned())
    }

    fn err(pairs: &[(&str, &str)]) -> String {
        format!("{:#}", load(pairs).unwrap_err())
    }

    const DB: (&str, &str) = ("DATABASE_URL", "postgres://u:p@localhost/db");

    #[test]
    fn defaults_apply_when_only_database_url_is_set() {
        let cfg = load(&[DB]).unwrap();
        assert_eq!(cfg.database_url, "postgres://u:p@localhost/db");
        assert_eq!(cfg.database_max_connections, 10);
        assert_eq!(cfg.database_acquire_timeout, Duration::from_secs(10));
        assert_eq!(cfg.bind_addr, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(cfg.default_page_size, 50);
        assert_eq!(cfg.max_page_size, 200);
        assert_eq!(cfg.request_timeout, Duration::from_secs(30));
        assert!(cfg.auto_migrate);
        assert_eq!(cfg.log_format, LogFormat::Pretty);
        assert_eq!(cfg.cors_allow_origin, None);
        assert_eq!(cfg.web_dir, None);
    }

    #[test]
    fn database_url_is_required() {
        assert!(err(&[]).contains("DATABASE_URL"));
    }

    #[test]
    fn prefixed_database_url_wins_over_plain() {
        let cfg = load(&[DB, ("KADI_ATLAS__DATABASE_URL", "postgres://prefixed/db")]).unwrap();
        assert_eq!(cfg.database_url, "postgres://prefixed/db");
    }

    #[test]
    fn empty_values_count_as_unset() {
        let cfg = load(&[
            DB,
            ("KADI_ATLAS__MAX_PAGE_SIZE", ""),
            ("KADI_ATLAS__CORS_ALLOW_ORIGIN", ""),
        ])
        .unwrap();
        assert_eq!(cfg.max_page_size, 200);
        assert_eq!(cfg.cors_allow_origin, None);
    }

    #[test]
    fn overrides_are_parsed() {
        let cfg = load(&[
            DB,
            ("KADI_ATLAS__BIND_ADDR", "0.0.0.0:9000"),
            ("KADI_ATLAS__DATABASE_MAX_CONNECTIONS", "3"),
            ("KADI_ATLAS__DEFAULT_PAGE_SIZE", "5"),
            ("KADI_ATLAS__MAX_PAGE_SIZE", "5"),
            ("KADI_ATLAS__REQUEST_TIMEOUT_SECS", "2"),
            ("KADI_ATLAS__LOG_FORMAT", "json"),
            ("KADI_ATLAS__CORS_ALLOW_ORIGIN", "https://example.org"),
        ])
        .unwrap();
        assert_eq!(cfg.bind_addr, "0.0.0.0:9000".parse().unwrap());
        assert_eq!(cfg.database_max_connections, 3);
        assert_eq!(cfg.default_page_size, 5);
        assert_eq!(cfg.max_page_size, 5);
        assert_eq!(cfg.request_timeout, Duration::from_secs(2));
        assert_eq!(cfg.log_format, LogFormat::Json);
        assert_eq!(cfg.cors_allow_origin.as_deref(), Some("https://example.org"));
    }

    #[test]
    fn booleans_accept_common_spellings() {
        for (raw, expected) in [
            ("1", true),
            ("TRUE", true),
            ("yes", true),
            ("On", true),
            ("0", false),
            ("false", false),
            ("NO", false),
            ("off", false),
        ] {
            let cfg = load(&[DB, ("KADI_ATLAS__AUTO_MIGRATE", raw)]).unwrap();
            assert_eq!(cfg.auto_migrate, expected, "AUTO_MIGRATE={raw}");
        }
        assert!(err(&[DB, ("KADI_ATLAS__AUTO_MIGRATE", "maybe")]).contains("must be a boolean"));
    }

    #[test]
    fn invalid_values_are_rejected_with_the_variable_name() {
        assert!(err(&[DB, ("KADI_ATLAS__BIND_ADDR", "localhost")]).contains("BIND_ADDR"));
        assert!(err(&[DB, ("KADI_ATLAS__DATABASE_MAX_CONNECTIONS", "-1")])
            .contains("KADI_ATLAS__DATABASE_MAX_CONNECTIONS is invalid"));
        assert!(err(&[DB, ("KADI_ATLAS__LOG_FORMAT", "xml")]).contains("`pretty` or `json`"));
    }

    #[test]
    fn page_sizes_are_validated() {
        assert!(err(&[DB, ("KADI_ATLAS__DEFAULT_PAGE_SIZE", "0")]).contains(">= 1"));
        assert!(err(&[DB, ("KADI_ATLAS__MAX_PAGE_SIZE", "-5")]).contains(">= 1"));
        assert!(err(&[
            DB,
            ("KADI_ATLAS__DEFAULT_PAGE_SIZE", "100"),
            ("KADI_ATLAS__MAX_PAGE_SIZE", "10"),
        ])
        .contains("must not exceed"));
    }

    #[test]
    fn web_dir_must_contain_index_html() {
        let missing = std::env::temp_dir().join("kadi-atlas-config-test-does-not-exist");
        assert!(err(&[DB, ("KADI_ATLAS__WEB_DIR", missing.to_str().unwrap())])
            .contains("must be a directory containing index.html"));

        let web = concat!(env!("CARGO_MANIFEST_DIR"), "/web");
        let cfg = load(&[DB, ("KADI_ATLAS__WEB_DIR", web)]).unwrap();
        assert_eq!(cfg.web_dir, Some(PathBuf::from(web)));
    }
}
