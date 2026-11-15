//! Users and sessions.

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub status: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct Credentials {
    pub id: Uuid,
    pub password_hash: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionLookup {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub csrf_token: String,
    pub expires_at: OffsetDateTime,
}

pub async fn email_exists(conn: &mut PgConnection, email_normalized: &str) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT auth_email_exists($1)")
        .bind(email_normalized)
        .fetch_one(conn)
        .await?)
}

/// Requires `app.user_id` = `id` in the transaction context.
pub async fn insert_user(
    conn: &mut PgConnection,
    id: Uuid,
    email: &str,
    email_normalized: &str,
    display_name: &str,
    password_hash: Option<&str>,
) -> Result<UserRow> {
    Ok(sqlx::query_as(
        "INSERT INTO users (id, email, email_normalized, display_name, password_hash)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, email, display_name, status, created_at",
    )
    .bind(id)
    .bind(email)
    .bind(email_normalized)
    .bind(display_name)
    .bind(password_hash)
    .fetch_one(conn)
    .await?)
}

pub async fn find_credentials(
    conn: &mut PgConnection,
    email_normalized: &str,
) -> Result<Option<Credentials>> {
    Ok(
        sqlx::query_as("SELECT id, password_hash, status FROM auth_find_user_by_email($1)")
            .bind(email_normalized)
            .fetch_optional(conn)
            .await?,
    )
}

/// Visible only for the current user or members of the current organization.
pub async fn get_user(conn: &mut PgConnection, id: Uuid) -> Result<Option<UserRow>> {
    Ok(sqlx::query_as(
        "SELECT id, email, display_name, status, created_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub async fn insert_session(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
    token_digest: &[u8],
    csrf_token: &str,
    expires_at: OffsetDateTime,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_digest, csrf_token, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(user_id)
    .bind(token_digest)
    .bind(csrf_token)
    .bind(expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find_session(
    conn: &mut PgConnection,
    token_digest: &[u8],
) -> Result<Option<SessionLookup>> {
    Ok(sqlx::query_as(
        "SELECT session_id, user_id, csrf_token, expires_at FROM auth_find_session($1)",
    )
    .bind(token_digest)
    .fetch_optional(conn)
    .await?)
}

/// Requires `app.user_id` = owner of the session.
pub async fn revoke_session(conn: &mut PgConnection, session_id: Uuid) -> Result<()> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(session_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Revokes every active session of the current user. Returns the count.
pub async fn revoke_all_sessions(conn: &mut PgConnection, user_id: Uuid) -> Result<u64> {
    Ok(sqlx::query(
        "UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(conn)
    .await?
    .rows_affected())
}
