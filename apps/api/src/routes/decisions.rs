//! QUORUM Decision register: versioned, human-owned decision records.

use super::projects::require_open_project;
use super::{ApiJson, ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use quorum_domain::Permission;
use quorum_domain::decision::{DecisionContent, DecisionStatus, format_reference};
use quorum_domain::ids::DecisionId;
use quorum_storage::{decisions, orgs, projects};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

#[derive(Serialize)]
pub struct DecisionView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub reference: String,
    pub number: i32,
    #[serde(flatten)]
    pub content: DecisionContent,
    pub decision_maker_id: Option<Uuid>,
    pub decision_maker_name: Option<String>,
    #[serde(with = "iso_date::option")]
    pub review_date: Option<Date>,
    pub status: String,
    pub superseded_by: Option<Uuid>,
    pub version: i32,
    pub created_by: Uuid,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "ts::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<decisions::DecisionRow> for DecisionView {
    fn from(r: decisions::DecisionRow) -> Self {
        let content = r.content();
        Self {
            id: r.id,
            project_id: r.project_id,
            reference: format_reference(r.number),
            number: r.number,
            content,
            decision_maker_id: r.decision_maker_id,
            decision_maker_name: r.decision_maker_name,
            review_date: r.review_date,
            status: r.status,
            superseded_by: r.superseded_by,
            version: r.version,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// GET /api/v1/orgs/{org}/projects/{project}/decisions
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(page): ApiQuery<Page>,
) -> ApiResult<Json<Paged<DecisionView>>> {
    let (limit, offset) = page.bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if projects::get(&mut scope.tx, org, project).await?.is_none() {
        return Err(ApiError::NotFound("Project"));
    }
    let rows = decisions::list(&mut scope.tx, org, project, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter().map(Into::into).collect(),
        limit,
        offset,
    )))
}

#[derive(Deserialize)]
pub struct DecisionInput {
    #[serde(flatten)]
    pub content: DecisionContent,
    pub decision_maker_id: Option<Uuid>,
    #[serde(default, with = "iso_date::option")]
    pub review_date: Option<Date>,
}

#[derive(Deserialize)]
pub struct UpdateDecision {
    pub base_version: i32,
    #[serde(flatten)]
    pub input: DecisionInput,
}

async fn check_decision_maker(scope: &mut OrgScope, user: Option<Uuid>) -> ApiResult<()> {
    if let Some(user) = user {
        let role = orgs::role_of(&mut scope.tx, scope.org.as_uuid(), user).await?;
        let can_decide = role
            .and_then(|r| r.parse::<quorum_domain::Role>().ok())
            .is_some_and(|r| r.allows(Permission::DecisionsDecide));
        if !can_decide {
            return Err(ApiError::Validation {
                field: "decision_maker_id".into(),
                message: "must be a member of this organization who may decide".into(),
            });
        }
    }
    Ok(())
}

async fn load(scope: &mut OrgScope, id: Uuid, lock: bool) -> ApiResult<decisions::DecisionRow> {
    decisions::get(&mut scope.tx, scope.org.as_uuid(), id, lock)
        .await?
        .ok_or(ApiError::NotFound("Decision"))
}

/// POST /api/v1/orgs/{org}/projects/{project}/decisions
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<DecisionInput>,
) -> ApiResult<(StatusCode, Json<DecisionView>)> {
    let content = req.content.validated()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsWrite).await?;
    // Locks the project row, which also serializes decision numbering.
    require_open_project(&mut scope, project).await?;
    check_decision_maker(&mut scope, req.decision_maker_id).await?;
    let id = DecisionId::new().as_uuid();
    let number = decisions::insert(
        &mut scope.tx,
        id,
        org,
        project,
        &content,
        req.decision_maker_id,
        req.review_date,
        scope.user.as_uuid(),
    )
    .await?;
    projects::touch(&mut scope.tx, org, project).await?;
    scope
        .audit(
            "decision.created",
            "decision",
            id,
            json!({ "project_id": project, "reference": format_reference(number) }),
        )
        .await?;
    let row = load(&mut scope, id, false).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// GET /api/v1/orgs/{org}/decisions/{id}
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<DecisionView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let row = load(&mut scope, id, false).await?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

