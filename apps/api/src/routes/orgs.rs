//! Organizations, members and invitations.

use super::{ApiJson, ApiPath, ts};
use crate::context::{OrgScope, Session};
use crate::error::{ApiError, ApiResult};
use crate::request_id;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use quorum_domain::ids::{InvitationId, OrganizationId};
use quorum_domain::{Permission, Role, validate};
use quorum_storage::{audit, identity, orgs};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Serialize)]
pub struct OrganizationView {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    pub role: Role,
    pub permissions: &'static [Permission],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateOrganization {
    pub name: String,
}

/// POST /api/v1/orgs — creates an organization owned by the caller.
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    ApiJson(req): ApiJson<CreateOrganization>,
) -> ApiResult<(StatusCode, Json<OrganizationView>)> {
    let name = validate::single_line("name", &req.name, 1, 120)?;
    let org = OrganizationId::new();
    let mut tx = quorum_storage::begin(state.pool(), Some(session.user_id), Some(org)).await?;
    let row =
        orgs::insert_organization(&mut tx, org.as_uuid(), &name, session.user_id.as_uuid()).await?;
    orgs::insert_membership(
        &mut tx,
        org.as_uuid(),
        session.user_id.as_uuid(),
        Role::Owner.as_str(),
    )
    .await?;
    audit::record(
        &mut tx,
        &audit::NewAuditEvent {
            organization_id: Some(org.as_uuid()),
            actor_user_id: Some(session.user_id.as_uuid()),
            event_type: "organization.created",
            resource_type: Some("organization"),
            resource_id: Some(org.as_uuid()),
            outcome: "success",
            correlation_id: &request_id::current(),
            details: json!({}),
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(OrganizationView {
            id: row.id,
            name: row.name,
            created_at: row.created_at,
            role: Role::Owner,
            permissions: Role::Owner.permissions(),
        }),
    ))
}

/// GET /api/v1/orgs/{org}
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
) -> ApiResult<Json<OrganizationView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let row = orgs::get_organization(&mut scope.tx, org)
        .await?
        .ok_or(ApiError::NotFound("Organization"))?;
    let role = scope.role;
    scope.commit().await?;
    Ok(Json(OrganizationView {
        id: row.id,
        name: row.name,
        created_at: row.created_at,
        role,
        permissions: role.permissions(),
    }))
}

#[derive(Serialize)]
pub struct MemberView {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    #[serde(with = "ts::rfc3339")]
    pub joined_at: OffsetDateTime,
}

/// GET /api/v1/orgs/{org}/members
pub async fn members(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<MemberView>>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let rows = orgs::list_members(&mut scope.tx, org).await?;
    scope.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|m| MemberView {
                user_id: m.user_id,
                email: m.email,
                display_name: m.display_name,
                role: m.role,
                joined_at: m.created_at,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRole {
    pub role: Role,
}

/// Rules shared by role changes and removals: the actor must be allowed to
/// assign both the member's current role and (for changes) the new one, and
/// the last owner can never be demoted or removed.
async fn guard_member_change(
    scope: &mut OrgScope,
    target: Uuid,
    new_role: Option<Role>,
) -> ApiResult<Role> {
    let current = orgs::role_of(&mut scope.tx, scope.org.as_uuid(), target)
        .await?
        .and_then(|r| r.parse::<Role>().ok())
        .ok_or(ApiError::NotFound("Member"))?;
    if !scope.role.may_assign(current) || new_role.is_some_and(|r| !scope.role.may_assign(r)) {
        return Err(ApiError::Forbidden);
    }
    if current == Role::Owner
        && new_role != Some(Role::Owner)
        && orgs::count_owners(&mut scope.tx, scope.org.as_uuid()).await? <= 1
    {
        return Err(ApiError::conflict(
            "An organization must keep at least one owner.",
        ));
    }
    Ok(current)
}

/// PATCH /api/v1/orgs/{org}/members/{user}
pub async fn change_role(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, user)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<ChangeRole>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::MembersManage).await?;
    let previous = guard_member_change(&mut scope, user, Some(req.role)).await?;
    orgs::update_role(&mut scope.tx, org, user, req.role.as_str()).await?;
    scope
        .audit(
            "membership.role_changed",
            "user",
            user,
            json!({ "from": previous, "to": req.role }),
        )
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/orgs/{org}/members/{user}
pub async fn remove_member(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, user)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::MembersManage).await?;
    let previous = guard_member_change(&mut scope, user, None).await?;
    orgs::delete_membership(&mut scope.tx, org, user).await?;
    scope
        .audit(
            "membership.removed",
            "user",
            user,
            json!({ "role": previous }),
        )
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInvitation {
    pub email: String,
    pub role: Role,
}

#[derive(Serialize)]
pub struct InvitationView {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub status: &'static str,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "ts::rfc3339")]
    pub expires_at: OffsetDateTime,
}

#[derive(Serialize)]
pub struct IssuedInvitation {
    pub invitation: InvitationView,
    /// Shown once. QUORUM does not send email in this release; deliver the
    /// token to the invitee through a trusted channel.
    pub token: String,
}

