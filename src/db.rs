//! Database pool construction and migrations.

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;

/// Embedded migrations from the `migrations/` directory.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Build a connection pool from configuration.
pub async fn connect(cfg: &Config) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(cfg.database_max_connections)
        .acquire_timeout(cfg.database_acquire_timeout)
        .idle_timeout(Some(Duration::from_secs(600)))
        .test_before_acquire(true)
        .connect(&cfg.database_url)
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
