//! Kadı Atlas HTTP server (read-only).

use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use tokio::signal;

use kadi_atlas::app::{build_router, init_tracing, AppState};
use kadi_atlas::{config::Config, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    init_tracing(config.log_format);

    tracing::info!(
        bind = %config.bind_addr,
        max_connections = config.database_max_connections,
        auto_migrate = config.auto_migrate,
        "starting kadi-atlas"
    );

    let pool = db::connect(&config).await?;

    if config.auto_migrate {
        db::migrate(&pool).await?;
        tracing::info!("migrations up to date");
    } else {
        db::ping(&pool).await?;
    }

    let state = AppState::new(pool.clone(), &config);
    let app = build_router(state, &config);

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.bind_addr))?;
    tracing::info!(addr = %listener.local_addr()?, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    tracing::info!("shutdown signal received, draining connections");
    // Give in-flight work a brief window, then close the pool cleanly.
    tokio::time::timeout(Duration::from_secs(10), pool.close())
        .await
        .ok();
    tracing::info!("bye");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