fn invitation_view(r: orgs::InvitationRow) -> InvitationView {
    let now = OffsetDateTime::now_utc();
    let status = if r.accepted_at.is_some() {
        "accepted"
    } else if r.revoked_at.is_some() {
        "revoked"
    } else if r.expires_at <= now {
        "expired"
    } else {
        "pending"
    };
    InvitationView {
        id: r.id,
        email: r.email_normalized,
        role: r.role,
        status,
        created_at: r.created_at,
        expires_at: r.expires_at,
    }
}

/// POST /api/v1/orgs/{org}/invitations
pub async fn invite(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiJson(req): ApiJson<CreateInvitation>,
) -> ApiResult<(StatusCode, Json<IssuedInvitation>)> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::MembersManage).await?;
    if !scope.role.may_assign(req.role) {
        return Err(ApiError::Forbidden);
    }
    let email = validate::email(&req.email)?;
    let token = quorum_auth::issue_token().map_err(|_| ApiError::Internal)?;
    let ttl =
        time::Duration::try_from(state.config().invitation_ttl).map_err(|_| ApiError::Internal)?;
    let expires_at = OffsetDateTime::now_utc()
        .checked_add(ttl)
        .ok_or(ApiError::Internal)?;
    let id = InvitationId::new().as_uuid();
    let row = orgs::insert_invitation(
        &mut scope.tx,
        id,
        org,
        &email,
        req.role.as_str(),
        &token.digest,
        scope.user.as_uuid(),
        expires_at,
    )
    .await?;
    scope
        .audit(
            "invitation.created",
            "invitation",
            id,
            json!({ "role": req.role }),
        )
        .await?;
    scope.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(IssuedInvitation {
            invitation: invitation_view(row),
            token: token.secret,
        }),
    ))
}

/// GET /api/v1/orgs/{org}/invitations
pub async fn invitations(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<InvitationView>>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::MembersManage).await?;
    let rows = orgs::list_invitations(&mut scope.tx, org).await?;
    scope.commit().await?;
    Ok(Json(rows.into_iter().map(invitation_view).collect()))
}

/// DELETE /api/v1/orgs/{org}/invitations/{id}
pub async fn revoke_invitation(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::MembersManage).await?;
    if !orgs::revoke_invitation(&mut scope.tx, org, id).await? {
        return Err(ApiError::NotFound("Pending invitation"));
    }
    scope
        .audit("invitation.revoked", "invitation", id, json!({}))
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptInvitation {
    pub token: String,
}

#[derive(Serialize)]
pub struct AcceptedInvitation {
    pub organization_id: Uuid,
    pub role: String,
}

/// POST /api/v1/invitations/accept — the signed-in user joins the inviting
/// organization. The invitation must be addressed to the user's email.
pub async fn accept(
    State(state): State<AppState>,
    session: Session,
    ApiJson(req): ApiJson<AcceptInvitation>,
) -> ApiResult<Json<AcceptedInvitation>> {
    let not_found =
        || ApiError::NotFound("Invitation (it may have expired, been used or been revoked)");
    if !quorum_auth::plausible_token(req.token.trim()) {
        return Err(not_found());
    }
    let digest = quorum_auth::token_digest(req.token.trim());
    let pending = {
        let mut conn = state.pool().acquire().await?;
        orgs::find_pending_invitation(&mut conn, &digest)
            .await?
            .ok_or_else(not_found)?
    };
    let org = OrganizationId::from_uuid(pending.organization_id);
    let mut tx = quorum_storage::begin(state.pool(), Some(session.user_id), Some(org)).await?;
    let me = identity::get_user(&mut tx, session.user_id.as_uuid())
        .await?
        .ok_or(ApiError::Unauthenticated)?;
    let email = validate::email(&me.email)?;
    if email != pending.email_normalized {
        // Do not reveal which address the invitation was for.
        return Err(not_found());
    }
    if orgs::role_of(&mut tx, org.as_uuid(), session.user_id.as_uuid())
        .await?
        .is_some()
    {
        return Err(ApiError::conflict(
            "You are already a member of this organization.",
        ));
    }
    if !orgs::mark_invitation_accepted(&mut tx, pending.id, session.user_id.as_uuid()).await? {
        return Err(not_found());
    }
    orgs::insert_membership(
        &mut tx,
        org.as_uuid(),
        session.user_id.as_uuid(),
        &pending.role,
    )
    .await?;
    audit::record(
        &mut tx,
        &audit::NewAuditEvent {
            organization_id: Some(org.as_uuid()),
            actor_user_id: Some(session.user_id.as_uuid()),
            event_type: "invitation.accepted",
            resource_type: Some("invitation"),
            resource_id: Some(pending.id),
            outcome: "success",
            correlation_id: &request_id::current(),
            details: json!({ "role": pending.role }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(AcceptedInvitation {
        organization_id: org.as_uuid(),
        role: pending.role,
    }))
}
