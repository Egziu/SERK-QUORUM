//! QUORUM Council: structured human review of several AI responses to the
//! same brief. Council does not decide anything by itself — people annotate,
//! rate and record the decision, which lands in the Decision register.

use super::connect::ResponseView;
use super::projects::require_open_project;
use super::{ApiJson, ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use quorum_domain::Permission;
use quorum_domain::council::{self, AnnotationKind, CouncilStatus, Criterion};
use quorum_domain::decision::{DecisionContent, format_reference};
use quorum_domain::document::DocumentCategory;
use quorum_storage::{connect as responses, council as store, decisions, documents, projects};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use time::OffsetDateTime;
use uuid::Uuid;

pub const MAX_POSITIONS: i16 = 12;

#[derive(Serialize)]
pub struct CouncilSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub question: String,
    pub status: String,
    pub response_count: i64,
    pub decision_id: Option<Uuid>,
    pub version: i32,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "ts::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<store::CouncilRow> for CouncilSummary {
    fn from(c: store::CouncilRow) -> Self {
        Self {
            id: c.id,
            project_id: c.project_id,
            title: c.title,
            question: c.question,
            status: c.status,
            response_count: c.response_count,
            decision_id: c.decision_id,
            version: c.version,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct MemberView {
    pub position: i16,
    pub response: ResponseView,
    #[serde(with = "ts::rfc3339")]
    pub added_at: OffsetDateTime,
}

#[derive(Serialize)]
pub struct RetiredView {
    pub position: i16,
    pub response_id: Uuid,
    pub replaced_by: Option<Uuid>,
    #[serde(with = "ts::rfc3339::option")]
    pub removed_at: Option<OffsetDateTime>,
}

#[derive(Serialize)]
pub struct AnnotationView {
    pub id: Uuid,
    pub response_id: Option<Uuid>,
    pub kind: String,
    pub quote: String,
    pub body: String,
    pub author_id: Uuid,
    pub author_name: String,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Serialize)]
pub struct RatingView {
    pub response_id: Uuid,
    pub criterion: String,
    pub score: i16,
    pub reason: String,
    pub rated_by: Uuid,
    pub rated_by_name: String,
}

#[derive(Serialize)]
pub struct CriterionView {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Serialize)]
pub struct DecisionLink {
    pub id: Uuid,
    pub reference: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct CouncilDetail {
    #[serde(flatten)]
    pub council: CouncilSummary,
    pub decision_summary: String,
    pub decision: Option<DecisionLink>,
    pub members: Vec<MemberView>,
    pub retired: Vec<RetiredView>,
    pub annotations: Vec<AnnotationView>,
    pub ratings: Vec<RatingView>,
    pub criteria: Vec<CriterionView>,
    pub can_edit: bool,
    pub can_decide: bool,
}

async fn load(scope: &mut OrgScope, id: Uuid, lock: bool) -> ApiResult<store::CouncilRow> {
    store::get(&mut scope.tx, scope.org.as_uuid(), id, lock)
        .await?
        .ok_or(ApiError::NotFound("Council"))
}

async fn detail(scope: &mut OrgScope, id: Uuid) -> ApiResult<CouncilDetail> {
    let org = scope.org.as_uuid();
    let row = load(scope, id, false).await?;
    let mut members = Vec::new();
    let mut retired = Vec::new();
    for m in store::members(&mut scope.tx, org, id).await? {
        if m.removed_at.is_some() {
            retired.push(RetiredView {
                position: m.position,
                response_id: m.response_id,
                replaced_by: m.replaced_by,
                removed_at: m.removed_at,
            });
        } else {
            let r = responses::get_response(&mut scope.tx, org, m.response_id)
                .await?
                .ok_or(ApiError::Internal)?;
            members.push(MemberView {
                position: m.position,
                response: r.into(),
                added_at: m.added_at,
            });
        }
    }
    let annotations = store::annotations(&mut scope.tx, org, id)
        .await?
        .into_iter()
        .map(|a| AnnotationView {
            id: a.id,
            response_id: a.response_id,
            kind: a.kind,
            quote: a.quote,
            body: a.body,
            author_id: a.author_id,
            author_name: a.author_name,
            created_at: a.created_at,
        })
        .collect();
    let ratings = store::ratings(&mut scope.tx, org, id)
        .await?
        .into_iter()
        .map(|r| RatingView {
            response_id: r.response_id,
            criterion: r.criterion,
            score: r.score,
            reason: r.reason,
            rated_by: r.rated_by,
            rated_by_name: r.rated_by_name,
        })
        .collect();
    let decision = match row.decision_id {
        Some(d) => decisions::get(&mut scope.tx, org, d, false)
            .await?
            .map(|d| DecisionLink {
                id: d.id,
                reference: format_reference(d.number),
                status: d.status,
            }),
        None => None,
    };
    let open = CouncilStatus::parse(&row.status)?.is_open();
    Ok(CouncilDetail {
        decision_summary: row.decision_summary.clone(),
        decision,
        members,
        retired,
        annotations,
        ratings,
        criteria: Criterion::ALL
            .into_iter()
            .map(|c| CriterionView {
                key: c.as_str(),
                label: c.label(),
            })
            .collect(),
        can_edit: open && scope.role.allows(Permission::DecisionsWrite),
        can_decide: open && scope.role.allows(Permission::DecisionsDecide),
        council: row.into(),
    })
}

/// Loads a council for a change, locking it and refusing completed councils.
async fn open_for_change(scope: &mut OrgScope, id: Uuid) -> ApiResult<store::CouncilRow> {
    let current = load(scope, id, false).await?;
    require_open_project(scope, current.project_id).await?;
    let current = load(scope, id, true).await?;
    if !CouncilStatus::parse(&current.status)?.is_open() {
        return Err(ApiError::conflict(
            "This council is completed and can no longer be changed.",
        ));
    }
    Ok(current)
}

fn require_review_role(scope: &OrgScope) -> ApiResult<()> {
    if scope.role.allows(Permission::DecisionsWrite)
        || scope.role.allows(Permission::DecisionsDecide)
    {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

fn check_version(current: &store::CouncilRow, base: i32) -> ApiResult<()> {
    if current.version != base {
        return Err(ApiError::conflict(format!(
            "The council changed since you opened it (now version {}). Reload and try again.",
            current.version
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------- sessions

/// GET /api/v1/orgs/{org}/projects/{project}/councils
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(page): ApiQuery<Page>,
) -> ApiResult<Json<Paged<CouncilSummary>>> {
    let (limit, offset) = page.bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if projects::get(&mut scope.tx, org, project).await?.is_none() {
        return Err(ApiError::NotFound("Project"));
    }
    let rows = store::list(&mut scope.tx, org, project, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter().map(Into::into).collect(),
        limit,
        offset,
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CouncilInput {
    pub title: String,
    #[serde(default)]
    pub question: String,
    /// Responses to place in positions 1..n, in order.
    #[serde(default)]
    pub response_ids: Vec<Uuid>,
}

/// POST /api/v1/orgs/{org}/projects/{project}/councils
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<CouncilInput>,
) -> ApiResult<(StatusCode, Json<CouncilDetail>)> {
    let title = council::title(&req.title)?;
    let question = council::question(&req.question)?;
    if req.response_ids.len() > MAX_POSITIONS as usize {
        return Err(ApiError::Validation {
            field: "response_ids".into(),
            message: format!("a council holds at most {MAX_POSITIONS} responses"),
        });
    }
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsWrite).await?;
    require_open_project(&mut scope, project).await?;
    let id = Uuid::now_v7();
    store::insert(
        &mut scope.tx,
        id,
        org,
        project,
        &title,
        &question,
        scope.user.as_uuid(),
    )
    .await?;
    let mut seen = std::collections::HashSet::new();
    for (i, response) in req.response_ids.iter().enumerate() {
        if !seen.insert(*response) {
            return Err(ApiError::Validation {
                field: "response_ids".into(),
                message: "each response can appear once".into(),
            });
        }
        check_response(&mut scope, project, *response).await?;
        store::add_member(
            &mut scope.tx,
            org,
            id,
            *response,
            i as i16 + 1,
            scope.user.as_uuid(),
        )
        .await?;
    }
    let status = initial_status(req.response_ids.len());
    if status != CouncilStatus::Draft {
        store::set_status(&mut scope.tx, org, id, status.as_str()).await?;
    }
    scope
        .audit(
            "council.created",
            "council",
            id,
            json!({ "project_id": project, "responses": req.response_ids.len() }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// A council with responses is waiting for more; two or more can be compared.
fn initial_status(responses: usize) -> CouncilStatus {
    match responses {
        0 => CouncilStatus::Draft,
        1 => CouncilStatus::AwaitingResponses,
        _ => CouncilStatus::ReadyForReview,
    }
}

async fn check_response(scope: &mut OrgScope, project: Uuid, response: Uuid) -> ApiResult<()> {
    responses::get_response(&mut scope.tx, scope.org.as_uuid(), response)
        .await?
        .filter(|r| r.project_id == project)
        .map(|_| ())
        .ok_or(ApiError::Validation {
            field: "response_id".into(),
            message: "must be a response imported into this project".into(),
        })
}

/// GET /api/v1/orgs/{org}/councils/{council}
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<CouncilDetail>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCouncil {
    pub title: String,
    #[serde(default)]
    pub question: String,
    pub base_version: i32,
}

/// PATCH /api/v1/orgs/{org}/councils/{council}
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<UpdateCouncil>,
) -> ApiResult<Json<CouncilDetail>> {
    let title = council::title(&req.title)?;
    let question = council::question(&req.question)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsWrite).await?;
    let current = open_for_change(&mut scope, id).await?;
    check_version(&current, req.base_version)?;
    store::update_details(&mut scope.tx, org, id, &title, &question).await?;
    scope
        .audit("council.updated", "council", id, json!({}))
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    pub to: CouncilStatus,
    pub base_version: i32,
}

/// POST /api/v1/orgs/{org}/councils/{council}/transition
pub async fn transition(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<TransitionInput>,
) -> ApiResult<Json<CouncilDetail>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let current = load(&mut scope, id, false).await?;
    require_open_project(&mut scope, current.project_id).await?;
    let current = load(&mut scope, id, true).await?;
    check_version(&current, req.base_version)?;
    let from = CouncilStatus::parse(&current.status)?;
    let needed = from.transition(req.to)?;
    if !scope.role.allows(needed) {
        return Err(ApiError::Forbidden);
    }
    store::set_status(&mut scope.tx, org, id, req.to.as_str()).await?;
    scope
        .audit(
            "council.status_changed",
            "council",
            id,
            json!({ "from": from, "to": req.to }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

// ---------------------------------------------------------------- responses in a council

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddResponse {
    pub response_id: Uuid,
    /// 1-based slot. If occupied, the previous response is retired and
    /// recorded as replaced (it stays in the council's history).
    pub position: Option<i16>,
}

/// POST /api/v1/orgs/{org}/councils/{council}/responses
pub async fn add_response(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<AddResponse>,
) -> ApiResult<Json<CouncilDetail>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsWrite).await?;
    let current = open_for_change(&mut scope, id).await?;
    check_response(&mut scope, current.project_id, req.response_id).await?;
    let members = store::members(&mut scope.tx, org, id).await?;
    if members.iter().any(|m| m.response_id == req.response_id) {
        return Err(ApiError::conflict(
            "This response is already part of the council.",
        ));
    }
    let active: Vec<_> = members.iter().filter(|m| m.removed_at.is_none()).collect();
    let position = match req.position {
        Some(p) if (1..=MAX_POSITIONS).contains(&p) => p,
        Some(_) => {
            return Err(ApiError::Validation {
                field: "position".into(),
                message: format!("must be between 1 and {MAX_POSITIONS}"),
            });
        }
        None => (1..=MAX_POSITIONS)
            .find(|p| !active.iter().any(|m| m.position == *p))
            .ok_or(ApiError::conflict("The council is full."))?,
    };
    let replaced = store::active_at(&mut scope.tx, org, id, position).await?;
    if let Some(old) = replaced {
        store::retire(&mut scope.tx, org, id, old, Some(req.response_id)).await?;
    }
    store::add_member(
        &mut scope.tx,
        org,
        id,
        req.response_id,
        position,
        scope.user.as_uuid(),
    )
    .await?;
    let count = active.len() + usize::from(replaced.is_none());
    let status = CouncilStatus::parse(&current.status)?;
    let next = match (status, count) {
        (CouncilStatus::Draft, 1) => Some(CouncilStatus::AwaitingResponses),
        (CouncilStatus::Draft | CouncilStatus::AwaitingResponses, n) if n >= 2 => {
            Some(CouncilStatus::ReadyForReview)
        }
        _ => None,
    };
    match next {
        Some(s) => store::set_status(&mut scope.tx, org, id, s.as_str()).await?,
        None => store::touch(&mut scope.tx, org, id).await?,
    }
    scope
        .audit(
            "council.response_added",
            "council",
            id,
            json!({ "response_id": req.response_id, "position": position, "replaced": replaced }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

/// DELETE /api/v1/orgs/{org}/councils/{council}/responses/{response}
/// Retires the response from the council; the response itself is untouched.
pub async fn remove_response(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id, response)): ApiPath<(Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsWrite).await?;
    open_for_change(&mut scope, id).await?;
    let members = store::members(&mut scope.tx, org, id).await?;
    if !members
        .iter()
        .any(|m| m.response_id == response && m.removed_at.is_none())
    {
        return Err(ApiError::NotFound("Council response"));
    }
    store::retire(&mut scope.tx, org, id, response, None).await?;
    store::touch(&mut scope.tx, org, id).await?;
    scope
        .audit(
            "council.response_removed",
            "council",
            id,
            json!({ "response_id": response }),
        )
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- annotations & ratings

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationInput {
    pub response_id: Option<Uuid>,
    pub kind: AnnotationKind,
    #[serde(default)]
    pub quote: String,
    pub body: String,
}

/// POST /api/v1/orgs/{org}/councils/{council}/annotations
pub async fn annotate(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<AnnotationInput>,
) -> ApiResult<(StatusCode, Json<CouncilDetail>)> {
    let quote = quorum_domain::validate::multi_line("quote", &req.quote, 0, 2_000)?;
    let body = quorum_domain::validate::multi_line("body", &req.body, 1, 5_000)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    require_review_role(&scope)?;
    open_for_change(&mut scope, id).await?;
    if let Some(r) = req.response_id {
        let active = store::members(&mut scope.tx, org, id)
            .await?
            .iter()
            .any(|m| m.response_id == r && m.removed_at.is_none());
        if !active {
            return Err(ApiError::Validation {
                field: "response_id".into(),
                message: "must be a response in this council".into(),
            });
        }
    }
    let annotation = Uuid::now_v7();
    store::add_annotation(
        &mut scope.tx,
        annotation,
        org,
        id,
        req.response_id,
        req.kind.as_str(),
        &quote,
        &body,
        scope.user.as_uuid(),
    )
    .await?;
    store::touch(&mut scope.tx, org, id).await?;
    scope
        .audit(
            "council.annotation_added",
            "council",
            id,
            json!({ "annotation_id": annotation, "kind": req.kind, "response_id": req.response_id }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// DELETE /api/v1/orgs/{org}/councils/{council}/annotations/{annotation} — author only.
pub async fn delete_annotation(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id, annotation)): ApiPath<(Uuid, Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    require_review_role(&scope)?;
    open_for_change(&mut scope, id).await?;
    if !store::delete_annotation(&mut scope.tx, org, id, annotation, scope.user.as_uuid()).await? {
        return Err(ApiError::NotFound("Annotation"));
    }
    store::touch(&mut scope.tx, org, id).await?;
    scope
        .audit(
            "council.annotation_deleted",
            "council",
            id,
            json!({ "annotation_id": annotation }),
        )
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatingInput {
    pub response_id: Uuid,
    pub criterion: Criterion,
    pub score: i16,
    #[serde(default)]
    pub reason: String,
}

/// PUT /api/v1/orgs/{org}/councils/{council}/ratings — one person's rating of
/// one response on one criterion. Ratings are human judgements, never computed.
pub async fn rate(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<RatingInput>,
) -> ApiResult<Json<CouncilDetail>> {
    let score = council::rating_score(req.score)?;
    let reason = quorum_domain::validate::multi_line("reason", &req.reason, 0, 2_000)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    require_review_role(&scope)?;
    open_for_change(&mut scope, id).await?;
    let active = store::members(&mut scope.tx, org, id)
        .await?
        .iter()
        .any(|m| m.response_id == req.response_id && m.removed_at.is_none());
    if !active {
        return Err(ApiError::Validation {
            field: "response_id".into(),
            message: "must be a response in this council".into(),
        });
    }
    store::upsert_rating(
        &mut scope.tx,
        org,
        id,
        req.response_id,
        req.criterion.as_str(),
        score,
        &reason,
        scope.user.as_uuid(),
    )
    .await?;
    store::touch(&mut scope.tx, org, id).await?;
    scope
        .audit(
            "council.rated",
            "council",
            id,
            json!({ "response_id": req.response_id, "criterion": req.criterion, "score": score }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

// ---------------------------------------------------------------- decision

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionInput {
    pub summary: String,
    #[serde(default)]
    pub title: String,
    pub base_version: i32,
    /// The person explicitly confirms that this is their decision.
    #[serde(default)]
    pub confirm: bool,
}

/// POST /api/v1/orgs/{org}/councils/{council}/decision — records the human
/// decision, creates a draft record in the Decision register linking back to
/// this council, and freezes the council.
pub async fn record_decision(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<DecisionInput>,
) -> ApiResult<Json<CouncilDetail>> {
    if !req.confirm {
        return Err(ApiError::Validation {
            field: "confirm".into(),
            message: "confirm that this decision is yours to record".into(),
        });
    }
    let summary = council::decision_summary(&req.summary)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::DecisionsDecide).await?;
    let current = open_for_change(&mut scope, id).await?;
    check_version(&current, req.base_version)?;
    let status = CouncilStatus::parse(&current.status)?;
    if !matches!(
        status,
        CouncilStatus::ReadyForReview | CouncilStatus::UnderReview
    ) {
        return Err(ApiError::conflict(
            "Compare at least two responses before recording a decision.",
        ));
    }
    let report = markdown(&mut scope, &current).await?;
    let title = if req.title.trim().is_empty() {
        current.title.clone()
    } else {
        req.title.clone()
    };
    let content = DecisionContent {
        title,
        problem_statement: current.question.clone(),
        context: format!("Recorded from QUORUM Council \"{}\".", current.title),
        alternatives: alternatives(&mut scope, id).await?,
        evaluation_criteria: Criterion::ALL
            .iter()
            .map(|c| c.label())
            .collect::<Vec<_>>()
            .join(", "),
        evidence: truncate(&report, 19_000),
        outcome: summary.clone(),
        consequences: String::new(),
        risks: risks(&mut scope, id).await?,
    }
    .validated()?;
    let decision = Uuid::now_v7();
    let number = decisions::insert(
        &mut scope.tx,
        decision,
        org,
        current.project_id,
        &content,
        Some(scope.user.as_uuid()),
        None,
        scope.user.as_uuid(),
    )
    .await?;
    store::record_decision(&mut scope.tx, org, id, &summary, decision).await?;
    projects::touch(&mut scope.tx, org, current.project_id).await?;
    scope
        .audit(
            "council.decision_recorded",
            "council",
            id,
            json!({ "decision_id": decision, "reference": format_reference(number) }),
        )
        .await?;
    scope
        .audit(
            "decision.created",
            "decision",
            decision,
            json!({ "project_id": current.project_id, "reference": format_reference(number), "council_id": id }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push_str("\n\n[Truncated. Export the council for the full record.]");
        out
    }
}

async fn alternatives(scope: &mut OrgScope, id: Uuid) -> ApiResult<String> {
    let org = scope.org.as_uuid();
    let mut out = String::new();
    for m in store::members(&mut scope.tx, org, id).await? {
        if m.removed_at.is_some() {
            continue;
        }
        if let Some(r) = responses::get_response(&mut scope.tx, org, m.response_id).await? {
            let _ = writeln!(
                out,
                "{}. {} — {}{} ({})",
                m.position,
                r.role_name.as_deref().unwrap_or("Response"),
                r.provider,
                if r.model.is_empty() {
                    String::new()
                } else {
                    format!(" / {}", r.model)
                },
                r.provenance
            );
        }
    }
    Ok(out)
}

async fn risks(scope: &mut OrgScope, id: Uuid) -> ApiResult<String> {
    let mut out = String::new();
    for a in store::annotations(&mut scope.tx, scope.org.as_uuid(), id).await? {
        if matches!(a.kind.as_str(), "risk" | "contradiction" | "assumption") {
            let _ = writeln!(out, "- [{}] {}", a.kind, a.body.replace('\n', " "));
        }
    }
    Ok(truncate(&out, 19_000))
}

// ---------------------------------------------------------------- export

async fn markdown(scope: &mut OrgScope, c: &store::CouncilRow) -> ApiResult<String> {
    let org = scope.org.as_uuid();
    let project = projects::get(&mut scope.tx, org, c.project_id)
        .await?
        .ok_or(ApiError::Internal)?;
    let mut md = String::new();
    let _ = writeln!(md, "# Council: {}\n", c.title);
    let _ = writeln!(md, "- Project: {}", project.name);
    let _ = writeln!(md, "- Status: {}", c.status);
    let _ = writeln!(md, "- Council version: {}", c.version);
    if !c.question.is_empty() {
        let _ = writeln!(md, "\n## Question\n\n{}", c.question);
    }
    if !c.decision_summary.is_empty() {
        let _ = writeln!(md, "\n## Human decision\n\n{}", c.decision_summary);
    }
    let ratings = store::ratings(&mut scope.tx, org, c.id).await?;
    let annotations = store::annotations(&mut scope.tx, org, c.id).await?;
    let _ = writeln!(md, "\n## Responses");
    for m in store::members(&mut scope.tx, org, c.id).await? {
        if m.removed_at.is_some() {
            continue;
        }
        let Some(r) = responses::get_response(&mut scope.tx, org, m.response_id).await? else {
            continue;
        };
        let _ = writeln!(
            md,
            "\n### {}. {} ({}{})\n",
            m.position,
            r.role_name.as_deref().unwrap_or("Response"),
            r.provider,
            if r.model.is_empty() {
                String::new()
            } else {
                format!(" / {}", r.model)
            }
        );
        let _ = writeln!(
            md,
            "- Provenance: {} via {}; imported by {}",
            r.provenance, r.source_mode, r.imported_by_name
        );
        let digest: String = r
            .content_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let _ = writeln!(md, "- SHA-256: `{digest}`");
        let mut per: BTreeMap<&str, Vec<i16>> = BTreeMap::new();
        for x in ratings.iter().filter(|x| x.response_id == r.id) {
            per.entry(x.criterion.as_str()).or_default().push(x.score);
        }
        if !per.is_empty() {
            let _ = writeln!(md, "- Human ratings:");
            for (k, v) in per {
                let list = v.iter().map(i16::to_string).collect::<Vec<_>>().join(", ");
                let _ = writeln!(md, "  - {k}: {list}");
            }
        }
        for a in annotations.iter().filter(|a| a.response_id == Some(r.id)) {
            let _ = writeln!(
                md,
                "- {} ({}): {}",
                a.kind,
                a.author_name,
                a.body.replace('\n', " ")
            );
        }
        let _ = writeln!(md, "\n{}\n", r.body);
    }
    let general: Vec<_> = annotations
        .iter()
        .filter(|a| a.response_id.is_none())
        .collect();
    if !general.is_empty() {
        let _ = writeln!(md, "\n## Council notes\n");
        for a in general {
            let _ = writeln!(
                md,
                "- {} ({}): {}",
                a.kind,
                a.author_name,
                a.body.replace('\n', " ")
            );
        }
    }
    let _ = writeln!(
        md,
        "\n---\nResponses marked user_declared were pasted or uploaded by a person; QUORUM did not verify which provider or model produced them."
    );
    Ok(md)
}

/// GET /api/v1/orgs/{org}/councils/{council}/export.md
pub async fn export_markdown(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Response> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let c = load(&mut scope, id, false).await?;
    let md = markdown(&mut scope, &c).await?;
    scope
        .audit(
            "council.exported",
            "council",
            id,
            json!({ "format": "markdown" }),
        )
        .await?;
    scope.commit().await?;
    let mut res = md.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"council.md\""),
    );
    Ok(res)
}

#[derive(Serialize)]
pub struct ExportedDocument {
    pub document_id: Uuid,
}

/// POST /api/v1/orgs/{org}/councils/{council}/export-document — saves the
/// council report as a new Studio document (marked AI-assisted).
pub async fn export_document(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<(StatusCode, Json<ExportedDocument>)> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::DocumentsWrite).await?;
    let c = load(&mut scope, id, false).await?;
    require_open_project(&mut scope, c.project_id).await?;
    let body = truncate(&markdown(&mut scope, &c).await?, 190_000);
    let title: String = format!("Council report: {}", c.title)
        .chars()
        .take(200)
        .collect();
    let document = Uuid::now_v7();
    documents::insert(
        &mut scope.tx,
        document,
        org,
        c.project_id,
        &title,
        DocumentCategory::General.as_str(),
        &body,
        true,
        scope.user.as_uuid(),
    )
    .await?;
    projects::touch(&mut scope.tx, org, c.project_id).await?;
    scope
        .audit(
            "document.created",
            "document",
            document,
            json!({ "project_id": c.project_id, "source": "council", "council_id": id, "ai_assisted": true }),
        )
        .await?;
    scope.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(ExportedDocument {
            document_id: document,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_status_follows_response_count() {
        assert_eq!(initial_status(0), CouncilStatus::Draft);
        assert_eq!(initial_status(1), CouncilStatus::AwaitingResponses);
        assert_eq!(initial_status(4), CouncilStatus::ReadyForReview);
    }

    #[test]
    fn truncation_marks_the_cut() {
        assert_eq!(truncate("abc", 5), "abc");
        assert!(truncate("abcdef", 3).starts_with("abc\n\n[Truncated"));
    }
}
