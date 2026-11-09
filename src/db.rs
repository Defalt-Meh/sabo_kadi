//! Database pool construction and migrations.

use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::ConnectOptions;

use crate::config::Config;

/// Embedded migrations from the `migrations/` directory.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Build a connection pool from configuration.
pub async fn connect(cfg: &Config) -> Result<PgPool> {
    connect_with_slow_threshold(cfg, Duration::from_secs(1)).await
}

/// Like [`connect`], but only statements slower than `slow` are logged as
/// warnings. The importer sends multi-thousand-row statements over the network
/// (e.g. to Neon) that legitimately take seconds.
pub async fn connect_with_slow_threshold(cfg: &Config, slow: Duration) -> Result<PgPool> {
    let options = PgConnectOptions::from_str(&cfg.database_url)
        .context("DATABASE_URL is not a valid PostgreSQL connection string")?
        .log_slow_statements(log::LevelFilter::Warn, slow);
    PgPoolOptions::new()
        .max_connections(cfg.database_max_connections)
        .acquire_timeout(cfg.database_acquire_timeout)
        .idle_timeout(Some(Duration::from_secs(600)))
        .test_before_acquire(true)
        .connect_with(options)
        .await
        .context("failed to connect to the database")
}

/// Apply any pending migrations.
pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("database migration failed")?;
    Ok(())
}

/// Cheap liveness probe used by the health endpoint.
pub async fn ping(pool: &PgPool) -> Result<()> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .context("database ping failed")?;
    Ok(())
}
