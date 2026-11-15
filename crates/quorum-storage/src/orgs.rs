//! Organizations, memberships and invitations.

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct OrganizationRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct MyMembershipRow {
    pub organization_id: Uuid,
    pub organization_name: String,
    pub role: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct MemberRow {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct InvitationRow {
    pub id: Uuid,
    pub email_normalized: String,
    pub role: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub accepted_at: Option<OffsetDateTime>,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PendingInvitation {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub email_normalized: String,
    pub role: String,
}

/// Requires `app.org_id` = `id` and `app.user_id` = `created_by`.
pub async fn insert_organization(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    created_by: Uuid,
) -> Result<OrganizationRow> {
    Ok(sqlx::query_as(
        "INSERT INTO organizations (id, name, created_by) VALUES ($1, $2, $3) RETURNING id, name, created_at",
    )
    .bind(id)
    .bind(name)
    .bind(created_by)
    .fetch_one(conn)
    .await?)
}

pub async fn get_organization(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<OrganizationRow>> {
    Ok(
        sqlx::query_as("SELECT id, name, created_at FROM organizations WHERE id = $1")
            .bind(id)
            .fetch_optional(conn)
            .await?,
    )
}

pub async fn insert_membership(
    conn: &mut PgConnection,
    org: Uuid,
    user: Uuid,
    role: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org)
        .bind(user)
        .bind(role)
        .execute(conn)
        .await?;
    Ok(())
}

/// The caller's role in an organization, if any.
pub async fn role_of(conn: &mut PgConnection, org: Uuid, user: Uuid) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT role FROM memberships WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_optional(conn)
    .await?)
}

/// Organizations the current user belongs to.
pub async fn my_memberships(conn: &mut PgConnection, user: Uuid) -> Result<Vec<MyMembershipRow>> {
    Ok(sqlx::query_as(
        "SELECT m.organization_id, o.name AS organization_name, m.role
         FROM memberships m JOIN organizations o ON o.id = m.organization_id
         WHERE m.user_id = $1 ORDER BY o.name",
    )
    .bind(user)
    .fetch_all(conn)
    .await?)
}

pub async fn list_members(conn: &mut PgConnection, org: Uuid) -> Result<Vec<MemberRow>> {
    Ok(sqlx::query_as(
        "SELECT m.user_id, u.email, u.display_name, m.role, m.created_at
         FROM memberships m JOIN users u ON u.id = m.user_id
         WHERE m.organization_id = $1 ORDER BY u.display_name, u.email",
    )
    .bind(org)
    .fetch_all(conn)
    .await?)
}

pub async fn count_owners(conn: &mut PgConnection, org: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM memberships WHERE organization_id = $1 AND role = 'owner'",
    )
    .bind(org)
    .fetch_one(conn)
    .await?)
}

pub async fn update_role(
    conn: &mut PgConnection,
    org: Uuid,
    user: Uuid,
    role: &str,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE memberships SET role = $3, updated_at = now() WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .bind(role)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

pub async fn delete_membership(conn: &mut PgConnection, org: Uuid, user: Uuid) -> Result<bool> {
    Ok(
        sqlx::query("DELETE FROM memberships WHERE organization_id = $1 AND user_id = $2")
            .bind(org)
            .bind(user)
            .execute(conn)
            .await?
            .rows_affected()
            == 1,
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_invitation(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    email_normalized: &str,
    role: &str,
    token_digest: &[u8],
    invited_by: Uuid,
    expires_at: OffsetDateTime,
) -> Result<InvitationRow> {
    Ok(sqlx::query_as(
        "INSERT INTO invitations (id, organization_id, email_normalized, role, token_digest, invited_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, email_normalized, role, created_at, expires_at, accepted_at, revoked_at",
    )
    .bind(id)
    .bind(org)
    .bind(email_normalized)
    .bind(role)
    .bind(token_digest)
    .bind(invited_by)
    .bind(expires_at)
    .fetch_one(conn)
    .await?)
}

pub async fn list_invitations(conn: &mut PgConnection, org: Uuid) -> Result<Vec<InvitationRow>> {
    Ok(sqlx::query_as(
        "SELECT id, email_normalized, role, created_at, expires_at, accepted_at, revoked_at
         FROM invitations WHERE organization_id = $1 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(org)
    .fetch_all(conn)
    .await?)
}

pub async fn find_pending_invitation(
    conn: &mut PgConnection,
    token_digest: &[u8],
) -> Result<Option<PendingInvitation>> {
    Ok(sqlx::query_as(
        "SELECT id, organization_id, email_normalized, role FROM auth_find_invitation($1)",
    )
    .bind(token_digest)
    .fetch_optional(conn)
    .await?)
}

/// Marks an invitation accepted. Requires `app.org_id` = invitation org.
/// Returns false if it was concurrently accepted or revoked.
pub async fn mark_invitation_accepted(
    conn: &mut PgConnection,
    id: Uuid,
    user: Uuid,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE invitations SET accepted_at = now(), accepted_by = $2
         WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(id)
    .bind(user)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

pub async fn revoke_invitation(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE invitations SET revoked_at = now()
         WHERE organization_id = $1 AND id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(org)
    .bind(id)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}
