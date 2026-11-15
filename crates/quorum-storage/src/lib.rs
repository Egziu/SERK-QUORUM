//! PostgreSQL persistence for SERK QUORUM.
//!
//! All SQL is static and parameterized (sqlx 0.9 accepts only `&'static str`
//! queries without an explicit opt-out, which this crate never uses).
//!
//! # Tenant context
//!
//! Every operation on tenant data runs inside a transaction opened with
//! [`begin`], which sets `app.user_id` and `app.org_id` *transaction-locally*.
//! PostgreSQL row-level security policies read those settings, so even a
//! query that forgot its `organization_id` filter cannot return another
//! tenant's rows. Because the settings are transaction-local they are
//! discarded at commit or rollback and cannot leak to the next request that
//! borrows the same pooled connection (verified by integration tests).

pub mod audit;
pub mod connect;
pub mod council;
pub mod decisions;
pub mod documents;
pub mod identity;
pub mod orgs;
pub mod projects;
pub mod usage;

use quorum_domain::ids::{OrganizationId, UserId};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;

pub use sqlx::PgConnection;

pub type Tx = Transaction<'static, Postgres>;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("record not found")]
    NotFound,
    #[error("conflicts with an existing record")]
    Conflict,
    #[error("violates a data integrity rule")]
    Integrity,
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => StorageError::NotFound,
            sqlx::Error::Database(db) => match db.code().as_deref() {
                Some("23505") => StorageError::Conflict,
                // check, foreign key, not-null, exclusion; also RLS WITH CHECK (42501)
                Some("23514" | "23503" | "23502" | "23P01" | "42501") => StorageError::Integrity,
                _ => StorageError::Database(e),
            },
            _ => StorageError::Database(e),
        }
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Creates a bounded connection pool.
pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool> {
    let options = PgConnectOptions::from_str(url)?.application_name("serk-quorum");
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// Opens a transaction with tenant context. `org` is `None` only for
/// operations that are not scoped to an organization (signup, listing a
/// user's own memberships, session management).
pub async fn begin(pool: &PgPool, user: Option<UserId>, org: Option<OrganizationId>) -> Result<Tx> {
    let mut tx = pool.begin().await?;
    let user = user.map(|u| u.to_string()).unwrap_or_default();
    let org = org.map(|o| o.to_string()).unwrap_or_default();
    sqlx::query("SELECT set_config('app.user_id', $1, true), set_config('app.org_id', $2, true)")
        .bind(user)
        .bind(org)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Readiness probe: verifies the database answers.
pub async fn ping(pool: &PgPool) -> Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
