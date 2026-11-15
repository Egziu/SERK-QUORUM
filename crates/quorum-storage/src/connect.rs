//! Web Connect persistence: briefs (versioned), custom roles, handoffs and
//! imported responses. All functions take a connection whose transaction has
//! tenant context set (see `crate::begin`); queries also filter by org.

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// ---------------------------------------------------------------- policy

pub async fn external_share_limit(conn: &mut PgConnection, org: Uuid) -> Result<String> {
    Ok(
        sqlx::query_scalar("SELECT external_share_limit FROM organizations WHERE id = $1")
            .bind(org)
            .fetch_one(conn)
            .await?,
    )
}

pub async fn set_external_share_limit(
    conn: &mut PgConnection,
    org: Uuid,
    limit: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE organizations SET external_share_limit = $2, updated_at = now() WHERE id = $1",
    )
    .bind(org)
    .bind(limit)
    .execute(conn)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------- roles

#[derive(Debug, Clone, FromRow)]
pub struct RoleRow {
    pub id: Uuid,
    pub name: String,
    pub focus: String,
    pub guidance: String,
}

pub async fn list_roles(conn: &mut PgConnection, org: Uuid) -> Result<Vec<RoleRow>> {
    Ok(sqlx::query_as(
        "SELECT id, name, focus, guidance FROM role_templates WHERE organization_id = $1 ORDER BY lower(name)",
    )
    .bind(org)
    .fetch_all(conn)
    .await?)
}

