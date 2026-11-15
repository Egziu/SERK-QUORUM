//! Local-account authentication and session management.
//!
//! Local accounts (email + Argon2id password) are the identity source in
//! this release. OpenID Connect sign-in is planned behind the same session
//! model (see ADR 0003); MFA will be delegated to the identity provider.

use super::{ApiJson, ts};
use crate::context::Session;
use crate::error::{ApiError, ApiResult};
use crate::request_id;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use quorum_domain::ids::{OrganizationId, UserId};
use quorum_domain::{Permission, Role, validate};
use quorum_storage::{audit, identity, orgs};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignupRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub organization_name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Serialize)]
pub struct MembershipView {
    pub organization_id: Uuid,
    pub organization_name: String,
    pub role: Role,
    pub permissions: &'static [Permission],
}

#[derive(Serialize)]
pub struct SessionView {
    pub user: UserView,
    pub memberships: Vec<MembershipView>,
    /// Send back in the `x-csrf-token` header on every state-changing request.
    pub csrf_token: String,
}

fn session_cookie(state: &AppState, token: &str) -> ApiResult<HeaderValue> {
    let c = state.config();
    let secure = if c.cookie_secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        c.session_cookie_name(),
        token,
        c.session_ttl.as_secs(),
        secure
    ))
    .map_err(|_| ApiError::Internal)
}

fn clear_cookie(state: &AppState) -> ApiResult<HeaderValue> {
    let c = state.config();
    let secure = if c.cookie_secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        c.session_cookie_name(),
        secure
    ))
    .map_err(|_| ApiError::Internal)
}

async fn hash_password(password: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || quorum_auth::hash_password(&password))
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(|_| ApiError::Internal)
}

/// Creates a session row inside `tx` (whose `app.user_id` must be `user`)
/// and returns (bearer secret, csrf token).
async fn create_session(
    state: &AppState,
    tx: &mut quorum_storage::Tx,
    user: UserId,
) -> ApiResult<(String, String)> {
    let token = quorum_auth::issue_token().map_err(|_| ApiError::Internal)?;
    let csrf = quorum_auth::issue_token()
        .map_err(|_| ApiError::Internal)?
        .secret;
    let ttl =
        time::Duration::try_from(state.config().session_ttl).map_err(|_| ApiError::Internal)?;
    let expires_at = OffsetDateTime::now_utc()
        .checked_add(ttl)
        .ok_or(ApiError::Internal)?;
    identity::insert_session(
        tx,
        Uuid::now_v7(),
        user.as_uuid(),
        &token.digest,
        &csrf,
        expires_at,
    )
    .await?;
    Ok((token.secret, csrf))
}

async fn session_view(
    state: &AppState,
    user: UserId,
    csrf_token: String,
) -> ApiResult<SessionView> {
    let mut tx = quorum_storage::begin(state.pool(), Some(user), None).await?;
    let row = identity::get_user(&mut tx, user.as_uuid())
        .await?
        .ok_or(ApiError::Unauthenticated)?;
    let memberships = orgs::my_memberships(&mut tx, user.as_uuid())
        .await?
        .into_iter()
        .filter_map(|m| {
            let role = m.role.parse::<Role>().ok()?;
            Some(MembershipView {
                organization_id: m.organization_id,
                organization_name: m.organization_name,
                role,
                permissions: role.permissions(),
            })
        })
        .collect();
    tx.commit().await?;
    Ok(SessionView {
        user: UserView {
            id: row.id,
            email: row.email,
            display_name: row.display_name,
            created_at: row.created_at,
        },
        memberships,
        csrf_token,
    })
}

