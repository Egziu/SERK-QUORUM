//! Authentication and authorization context for handlers.
//!
//! * [`Session`] — extracted from the session cookie. Rejects missing,
//!   expired and revoked sessions, and enforces the CSRF header on every
//!   state-changing method.
//! * [`OrgScope`] — opened per request for one organization: verifies
//!   membership (404 if none, so organization existence is not disclosed),
//!   checks the permission (403), and holds a transaction whose tenant
//!   context drives PostgreSQL row-level security.

use crate::error::{ApiError, ApiResult};
use crate::request_id;
use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::Method;
use axum::http::request::Parts;
use quorum_domain::ids::{OrganizationId, UserId};
use quorum_domain::{Permission, Role};
use quorum_storage::{Tx, audit, identity, orgs};
use serde_json::json;
use uuid::Uuid;

pub const CSRF_HEADER: &str = "x-csrf-token";

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: UserId,
    pub session_id: Uuid,
    pub csrf_token: String,
}

fn cookie_value<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts
        .headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

impl FromRequestParts<AppState> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Self> {
        let token = cookie_value(parts, state.config().session_cookie_name())
            .ok_or(ApiError::Unauthenticated)?;
        if !quorum_auth::plausible_token(token) {
            return Err(ApiError::Unauthenticated);
        }
        let digest = quorum_auth::token_digest(token);
        let mut conn = state.pool().acquire().await?;
        let found = identity::find_session(&mut conn, &digest)
            .await?
            .ok_or(ApiError::Unauthenticated)?;
        let session = Session {
            user_id: UserId::from_uuid(found.user_id),
            session_id: found.session_id,
            csrf_token: found.csrf_token,
        };
        let safe = matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
        if !safe {
            let presented = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if !quorum_auth::constant_time_eq(presented, &session.csrf_token) {
                return Err(ApiError::Csrf);
            }
        }
        Ok(session)
    }
}

/// An authorized, tenant-scoped unit of work.
pub struct OrgScope {
    pub tx: Tx,
    pub org: OrganizationId,
    pub user: UserId,
    pub role: Role,
}

impl OrgScope {
    /// Opens a transaction for `org` and requires `permission`.
    pub async fn open(
        state: &AppState,
        session: &Session,
        org: Uuid,
        permission: Permission,
    ) -> ApiResult<Self> {
        let org = OrganizationId::from_uuid(org);
        let mut tx = quorum_storage::begin(state.pool(), Some(session.user_id), Some(org)).await?;
        let role = orgs::role_of(&mut tx, org.as_uuid(), session.user_id.as_uuid())
            .await?
            .and_then(|r| r.parse::<Role>().ok())
            .ok_or(ApiError::NotFound("Organization"))?;
        let scope = Self {
            tx,
            org,
            user: session.user_id,
            role,
        };
        if !role.allows(permission) {
            scope.deny(state, permission).await;
            return Err(ApiError::Forbidden);
        }
        Ok(scope)
    }

    pub fn require(&self, permission: Permission) -> ApiResult<()> {
        if self.role.allows(permission) {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }

    /// Records a denied action in a separate transaction (the request's own
    /// transaction is rolled back).
    async fn deny(self, state: &AppState, permission: Permission) {
        drop(self.tx);
        let result = async {
            let mut tx =
                quorum_storage::begin(state.pool(), Some(self.user), Some(self.org)).await?;
            audit::record(
                &mut tx,
                &audit::NewAuditEvent {
                    organization_id: Some(self.org.as_uuid()),
                    actor_user_id: Some(self.user.as_uuid()),
                    event_type: "access.denied",
                    resource_type: None,
                    resource_id: None,
                    outcome: "denied",
                    correlation_id: &request_id::current(),
                    details: json!({ "permission": permission, "role": self.role }),
                },
            )
            .await?;
            tx.commit().await?;
            Ok::<_, quorum_storage::StorageError>(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!(error = %e, "could not record access denial");
        }
    }

    /// Appends an audit event inside this transaction, so it commits or
    /// rolls back together with the change it describes.
    pub async fn audit(
        &mut self,
        event_type: &str,
        resource_type: &str,
        resource_id: Uuid,
        details: serde_json::Value,
    ) -> ApiResult<()> {
        audit::record(
            &mut self.tx,
            &audit::NewAuditEvent {
                organization_id: Some(self.org.as_uuid()),
                actor_user_id: Some(self.user.as_uuid()),
                event_type,
                resource_type: Some(resource_type),
                resource_id: Some(resource_id),
                outcome: "success",
                correlation_id: &request_id::current(),
                details,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn commit(self) -> ApiResult<()> {
        self.tx.commit().await?;
        Ok(())
    }
}

/// Pagination query parameters: `limit` 1–100 (default 50), `offset` 0–10 000.
#[derive(Debug, serde::Deserialize)]
pub struct Page {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Page {
    pub fn bounds(&self) -> ApiResult<(i64, i64)> {
        let limit = self.limit.unwrap_or(50);
        let offset = self.offset.unwrap_or(0);
        if !(1..=100).contains(&limit) || !(0..=10_000).contains(&offset) {
            return Err(ApiError::BadRequest(
                "limit must be 1–100 and offset 0–10000.".into(),
            ));
        }
        Ok((limit, offset))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

impl<T> Paged<T> {
    /// Builds a page from a query that fetched `limit + 1` rows.
    pub fn from_overfetch(mut items: Vec<T>, limit: i64, offset: i64) -> Self {
        let cap = usize::try_from(limit).unwrap_or(0);
        let has_more = items.len() > cap;
        items.truncate(cap);
        Self {
            items,
            limit,
            offset,
            has_more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_bounds() {
        assert_eq!(
            Page {
                limit: None,
                offset: None
            }
            .bounds()
            .expect("ok"),
            (50, 0)
        );
        assert!(
            Page {
                limit: Some(0),
                offset: None
            }
            .bounds()
            .is_err()
        );
        assert!(
            Page {
                limit: Some(101),
                offset: None
            }
            .bounds()
            .is_err()
        );
        assert!(
            Page {
                limit: Some(10),
                offset: Some(-1)
            }
            .bounds()
            .is_err()
        );
    }

    #[test]
    fn overfetch_sets_has_more() {
        let p = Paged::from_overfetch(vec![1, 2, 3], 2, 0);
        assert_eq!(p.items, vec![1, 2]);
        assert!(p.has_more);
        let p = Paged::from_overfetch(vec![1], 2, 0);
        assert!(!p.has_more);
    }
}
