use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

/// Liveness: the process is running. Does not touch dependencies.
pub async fn live() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Readiness: the database answers. Returns 503 otherwise.
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match quorum_storage::ping(state.pool()).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
        Err(e) => {
            tracing::warn!(error = %e, "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
        }
    }
}
