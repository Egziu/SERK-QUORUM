//! `quorum-api serve` — run the HTTP service.
//! `quorum-api migrate` — apply database migrations using
//! `QUORUM_MIGRATION_DATABASE_URL` (the schema owner role), never the
//! application role.

use quorum_api::{AppState, Config, router};
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    let json = std::env::var("QUORUM_LOG_FORMAT").is_ok_and(|v| v == "json");
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
}

async fn migrate() -> Result<(), String> {
    let url = std::env::var("QUORUM_MIGRATION_DATABASE_URL")
        .map_err(|_| "QUORUM_MIGRATION_DATABASE_URL is required".to_owned())?;
    let pool = quorum_storage::connect(&url, 1)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    quorum_storage::MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| format!("migration failed: {e}"))?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn serve() -> Result<(), String> {
    let config = Config::from_env().map_err(|e| e.to_string())?;
    tracing::info!(?config, "starting SERK QUORUM API");
    if !config.cookie_secure {
        tracing::warn!("QUORUM_COOKIE_SECURE is off: acceptable only for local HTTP development");
    }
    let pool = quorum_storage::connect(&config.database_url, config.database_max_connections)
        .await
        .map_err(|e| format!("database: {e}"))?;
    let bind = config.bind;
    let state = AppState::new(config, pool).map_err(|e| format!("http client: {e}"))?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| format!("bind {bind}: {e}"))?;
    tracing::info!(%bind, "listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| format!("server: {e}"))?;
    tracing::info!("shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received; draining requests");
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    let result = match command.as_str() {
        "serve" => serve().await,
        "migrate" => migrate().await,
        other => Err(format!(
            "unknown command `{other}`; expected `serve` or `migrate`"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