pub async fn get_role(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Option<RoleRow>> {
    Ok(sqlx::query_as(
        "SELECT id, name, focus, guidance FROM role_templates WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub async fn insert_role(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    name: &str,
    focus: &str,
    guidance: &str,
    by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO role_templates (id, organization_id, name, focus, guidance, created_by) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(org)
    .bind(name)
    .bind(focus)
    .bind(guidance)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_role(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<bool> {
    Ok(
        sqlx::query("DELETE FROM role_templates WHERE organization_id = $1 AND id = $2")
            .bind(org)
            .bind(id)
            .execute(conn)
            .await?
            .rows_affected()
            == 1,
    )
}

// ---------------------------------------------------------------- briefs

#[derive(Debug, Clone, FromRow)]
pub struct BriefRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub mode: String,
    pub classification: String,
    pub current_version: i32,
    pub created_by: Uuid,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub content: serde_json::Value,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_brief(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    project: Uuid,
    title: &str,
    mode: &str,
    classification: &str,
    content: &serde_json::Value,
    by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO briefs (id, organization_id, project_id, title, mode, classification, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(org)
    .bind(project)
    .bind(title)
    .bind(mode)
    .bind(classification)
    .bind(by)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO brief_versions (organization_id, brief_id, version, content, created_by) VALUES ($1, $2, 1, $3, $4)",
    )
    .bind(org)
    .bind(id)
    .bind(content)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

/// Appends a version. The caller holds the row lock from `get_brief(.., lock = true)`.
#[allow(clippy::too_many_arguments)]
pub async fn append_brief_version(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    observed: i32,
    title: &str,
    mode: &str,
    classification: &str,
    content: &serde_json::Value,
    by: Uuid,
) -> Result<i32> {
    let next = observed
        .checked_add(1)
        .ok_or(crate::StorageError::Integrity)?;
    sqlx::query(
        "INSERT INTO brief_versions (organization_id, brief_id, version, content, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org)
    .bind(id)
    .bind(next)
    .bind(content)
    .bind(by)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE briefs SET title = $3, mode = $4, classification = $5, current_version = $6, updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(title)
    .bind(mode)
    .bind(classification)
    .bind(next)
    .execute(conn)
    .await?;
    Ok(next)
}

pub async fn get_brief(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<Option<BriefRow>> {
    let sql = if lock {
        "SELECT b.id, b.project_id, b.title, b.mode, b.classification, b.current_version, b.created_by, b.created_at,
                b.updated_at, v.content
         FROM briefs b JOIN brief_versions v ON v.brief_id = b.id AND v.version = b.current_version
         WHERE b.organization_id = $1 AND b.id = $2 FOR UPDATE OF b"
    } else {
        "SELECT b.id, b.project_id, b.title, b.mode, b.classification, b.current_version, b.created_by, b.created_at,
                b.updated_at, v.content
         FROM briefs b JOIN brief_versions v ON v.brief_id = b.id AND v.version = b.current_version
         WHERE b.organization_id = $1 AND b.id = $2"
    };
    Ok(sqlx::query_as(sql)
        .bind(org)
        .bind(id)
        .fetch_optional(conn)
        .await?)
}

pub async fn list_briefs(
    conn: &mut PgConnection,
    org: Uuid,
    project: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<BriefRow>> {
    Ok(sqlx::query_as(
        "SELECT b.id, b.project_id, b.title, b.mode, b.classification, b.current_version, b.created_by, b.created_at,
                b.updated_at, v.content
         FROM briefs b JOIN brief_versions v ON v.brief_id = b.id AND v.version = b.current_version
         WHERE b.organization_id = $1 AND b.project_id = $2
         ORDER BY b.updated_at DESC, b.id LIMIT $3 OFFSET $4",
    )
    .bind(org)
    .bind(project)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}

// ---------------------------------------------------------------- handoffs

#[derive(Debug, Clone, FromRow)]
pub struct HandoffRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub brief_id: Uuid,
    pub brief_version: i32,
    pub provider: String,
    pub role_name: String,
    pub mode: String,
    pub instruction: String,
    pub instruction_sha256: Vec<u8>,
    pub context_items: serde_json::Value,
    pub status: String,
    pub opened_at: Option<OffsetDateTime>,
    pub created_by: Uuid,
    pub created_at: OffsetDateTime,
}

pub struct NewHandoff<'a> {
    pub id: Uuid,
    pub org: Uuid,
    pub project: Uuid,
    pub brief: Uuid,
    pub brief_version: i32,
    pub provider: &'a str,
    pub role_name: &'a str,
    pub instruction: &'a str,
    pub instruction_sha256: &'a [u8],
    pub context_items: &'a serde_json::Value,
    pub by: Uuid,
}

pub async fn insert_handoff(conn: &mut PgConnection, h: &NewHandoff<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO handoffs (id, organization_id, project_id, brief_id, brief_version, provider, role_name, mode,
                               instruction, instruction_sha256, context_items, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'web', $8, $9, $10, $11)",
    )
    .bind(h.id)
    .bind(h.org)
    .bind(h.project)
    .bind(h.brief)
    .bind(h.brief_version)
    .bind(h.provider)
    .bind(h.role_name)
    .bind(h.instruction)
    .bind(h.instruction_sha256)
    .bind(h.context_items)
    .bind(h.by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_handoff(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<Option<HandoffRow>> {
    Ok(sqlx::query_as(
        "SELECT id, project_id, brief_id, brief_version, provider, role_name, mode, instruction, instruction_sha256,
                context_items, status, opened_at, created_by, created_at
         FROM handoffs WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub async fn list_handoffs(
    conn: &mut PgConnection,
    org: Uuid,
    brief: Uuid,
) -> Result<Vec<HandoffRow>> {
    Ok(sqlx::query_as(
        "SELECT id, project_id, brief_id, brief_version, provider, role_name, mode, instruction, instruction_sha256,
                context_items, status, opened_at, created_by, created_at
         FROM handoffs WHERE organization_id = $1 AND brief_id = $2 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(org)
    .bind(brief)
    .fetch_all(conn)
    .await?)
}

/// Marks a handoff as opened by the user. Never moves backwards from imported.
pub async fn mark_opened(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE handoffs SET status = 'opened', opened_at = coalesce(opened_at, now())
         WHERE organization_id = $1 AND id = $2 AND status = 'prepared'",
    )
    .bind(org)
    .bind(id)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

pub async fn mark_imported(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<()> {
    sqlx::query("UPDATE handoffs SET status = 'imported' WHERE organization_id = $1 AND id = $2")
        .bind(org)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------- responses

#[derive(Debug, Clone, FromRow)]
pub struct ResponseRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub handoff_id: Option<Uuid>,
    pub brief_id: Option<Uuid>,
    pub brief_version: Option<i32>,
    pub provider: String,
    pub model: String,
    pub source_mode: String,
    pub provenance: String,
    pub body: String,
    pub content_sha256: Vec<u8>,
    pub reference_url: String,
    pub review_status: String,
    pub imported_by: Uuid,
    pub imported_by_name: String,
    pub imported_at: OffsetDateTime,
    pub role_name: Option<String>,
}

pub struct NewResponse<'a> {
    pub id: Uuid,
    pub org: Uuid,
    pub project: Uuid,
    pub handoff: Option<Uuid>,
    pub brief: Option<Uuid>,
    pub brief_version: Option<i32>,
    pub provider: &'a str,
    pub model: &'a str,
    pub source_mode: &'a str,
    pub provenance: &'a str,
    pub body: &'a str,
    pub content_sha256: &'a [u8],
    pub reference_url: &'a str,
    pub by: Uuid,
}

pub async fn insert_response(conn: &mut PgConnection, r: &NewResponse<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO responses (id, organization_id, project_id, handoff_id, brief_id, brief_version, provider, model,
                                source_mode, provenance, body, content_sha256, reference_url, imported_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(r.id)
    .bind(r.org)
    .bind(r.project)
    .bind(r.handoff)
    .bind(r.brief)
    .bind(r.brief_version)
    .bind(r.provider)
    .bind(r.model)
    .bind(r.source_mode)
    .bind(r.provenance)
    .bind(r.body)
    .bind(r.content_sha256)
    .bind(r.reference_url)
    .bind(r.by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find_duplicate(
    conn: &mut PgConnection,
    org: Uuid,
    project: Uuid,
    sha: &[u8],
) -> Result<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM responses WHERE organization_id = $1 AND project_id = $2 AND content_sha256 = $3",
    )
    .bind(org)
    .bind(project)
    .bind(sha)
    .fetch_optional(conn)
    .await?)
}

pub async fn get_response(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<Option<ResponseRow>> {
    Ok(sqlx::query_as(
        "SELECT r.id, r.project_id, r.handoff_id, r.brief_id, r.brief_version, r.provider, r.model, r.source_mode,
                r.provenance, r.body, r.content_sha256, r.reference_url, r.review_status, r.imported_by,
                coalesce(u.display_name, 'Former member') AS imported_by_name, r.imported_at, h.role_name
         FROM responses r LEFT JOIN users u ON u.id = r.imported_by LEFT JOIN handoffs h ON h.id = r.handoff_id
         WHERE r.organization_id = $1 AND r.id = $2",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub async fn list_responses(
    conn: &mut PgConnection,
    org: Uuid,
    project: Uuid,
    brief: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResponseRow>> {
    Ok(sqlx::query_as(
        "SELECT r.id, r.project_id, r.handoff_id, r.brief_id, r.brief_version, r.provider, r.model, r.source_mode,
                r.provenance, r.body, r.content_sha256, r.reference_url, r.review_status, r.imported_by,
                coalesce(u.display_name, 'Former member') AS imported_by_name, r.imported_at, h.role_name
         FROM responses r LEFT JOIN users u ON u.id = r.imported_by LEFT JOIN handoffs h ON h.id = r.handoff_id
         WHERE r.organization_id = $1 AND r.project_id = $2 AND ($3::uuid IS NULL OR r.brief_id = $3)
         ORDER BY r.imported_at DESC, r.id LIMIT $4 OFFSET $5",
    )
    .bind(org)
    .bind(project)
    .bind(brief)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}

pub async fn set_review_status(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    status: &str,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE responses SET review_status = $3 WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(status)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}
