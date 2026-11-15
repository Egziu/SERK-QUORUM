//! Provider usage records (metadata only; prompts and responses are not stored).

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewProviderRequest<'a> {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub project_id: Option<Uuid>,
    pub provider: &'a str,
    pub model: &'a str,
    pub task: &'a str,
    pub mode: &'a str,
    pub outcome: &'a str,
    pub prompt_chars: i32,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct ProviderRequestRow {
    pub id: Uuid,
    pub user_name: String,
    pub project_id: Option<Uuid>,
    pub provider: String,
    pub model: String,
    pub task: String,
    pub mode: String,
    pub outcome: String,
    pub prompt_chars: i32,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: i32,
    pub created_at: OffsetDateTime,
}

pub async fn record(conn: &mut PgConnection, r: &NewProviderRequest<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO provider_requests (id, organization_id, user_id, project_id, provider, model, task, mode, outcome,
                                        prompt_chars, input_tokens, output_tokens, latency_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(r.id)
    .bind(r.organization_id)
    .bind(r.user_id)
    .bind(r.project_id)
    .bind(r.provider)
    .bind(r.model)
    .bind(r.task)
    .bind(r.mode)
    .bind(r.outcome)
    .bind(r.prompt_chars)
    .bind(r.input_tokens)
    .bind(r.output_tokens)
    .bind(r.latency_ms)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list(
    conn: &mut PgConnection,
    org: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProviderRequestRow>> {
    Ok(sqlx::query_as(
        "SELECT r.id, coalesce(u.display_name, 'Former member') AS user_name, r.project_id, r.provider, r.model, r.task,
                r.mode, r.outcome, r.prompt_chars, r.input_tokens, r.output_tokens, r.latency_ms, r.created_at
         FROM provider_requests r LEFT JOIN users u ON u.id = r.user_id
         WHERE r.organization_id = $1 ORDER BY r.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(org)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}