/// PATCH /api/v1/orgs/{org}/decisions/{id} — edits a draft.
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<UpdateDecision>,
) -> ApiResult<Json<DecisionView>> {
    let content = req.input.content.validated()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsWrite).await?;
    let current = load(&mut scope, id, false).await?;
    require_open_project(&mut scope, current.project_id).await?;
    let current = load(&mut scope, id, true).await?;
    if !DecisionStatus::parse(&current.status)?.is_editable() {
        return Err(ApiError::conflict(
            "Only draft decisions can be edited. Return it to draft or record a new decision.",
        ));
    }
    if current.version != req.base_version {
        return Err(ApiError::conflict(format!(
            "The decision changed since you opened it (now version {}). Reload and reapply your edits.",
            current.version
        )));
    }
    check_decision_maker(&mut scope, req.input.decision_maker_id).await?;
    decisions::update_content(
        &mut scope.tx,
        org,
        id,
        &content,
        req.input.decision_maker_id,
        req.input.review_date,
    )
    .await?;
    decisions::snapshot(&mut scope.tx, org, id, "Edited", scope.user.as_uuid()).await?;
    scope
        .audit("decision.updated", "decision", id, json!({}))
        .await?;
    let row = load(&mut scope, id, false).await?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub to: DecisionStatus,
    pub base_version: i32,
    /// Required when superseding: the approved decision that replaces this one.
    pub superseded_by: Option<Uuid>,
    #[serde(default)]
    pub note: String,
    /// Approving, rejecting and superseding are consequential and require an
    /// explicit human confirmation flag.
    #[serde(default)]
    pub confirm: bool,
}

/// POST /api/v1/orgs/{org}/decisions/{id}/transition
pub async fn transition(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<Transition>,
) -> ApiResult<Json<DecisionView>> {
    let note = quorum_domain::validate::single_line("note", &req.note, 0, 500)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let current = load(&mut scope, id, false).await?;
    require_open_project(&mut scope, current.project_id).await?;
    let current = load(&mut scope, id, true).await?;
    let from = DecisionStatus::parse(&current.status)?;
    let required = from.transition(req.to)?;
    scope.require(required)?;
    if current.version != req.base_version {
        return Err(ApiError::conflict(
            "The decision changed since you opened it. Reload and review it again.",
        ));
    }
    if required == Permission::DecisionsDecide && !req.confirm {
        return Err(ApiError::Validation {
            field: "confirm".into(),
            message: "explicit confirmation is required for this decision".into(),
        });
    }
    if req.to == DecisionStatus::UnderReview {
        current.content().ready_for_review()?;
    }
    let superseded_by = if req.to == DecisionStatus::Superseded {
        let by = req.superseded_by.ok_or(ApiError::Validation {
            field: "superseded_by".into(),
            message: "required when superseding".into(),
        })?;
        if by == id {
            return Err(ApiError::Validation {
                field: "superseded_by".into(),
                message: "a decision cannot supersede itself".into(),
            });
        }
        let replacement = decisions::get(&mut scope.tx, org, by, false)
            .await?
            .ok_or(ApiError::NotFound("Superseding decision"))?;
        if replacement.project_id != current.project_id
            || replacement.status != DecisionStatus::Approved.as_str()
        {
            return Err(ApiError::Validation {
                field: "superseded_by".into(),
                message: "must be an approved decision in the same project".into(),
            });
        }
        Some(by)
    } else {
        None
    };
    decisions::set_status(&mut scope.tx, org, id, req.to.as_str(), superseded_by).await?;
    let label = if note.is_empty() {
        format!("Status: {} → {}", from.as_str(), req.to.as_str())
    } else {
        format!("Status: {} → {} — {}", from.as_str(), req.to.as_str(), note)
    };
    decisions::snapshot(&mut scope.tx, org, id, &label, scope.user.as_uuid()).await?;
    scope
        .audit(
            "decision.status_changed",
            "decision",
            id,
            json!({ "from": from, "to": req.to, "superseded_by": superseded_by }),
        )
        .await?;
    let row = load(&mut scope, id, false).await?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

#[derive(Serialize)]
pub struct DecisionVersionView {
    pub version: i32,
    pub status: String,
    pub snapshot: serde_json::Value,
    pub note: String,
    pub changed_by_name: String,
    #[serde(with = "ts::rfc3339")]
    pub changed_at: OffsetDateTime,
}

/// GET /api/v1/orgs/{org}/decisions/{id}/versions
pub async fn versions(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<DecisionVersionView>>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    load(&mut scope, id, false).await?;
    let rows = decisions::list_versions(&mut scope.tx, org, id).await?;
    scope.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|v| DecisionVersionView {
                version: v.version,
                status: v.status,
                snapshot: v.snapshot,
                note: v.note,
                changed_by_name: v.changed_by_name,
                changed_at: v.changed_at,
            })
            .collect(),
    ))
}
