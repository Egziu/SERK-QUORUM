//! Append-only audit events. The application role holds INSERT and SELECT
//! only; it cannot modify or delete events.

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewAuditEvent<'a> {
    pub organization_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub event_type: &'a str,
    pub resource_type: Option<&'a str>,
    pub resource_id: Option<Uuid>,
    pub outcome: &'a str,
    pub correlation_id: &'a str,
    /// Small, non-sensitive structured context. Never credentials or prompt text.
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuditEventRow {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_name: Option<String>,
    pub event_type: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub outcome: String,
    pub correlation_id: String,
    pub details: serde_json::Value,
    pub occurred_at: OffsetDateTime,
}

pub async fn record(conn: &mut PgConnection, e: &NewAuditEvent<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO audit_events (id, organization_id, actor_user_id, event_type, resource_type, resource_id, outcome, correlation_id, details)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::now_v7())
    .bind(e.organization_id)
    .bind(e.actor_user_id)
    .bind(e.event_type)
    .bind(e.resource_type)
    .bind(e.resource_id)
    .bind(e.outcome)
    .bind(e.correlation_id)
    .bind(&e.details)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list(
    conn: &mut PgConnection,
    org: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEventRow>> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.actor_user_id, u.display_name AS actor_name, a.event_type, a.resource_type, a.resource_id,
                a.outcome, a.correlation_id, a.details, a.occurred_at
         FROM audit_events a LEFT JOIN users u ON u.id = a.actor_user_id
         WHERE a.organization_id = $1 ORDER BY a.occurred_at DESC, a.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(org)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}