async fn record_unscoped(
    state: &AppState,
    actor: Option<Uuid>,
    event: &str,
    outcome: &str,
    details: serde_json::Value,
) {
    let result = async {
        let mut tx =
            quorum_storage::begin(state.pool(), actor.map(UserId::from_uuid), None).await?;
        audit::record(
            &mut tx,
            &audit::NewAuditEvent {
                organization_id: None,
                actor_user_id: actor,
                event_type: event,
                resource_type: Some("user"),
                resource_id: actor,
                outcome,
                correlation_id: &request_id::current(),
                details,
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(quorum_storage::StorageError::from)
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(error = %e, "could not record authentication event");
    }
}

/// POST /api/v1/auth/signup — creates an account, its first organization
/// and an owner membership. Disabled unless QUORUM_ALLOW_SIGNUP is set.
pub async fn signup(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<SignupRequest>,
) -> ApiResult<impl IntoResponse> {
    if !state.config().allow_signup {
        return Err(ApiError::Forbidden);
    }
    let email_display = req.email.trim().to_owned();
    let email = validate::email(&req.email)?;
    let display_name = validate::single_line("display_name", &req.display_name, 1, 120)?;
    let org_name = validate::single_line("organization_name", &req.organization_name, 1, 120)?;
    validate::password(&req.password)?;

    let user = UserId::new();
    let org = OrganizationId::new();
    let mut tx = quorum_storage::begin(state.pool(), Some(user), Some(org)).await?;
    if identity::email_exists(&mut tx, &email).await? {
        return Err(ApiError::conflict(
            "An account with this email already exists. Sign in instead.",
        ));
    }
    let hash = hash_password(req.password).await?;
    identity::insert_user(
        &mut tx,
        user.as_uuid(),
        &email_display,
        &email,
        &display_name,
        Some(&hash),
    )
    .await?;
    orgs::insert_organization(&mut tx, org.as_uuid(), &org_name, user.as_uuid()).await?;
    orgs::insert_membership(&mut tx, org.as_uuid(), user.as_uuid(), Role::Owner.as_str()).await?;
    for (event, rtype, rid) in [
        ("user.registered", "user", user.as_uuid()),
        ("organization.created", "organization", org.as_uuid()),
    ] {
        audit::record(
            &mut tx,
            &audit::NewAuditEvent {
                organization_id: Some(org.as_uuid()),
                actor_user_id: Some(user.as_uuid()),
                event_type: event,
                resource_type: Some(rtype),
                resource_id: Some(rid),
                outcome: "success",
                correlation_id: &request_id::current(),
                details: json!({}),
            },
        )
        .await?;
    }
    let (secret, csrf) = create_session(&state, &mut tx, user).await?;
    tx.commit().await?;

    let view = session_view(&state, user, csrf).await?;
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, session_cookie(&state, &secret)?);
    Ok((StatusCode::CREATED, headers, Json(view)))
}

/// POST /api/v1/auth/login
pub async fn login(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    // Normalize without revealing validation details for unknown accounts.
    let email = validate::email(&req.email).unwrap_or_default();
    if req.password.len() > 1024 {
        return Err(ApiError::Unauthenticated);
    }
    let throttle_key = format!("login:{email}");
    if state.inner.login_throttle.is_limited(&throttle_key) {
        record_unscoped(
            &state,
            None,
            "auth.login",
            "denied",
            json!({ "reason": "throttled" }),
        )
        .await;
        return Err(ApiError::RateLimited(
            "Too many failed sign-in attempts. Try again in 15 minutes.",
        ));
    }

    let creds = {
        let mut conn = state.pool().acquire().await?;
        identity::find_credentials(&mut conn, &email).await?
    };
    let password = req.password;
    let verified = match creds {
        Some(c) if c.status == "active" => match c.password_hash {
            Some(hash) => {
                let ok = tokio::task::spawn_blocking(move || {
                    quorum_auth::verify_password(&password, &hash)
                })
                .await
                .map_err(|_| ApiError::Internal)?;
                ok.then_some(UserId::from_uuid(c.id))
            }
            None => None,
        },
        _ => {
            let _ =
                tokio::task::spawn_blocking(move || quorum_auth::equalize_timing(&password)).await;
            None
        }
    };
    let Some(user) = verified else {
        state.inner.login_throttle.hit(&throttle_key);
        record_unscoped(&state, None, "auth.login", "failure", json!({})).await;
        return Err(ApiError::Unauthenticated);
    };
    state.inner.login_throttle.reset(&throttle_key);

    let mut tx = quorum_storage::begin(state.pool(), Some(user), None).await?;
    let (secret, csrf) = create_session(&state, &mut tx, user).await?;
    tx.commit().await?;
    record_unscoped(
        &state,
        Some(user.as_uuid()),
        "auth.login",
        "success",
        json!({}),
    )
    .await;

    let view = session_view(&state, user, csrf).await?;
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, session_cookie(&state, &secret)?);
    Ok((headers, Json(view)))
}

/// POST /api/v1/auth/logout — revokes the current session.
pub async fn logout(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<impl IntoResponse> {
    let mut tx = quorum_storage::begin(state.pool(), Some(session.user_id), None).await?;
    identity::revoke_session(&mut tx, session.session_id).await?;
    tx.commit().await?;
    record_unscoped(
        &state,
        Some(session.user_id.as_uuid()),
        "auth.logout",
        "success",
        json!({}),
    )
    .await;
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, clear_cookie(&state)?);
    Ok((StatusCode::NO_CONTENT, headers))
}

/// POST /api/v1/auth/logout-all — revokes every session of the current user.
pub async fn logout_all(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<impl IntoResponse> {
    let mut tx = quorum_storage::begin(state.pool(), Some(session.user_id), None).await?;
    let revoked = identity::revoke_all_sessions(&mut tx, session.user_id.as_uuid()).await?;
    tx.commit().await?;
    record_unscoped(
        &state,
        Some(session.user_id.as_uuid()),
        "auth.sessions_revoked",
        "success",
        json!({ "count": revoked }),
    )
    .await;
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, clear_cookie(&state)?);
    Ok((StatusCode::NO_CONTENT, headers))
}

/// GET /api/v1/auth/session — the current user, memberships and CSRF token.
pub async fn current(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<SessionView>> {
    Ok(Json(
        session_view(&state, session.user_id, session.csrf_token).await?,
    ))
}
