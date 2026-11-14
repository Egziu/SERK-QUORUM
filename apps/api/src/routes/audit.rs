use super::{ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::ApiResult;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use quorum_domain::Permission;
use quorum_storage::audit;
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Serialize)]
pub struct AuditView {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_name: Option<String>,
    pub event_type: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub outcome: String,
    pub correlation_id: String,
    pub details: serde_json::Value,
    #[serde(with = "ts::rfc3339")]
    pub occurred_at: OffsetDateTime,
}

/// GET /api/v1/orgs/{org}/audit-events — owners, administrators and auditors.
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiQuery(page): ApiQuery<Page>,
) -> ApiResult<Json<Paged<AuditView>>> {
    let (limit, offset) = page.bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::AuditRead).await?;
    let rows = audit::list(&mut scope.tx, org, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter()
            .map(|r| AuditView {
                id: r.id,
                actor_user_id: r.actor_user_id,
                actor_name: r.actor_name,
                event_type: r.event_type,
                resource_type: r.resource_type,
                resource_id: r.resource_id,
                outcome: r.outcome,
                correlation_id: r.correlation_id,
                details: r.details,
                occurred_at: r.occurred_at,
            })
            .collect(),
        limit,
        offset,
    )))
}
