//! Documents and their append-only versions.

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct DocumentRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub category: String,
    pub status: String,
    pub current_version: i32,
    pub approved_version: Option<i32>,
    pub created_by: Uuid,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct VersionRow {
    pub version: i32,
    pub title: String,
    pub body: String,
    pub ai_assisted: bool,
    pub change_note: String,
    pub created_by: Uuid,
    pub created_by_name: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct VersionSummaryRow {
    pub version: i32,
    pub title: String,
    pub ai_assisted: bool,
    pub change_note: String,
    pub created_by_name: String,
    pub created_at: OffsetDateTime,
    pub body_chars: i32,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    project: Uuid,
    title: &str,
    category: &str,
    body: &str,
    ai_assisted: bool,
    created_by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO documents (id, organization_id, project_id, title, category, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(org)
    .bind(project)
    .bind(title)
    .bind(category)
    .bind(created_by)
    .execute(&mut *conn)
    .await?;
    insert_version(
        conn,
        org,
        id,
        1,
        title,
        body,
        ai_assisted,
        "Initial version",
        created_by,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn insert_version(
    conn: &mut PgConnection,
    org: Uuid,
    document: Uuid,
    version: i32,
    title: &str,
    body: &str,
    ai_assisted: bool,
    note: &str,
    created_by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO document_versions (organization_id, document_id, version, title, body, ai_assisted, change_note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(org)
    .bind(document)
    .bind(version)
    .bind(title)
    .bind(body)
    .bind(ai_assisted)
    .bind(note)
    .bind(created_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Option<DocumentRow>> {
    Ok(sqlx::query_as(
        "SELECT id, project_id, title, category, status, current_version, approved_version, created_by, created_at, updated_at
         FROM documents WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

/// Same as [`get`] but takes a row lock for the rest of the transaction.
pub async fn get_for_update(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<Option<DocumentRow>> {
    Ok(sqlx::query_as(
        "SELECT id, project_id, title, category, status, current_version, approved_version, created_by, created_at, updated_at
         FROM documents WHERE organization_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub async fn list(
    conn: &mut PgConnection,
    org: Uuid,
    project: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<DocumentRow>> {
    Ok(sqlx::query_as(
        "SELECT id, project_id, title, category, status, current_version, approved_version, created_by, created_at, updated_at
         FROM documents WHERE organization_id = $1 AND project_id = $2
         ORDER BY updated_at DESC, id LIMIT $3 OFFSET $4",
    )
    .bind(org)
    .bind(project)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}

/// Appends a new version. The caller must hold the row lock from
/// [`get_for_update`] and pass the version it observed.
#[allow(clippy::too_many_arguments)]
pub async fn append_version(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    observed_version: i32,
    title: &str,
    category: &str,
    body: &str,
    ai_assisted: bool,
    note: &str,
    created_by: Uuid,
) -> Result<i32> {
    let next = observed_version
        .checked_add(1)
        .ok_or(crate::StorageError::Integrity)?;
    insert_version(
        conn,
        org,
        id,
        next,
        title,
        body,
        ai_assisted,
        note,
        created_by,
    )
    .await?;
    sqlx::query(
        "UPDATE documents SET title = $3, category = $4, current_version = $5, status = 'draft', updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(title)
    .bind(category)
    .bind(next)
    .execute(conn)
    .await?;
    Ok(next)
}

pub async fn approve(conn: &mut PgConnection, org: Uuid, id: Uuid, version: i32) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE documents SET status = 'approved', approved_version = $3, updated_at = now()
         WHERE organization_id = $1 AND id = $2 AND current_version = $3",
    )
    .bind(org)
    .bind(id)
    .bind(version)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

pub async fn get_version(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    version: i32,
) -> Result<Option<VersionRow>> {
    Ok(sqlx::query_as(
        "SELECT v.version, v.title, v.body, v.ai_assisted, v.change_note, v.created_by,
                coalesce(u.display_name, 'Former member') AS created_by_name, v.created_at
         FROM document_versions v LEFT JOIN users u ON u.id = v.created_by
         WHERE v.organization_id = $1 AND v.document_id = $2 AND v.version = $3",
    )
    .bind(org)
    .bind(id)
    .bind(version)
    .fetch_optional(conn)
    .await?)
}

pub async fn list_versions(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<Vec<VersionSummaryRow>> {
    Ok(sqlx::query_as(
        "SELECT v.version, v.title, v.ai_assisted, v.change_note,
                coalesce(u.display_name, 'Former member') AS created_by_name, v.created_at,
                char_length(v.body) AS body_chars
         FROM document_versions v LEFT JOIN users u ON u.id = v.created_by
         WHERE v.organization_id = $1 AND v.document_id = $2 ORDER BY v.version DESC",
    )
    .bind(org)
    .bind(id)
    .fetch_all(conn)
    .await?)
}
