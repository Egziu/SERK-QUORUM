use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ProjectRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub created_by: Uuid,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub document_count: i64,
    pub decision_count: i64,
}

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    name: &str,
    description: &str,
    created_by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO projects (id, organization_id, name, description, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(org)
    .bind(name)
    .bind(description)
    .bind(created_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Option<ProjectRow>> {
    Ok(sqlx::query_as(
        "SELECT p.id, p.organization_id, p.name, p.description, p.status, p.created_by, p.created_at, p.updated_at,
                (SELECT count(*) FROM documents d WHERE d.project_id = p.id) AS document_count,
                (SELECT count(*) FROM decisions c WHERE c.project_id = p.id) AS decision_count
         FROM projects p WHERE p.organization_id = $1 AND p.id = $2",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

/// Lists projects, most recently updated first. `include_archived` controls
/// whether archived projects are returned.
pub async fn list(
    conn: &mut PgConnection,
    org: Uuid,
    include_archived: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProjectRow>> {
    Ok(sqlx::query_as(
        "SELECT p.id, p.organization_id, p.name, p.description, p.status, p.created_by, p.created_at, p.updated_at,
                (SELECT count(*) FROM documents d WHERE d.project_id = p.id) AS document_count,
                (SELECT count(*) FROM decisions c WHERE c.project_id = p.id) AS decision_count
         FROM projects p
         WHERE p.organization_id = $1 AND ($2 OR p.status <> 'archived')
         ORDER BY p.updated_at DESC, p.id
         LIMIT $3 OFFSET $4",
    )
    .bind(org)
    .bind(include_archived)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}

pub async fn update(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    name: &str,
    description: &str,
    status: &str,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE projects SET name = $3, description = $4, status = $5, updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(status)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

/// Locks the project row for the rest of the transaction and returns its
/// status. Used before creating or changing children so that a concurrent
/// archive cannot interleave.
pub async fn lock_status(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT status FROM projects WHERE organization_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub async fn touch(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<()> {
    sqlx::query("UPDATE projects SET updated_at = now() WHERE organization_id = $1 AND id = $2")
        .bind(org)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}
