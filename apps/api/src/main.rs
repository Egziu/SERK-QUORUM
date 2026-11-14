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
    ensure_app_login_role(&pool).await?;
    Ok(())
}

/// On platforms that provide only one superuser (e.g. Railway Postgres),
/// creates or updates the API's login role `quorum_api` as a member of
/// `quorum_app`, with the password from `QUORUM_APP_DB_PASSWORD`. The role
/// owns nothing and cannot bypass row-level security. Skipped when unset.
async fn ensure_app_login_role(pool: &sqlx::PgPool) -> Result<(), String> {
    let Ok(password) = std::env::var("QUORUM_APP_DB_PASSWORD") else {
        return Ok(());
    };
    if password.len() < 24 {
        return Err("QUORUM_APP_DB_PASSWORD must be at least 24 characters".into());
    }
    // format(%L) quotes the password as a literal inside PostgreSQL.
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'quorum_api')")
            .fetch_one(pool)
            .await
            .map_err(|e| format!("role check: {e}"))?;
    let template = if exists {
        "SELECT format('ALTER ROLE quorum_api LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD %L', $1::text)"
    } else {
        "SELECT format('CREATE ROLE quorum_api LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD %L IN ROLE quorum_app', $1::text)"
    };
    let statement: String = sqlx::query_scalar(template)
        .bind(&password)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("role statement: {e}"))?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
        .execute(pool)
        .await
        .map_err(|_| "could not create or update role quorum_api".to_owned())?;
    sqlx::raw_sql("GRANT quorum_app TO quorum_api")
        .execute(pool)
        .await
        .map_err(|e| format!("grant: {e}"))?;
    tracing::info!("application login role quorum_api ensured");
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
