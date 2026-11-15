//! QUORUM Connect — Bring Your Own AI.
//!
//! Web Connect works with no API credentials: QUORUM prepares the exact
//! instruction, the person copies it, opens the provider's official website
//! themselves and pastes the answer back. QUORUM never contacts the provider
//! website, never automates it and never handles provider credentials.
//! Imported answers are recorded as *user-declared*; only answers QUORUM
//! fetched itself through an API are *provider-reported*.

use super::projects::require_open_project;
use super::{ApiJson, ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use quorum_domain::Permission;
use quorum_domain::connect::{
    self, BriefContent, BriefMode, Classification, ContextItem, INSTRUCTION_MAX, RoleTemplate,
    default_roles, render_instruction,
};
use quorum_domain::validate;
use quorum_providers::{ProviderId, ProviderStatus};
use quorum_storage::{connect as store, decisions, documents, projects};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

pub const RESPONSE_MAX_CHARS: usize = 200_000;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest(text: &str) -> [u8; 32] {
    quorum_auth::token_digest(text)
}

// ---------------------------------------------------------------- catalog

#[derive(Serialize)]
pub struct ProviderCard {
    pub id: ProviderId,
    pub vendor: &'static str,
    pub web_product: &'static str,
    pub website_url: &'static str,
    /// Web Connect is always available unless policy forbids external sharing entirely.
    pub web_status: &'static str,
    pub api_status: ProviderStatus,
    pub api_model: Option<String>,
    pub data_handling: &'static str,
}

#[derive(Serialize)]
pub struct ConnectCatalog {
    pub providers: Vec<ProviderCard>,
    pub roles: Vec<RoleTemplate>,
    pub classifications: Vec<Classification>,
    pub external_share_limit: Classification,
    pub notice: &'static str,
}

async fn org_limit(scope: &mut OrgScope) -> ApiResult<Classification> {
    let raw = store::external_share_limit(&mut scope.tx, scope.org.as_uuid()).await?;
    Ok(Classification::parse(&raw)?)
}

async fn all_roles(scope: &mut OrgScope) -> ApiResult<Vec<RoleTemplate>> {
    let mut roles = default_roles();
    for r in store::list_roles(&mut scope.tx, scope.org.as_uuid()).await? {
        roles.push(RoleTemplate {
            key: r.id.to_string(),
            name: r.name,
            focus: r.focus,
            guidance: r.guidance,
            built_in: false,
            suggested_provider: None,
        });
    }
    Ok(roles)
}

/// GET /api/v1/orgs/{org}/connect
pub async fn catalog(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
) -> ApiResult<Json<ConnectCatalog>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let limit = org_limit(&mut scope).await?;
    let roles = all_roles(&mut scope).await?;
    scope.commit().await?;
    let settings = &state.config().providers;
    Ok(Json(ConnectCatalog {
        providers: ProviderId::ALL
            .into_iter()
            .map(|p| ProviderCard {
                id: p,
                vendor: p.vendor(),
                web_product: p.web_product(),
                website_url: p.website_url(),
                web_status: "available",
                api_status: settings.status(p),
                api_model: settings.model(p).map(str::to_owned),
                data_handling: p.data_handling(),
            })
            .collect(),
        roles,
        classifications: Classification::ALL.to_vec(),
        external_share_limit: limit,
        notice: "Opening a provider website does not tell QUORUM whether you are signed in or subscribed. \
                 QUORUM cannot see or control what you do on external websites.",
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    pub external_share_limit: Classification,
}

/// PUT /api/v1/orgs/{org}/connect/policy
pub async fn set_policy(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiJson(req): ApiJson<PolicyInput>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::MembersManage).await?;
    let before = org_limit(&mut scope).await?;
    store::set_external_share_limit(&mut scope.tx, org, req.external_share_limit.as_str()).await?;
    scope
        .audit(
            "policy.external_share_limit_changed",
            "organization",
            org,
            json!({ "from": before, "to": req.external_share_limit }),
        )
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- roles

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleInput {
    pub name: String,
    #[serde(default)]
    pub focus: String,
    pub guidance: String,
}

/// POST /api/v1/orgs/{org}/roles
pub async fn create_role(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiJson(req): ApiJson<RoleInput>,
) -> ApiResult<(StatusCode, Json<RoleTemplate>)> {
    let name = connect::role_name(&req.name)?;
    let focus = validate::multi_line("focus", &req.focus, 0, 1_000)?;
    let guidance = connect::role_guidance(&req.guidance)?;
    if default_roles()
        .iter()
        .any(|r| r.name.eq_ignore_ascii_case(&name))
    {
        return Err(ApiError::conflict("A built-in role already has this name."));
    }
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProjectsManage).await?;
    let id = Uuid::now_v7();
    store::insert_role(
        &mut scope.tx,
        id,
        org,
        &name,
        &focus,
        &guidance,
        scope.user.as_uuid(),
    )
    .await
    .map_err(|e| match e {
        quorum_storage::StorageError::Conflict => {
            ApiError::conflict("A role with this name already exists.")
        }
        other => other.into(),
    })?;
    scope
        .audit(
            "role_template.created",
            "role_template",
            id,
            json!({ "name": name }),
        )
        .await?;
    scope.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(RoleTemplate {
            key: id.to_string(),
            name,
            focus,
            guidance,
            built_in: false,
            suggested_provider: None,
        }),
    ))
}

/// DELETE /api/v1/orgs/{org}/roles/{role}
pub async fn delete_role(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, role)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProjectsManage).await?;
    if !store::delete_role(&mut scope.tx, org, role).await? {
        return Err(ApiError::NotFound("Role"));
    }
    scope
        .audit("role_template.deleted", "role_template", role, json!({}))
        .await?;
    scope.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- briefs

#[derive(Serialize)]
pub struct BriefView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub mode: String,
    pub classification: String,
    pub version: i32,
    pub content: serde_json::Value,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "ts::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<store::BriefRow> for BriefView {
    fn from(b: store::BriefRow) -> Self {
        Self {
            id: b.id,
            project_id: b.project_id,
            title: b.title,
            mode: b.mode,
            classification: b.classification,
            version: b.current_version,
            content: b.content,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefInput {
    pub title: String,
    pub mode: BriefMode,
    pub classification: Classification,
    pub content: BriefContent,
    /// Required when updating: the version the editor started from.
    pub base_version: Option<i32>,
}

fn validated(req: BriefInput) -> ApiResult<(String, BriefMode, Classification, serde_json::Value)> {
    let title = validate::single_line("title", &req.title, 1, 200)?;
    let content = req.content.validated(req.mode)?;
    let json = serde_json::to_value(content).map_err(|_| ApiError::Internal)?;
    Ok((title, req.mode, req.classification, json))
}

/// GET /api/v1/orgs/{org}/projects/{project}/briefs
pub async fn list_briefs(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(page): ApiQuery<Page>,
) -> ApiResult<Json<Paged<BriefView>>> {
    let (limit, offset) = page.bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if projects::get(&mut scope.tx, org, project).await?.is_none() {
        return Err(ApiError::NotFound("Project"));
    }
    let rows = store::list_briefs(&mut scope.tx, org, project, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter().map(Into::into).collect(),
        limit,
        offset,
    )))
}

/// POST /api/v1/orgs/{org}/projects/{project}/briefs
pub async fn create_brief(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<BriefInput>,
) -> ApiResult<(StatusCode, Json<BriefView>)> {
    let (title, mode, classification, content) = validated(req)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    require_open_project(&mut scope, project).await?;
    let id = Uuid::now_v7();
    store::insert_brief(
        &mut scope.tx,
        id,
        org,
        project,
        &title,
        mode.as_str(),
        classification.as_str(),
        &content,
        scope.user.as_uuid(),
    )
    .await?;
    scope
        .audit(
            "brief.created",
            "brief",
            id,
            json!({ "project_id": project, "mode": mode, "classification": classification }),
        )
        .await?;
    let row = store::get_brief(&mut scope.tx, org, id, false)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// GET /api/v1/orgs/{org}/briefs/{brief}
pub async fn get_brief(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, brief)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<BriefView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let row = store::get_brief(&mut scope.tx, org, brief, false)
        .await?
        .ok_or(ApiError::NotFound("Brief"))?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

/// PATCH /api/v1/orgs/{org}/briefs/{brief} — appends a new version.
pub async fn update_brief(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, brief)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<BriefInput>,
) -> ApiResult<Json<BriefView>> {
    let base = req.base_version.ok_or(ApiError::Validation {
        field: "base_version".into(),
        message: "required when editing".into(),
    })?;
    let (title, mode, classification, content) = validated(req)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    let current = store::get_brief(&mut scope.tx, org, brief, false)
        .await?
        .ok_or(ApiError::NotFound("Brief"))?;
    require_open_project(&mut scope, current.project_id).await?;
    let current = store::get_brief(&mut scope.tx, org, brief, true)
        .await?
        .ok_or(ApiError::NotFound("Brief"))?;
    if current.current_version != base {
        return Err(ApiError::conflict(format!(
            "The brief changed since you opened it (now version {}). Reload and reapply your edits.",
            current.current_version
        )));
    }
    let version = store::append_brief_version(
        &mut scope.tx,
        org,
        brief,
        base,
        &title,
        mode.as_str(),
        classification.as_str(),
        &content,
        scope.user.as_uuid(),
    )
    .await?;
    scope
        .audit(
            "brief.updated",
            "brief",
            brief,
            json!({ "version": version, "classification": classification }),
        )
        .await?;
    let row = store::get_brief(&mut scope.tx, org, brief, false)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

// ---------------------------------------------------------------- instructions

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ContextRef {
    pub kind: String,
    pub id: Uuid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    pub assignments: Vec<Assignment>,
    #[serde(default)]
    pub context: Vec<ContextRef>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub provider: ProviderId,
    pub role: String,
}

#[derive(Serialize)]
pub struct PreparedInstruction {
    pub provider: ProviderId,
    pub role: String,
    pub role_name: String,
    pub instruction: String,
    pub characters: usize,
    /// Rough size hint (≈ 4 characters per token); not a provider count.
    pub estimated_tokens: usize,
    pub sha256: String,
}

#[derive(Serialize)]
pub struct Preview {
    pub brief_version: i32,
    pub classification: Classification,
    pub external_share_limit: Classification,
    pub sharing_allowed: bool,
    pub context_items: Vec<ContextItemView>,
    pub instructions: Vec<PreparedInstruction>,
}

#[derive(Serialize, Clone)]
pub struct ContextItemView {
    pub kind: String,
    pub id: String,
    pub version: i32,
    pub title: String,
    pub characters: usize,
}

const MAX_CONTEXT_ITEMS: usize = 12;

/// Loads user-selected context items. Only documents and decisions of this
/// organization *and project* can be included; nothing is added implicitly.
async fn load_context(
    scope: &mut OrgScope,
    project: Uuid,
    refs: &[ContextRef],
) -> ApiResult<Vec<ContextItem>> {
    if refs.len() > MAX_CONTEXT_ITEMS {
        return Err(ApiError::Validation {
            field: "context".into(),
            message: format!("select at most {MAX_CONTEXT_ITEMS} items"),
        });
    }
    let org = scope.org.as_uuid();
    let mut out = Vec::with_capacity(refs.len());
    for r in refs {
        match r.kind.as_str() {
            "document" => {
                let doc = documents::get(&mut scope.tx, org, r.id)
                    .await?
                    .filter(|d| d.project_id == project)
                    .ok_or(ApiError::NotFound("Context document"))?;
                let v = documents::get_version(&mut scope.tx, org, r.id, doc.current_version)
                    .await?
                    .ok_or(ApiError::Internal)?;
                out.push(ContextItem {
                    kind: "document".into(),
                    id: r.id.to_string(),
                    version: v.version,
                    title: v.title,
                    text: v.body,
                });
            }
            "decision" => {
                let d = decisions::get(&mut scope.tx, org, r.id, false)
                    .await?
                    .filter(|d| d.project_id == project)
                    .ok_or(ApiError::NotFound("Context decision"))?;
                let text = format!(
                    "Status: {}\nProblem: {}\nDecision: {}\nAlternatives considered: {}",
                    d.status, d.problem_statement, d.outcome, d.alternatives
                );
                out.push(ContextItem {
                    kind: "decision".into(),
                    id: r.id.to_string(),
                    version: d.version,
                    title: format!(
                        "{} {}",
                        quorum_domain::decision::format_reference(d.number),
                        d.title
                    ),
                    text,
                });
            }
            _ => {
                return Err(ApiError::Validation {
                    field: "context".into(),
                    message: "kind must be document or decision".into(),
                });
            }
        }
    }
    Ok(out)
}

fn find_role(roles: &[RoleTemplate], key: &str) -> ApiResult<RoleTemplate> {
    roles
        .iter()
        .find(|r| r.key == key)
        .cloned()
        .ok_or(ApiError::Validation {
            field: "role".into(),
            message: "unknown role".into(),
        })
}

fn prepared(provider: ProviderId, role: &RoleTemplate, instruction: String) -> PreparedInstruction {
    let chars = instruction.chars().count();
    PreparedInstruction {
        provider,
        role: role.key.clone(),
        role_name: role.name.clone(),
        sha256: hex(&digest(&instruction)),
        characters: chars,
        estimated_tokens: chars.div_ceil(4),
        instruction,
    }
}

/// POST /api/v1/orgs/{org}/briefs/{brief}/preview — renders instructions; stores nothing.
pub async fn preview(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, brief)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<PreviewInput>,
) -> ApiResult<Json<Preview>> {
    if req.assignments.is_empty() || req.assignments.len() > 8 {
        return Err(ApiError::Validation {
            field: "assignments".into(),
            message: "choose between 1 and 8 provider/role pairs".into(),
        });
    }
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    let b = store::get_brief(&mut scope.tx, org, brief, false)
        .await?
        .ok_or(ApiError::NotFound("Brief"))?;
    let project = projects::get(&mut scope.tx, org, b.project_id)
        .await?
        .ok_or(ApiError::Internal)?;
    let limit = org_limit(&mut scope).await?;
    let roles = all_roles(&mut scope).await?;
    let context = load_context(&mut scope, b.project_id, &req.context).await?;
    scope.commit().await?;

    let classification = Classification::parse(&b.classification)?;
    let mode = if b.mode == "quick" {
        BriefMode::Quick
    } else {
        BriefMode::Professional
    };
    let content: BriefContent =
        serde_json::from_value(b.content).map_err(|_| ApiError::Internal)?;
    let mut instructions = Vec::new();
    for a in &req.assignments {
        let role = find_role(&roles, &a.role)?;
        instructions.push(prepared(
            a.provider,
            &role,
            render_instruction(&project.name, mode, &content, &role, &context),
        ));
    }
    Ok(Json(Preview {
        brief_version: b.current_version,
        classification,
        external_share_limit: limit,
        sharing_allowed: classification.may_leave(limit),
        context_items: context
            .iter()
            .map(|c| ContextItemView {
                kind: c.kind.clone(),
                id: c.id.clone(),
                version: c.version,
                title: c.title.clone(),
                characters: c.text.chars().count(),
            })
            .collect(),
        instructions,
    }))
}

// ---------------------------------------------------------------- handoffs

#[derive(Serialize)]
pub struct HandoffView {
    pub id: Uuid,
    pub brief_id: Uuid,
    pub brief_version: i32,
    pub provider: String,
    pub role_name: String,
    pub mode: String,
    pub instruction: String,
    pub instruction_sha256: String,
    pub context_items: serde_json::Value,
    pub status: String,
    pub website_url: Option<&'static str>,
    #[serde(with = "ts::rfc3339::option")]
    pub opened_at: Option<OffsetDateTime>,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<store::HandoffRow> for HandoffView {
    fn from(h: store::HandoffRow) -> Self {
        Self {
            website_url: ProviderId::parse(&h.provider).map(ProviderId::website_url),
            id: h.id,
            brief_id: h.brief_id,
            brief_version: h.brief_version,
            provider: h.provider,
            role_name: h.role_name,
            mode: h.mode,
            instruction_sha256: hex(&h.instruction_sha256),
            instruction: h.instruction,
            context_items: h.context_items,
            status: h.status,
            opened_at: h.opened_at,
            created_at: h.created_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffInput {
    pub provider: ProviderId,
    pub role: String,
    pub base_version: i32,
    #[serde(default)]
    pub context: Vec<ContextRef>,
    /// The instruction as the person approved it (possibly edited). If absent,
    /// the rendered instruction is used.
    pub instruction: Option<String>,
}

/// POST /api/v1/orgs/{org}/briefs/{brief}/handoffs — records the approved
/// outbound instruction for one provider. Refused when the brief's
/// classification exceeds the organization's external sharing limit.
pub async fn create_handoff(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, brief)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<HandoffInput>,
) -> ApiResult<(StatusCode, Json<HandoffView>)> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    let b = store::get_brief(&mut scope.tx, org, brief, false)
        .await?
        .ok_or(ApiError::NotFound("Brief"))?;
    require_open_project(&mut scope, b.project_id).await?;
    if b.current_version != req.base_version {
        return Err(ApiError::conflict(
            "The brief changed. Review the latest version before sharing it.",
        ));
    }
    let classification = Classification::parse(&b.classification)?;
    let limit = org_limit(&mut scope).await?;
    if !classification.may_leave(limit) {
        drop(scope);
        return Err(ApiError::PolicyBlocked(format!(
            "This brief is classified {} and your organization allows at most {} material to be shared with external AI providers.",
            classification.as_str(),
            limit.as_str()
        )));
    }
    let project = projects::get(&mut scope.tx, org, b.project_id)
        .await?
        .ok_or(ApiError::Internal)?;
    let roles = all_roles(&mut scope).await?;
    let role = find_role(&roles, &req.role)?;
    let context = load_context(&mut scope, b.project_id, &req.context).await?;
    let mode = if b.mode == "quick" {
        BriefMode::Quick
    } else {
        BriefMode::Professional
    };
    let content: BriefContent =
        serde_json::from_value(b.content.clone()).map_err(|_| ApiError::Internal)?;
    let instruction = match req.instruction {
        Some(text) => validate::multi_line("instruction", &text, 1, INSTRUCTION_MAX)?,
        None => render_instruction(&project.name, mode, &content, &role, &context),
    };
    let edited = instruction != render_instruction(&project.name, mode, &content, &role, &context);
    let sha = digest(&instruction);
    let context_items = serde_json::to_value(
        context
            .iter()
            .map(|c| json!({ "kind": c.kind, "id": c.id, "version": c.version, "title": c.title }))
            .collect::<Vec<_>>(),
    )
    .map_err(|_| ApiError::Internal)?;
    let id = Uuid::now_v7();
    store::insert_handoff(
        &mut scope.tx,
        &store::NewHandoff {
            id,
            org,
            project: b.project_id,
            brief,
            brief_version: b.current_version,
            provider: req.provider.as_str(),
            role_name: &role.name,
            instruction: &instruction,
            instruction_sha256: &sha,
            context_items: &context_items,
            by: scope.user.as_uuid(),
        },
    )
    .await?;
    // Metadata only: the instruction text is not copied into the audit trail.
    scope
        .audit(
            "handoff.prepared",
            "handoff",
            id,
            json!({ "provider": req.provider, "role": role.name, "brief_version": b.current_version,
                    "classification": classification, "edited": edited, "context_items": context.len(),
                    "sha256": hex(&sha) }),
        )
        .await?;
    let row = store::get_handoff(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// GET /api/v1/orgs/{org}/briefs/{brief}/handoffs
pub async fn list_handoffs(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, brief)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<HandoffView>>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if store::get_brief(&mut scope.tx, org, brief, false)
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound("Brief"));
    }
    let rows = store::list_handoffs(&mut scope.tx, org, brief).await?;
    scope.commit().await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// POST /api/v1/orgs/{org}/handoffs/{handoff}/opened — the person opened the
/// provider website. Records the fact only; proves nothing about their account.
pub async fn mark_opened(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, handoff)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<HandoffView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    if store::get_handoff(&mut scope.tx, org, handoff)
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound("Handoff"));
    }
    if store::mark_opened(&mut scope.tx, org, handoff).await? {
        scope
            .audit("handoff.website_opened", "handoff", handoff, json!({}))
            .await?;
    }
    let row = store::get_handoff(&mut scope.tx, org, handoff)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

// ---------------------------------------------------------------- responses

#[derive(Serialize)]
pub struct ResponseView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub handoff_id: Option<Uuid>,
    pub brief_id: Option<Uuid>,
    pub brief_version: Option<i32>,
    pub provider: String,
    pub model: String,
    pub role_name: Option<String>,
    pub source_mode: String,
    pub provenance: String,
    pub provenance_label: &'static str,
    pub body: String,
    pub characters: usize,
    pub content_sha256: String,
    pub reference_url: String,
    pub review_status: String,
    pub imported_by_name: String,
    #[serde(with = "ts::rfc3339")]
    pub imported_at: OffsetDateTime,
}

impl From<store::ResponseRow> for ResponseView {
    fn from(r: store::ResponseRow) -> Self {
        Self {
            provenance_label: if r.provenance == "provider_reported" {
                "Returned through a QUORUM API request"
            } else {
                "Imported by a person; provider and model are their declaration"
            },
            characters: r.body.chars().count(),
            content_sha256: hex(&r.content_sha256),
            id: r.id,
            project_id: r.project_id,
            handoff_id: r.handoff_id,
            brief_id: r.brief_id,
            brief_version: r.brief_version,
            provider: r.provider,
            model: r.model,
            role_name: r.role_name,
            source_mode: r.source_mode,
            provenance: r.provenance,
            body: r.body,
            reference_url: r.reference_url,
            review_status: r.review_status,
            imported_by_name: r.imported_by_name,
            imported_at: r.imported_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportInput {
    pub handoff_id: Option<Uuid>,
    /// Provider the person says produced the text ("other" allowed).
    pub provider: String,
    #[serde(default)]
    pub model: String,
    pub text: String,
    /// "web_import" (pasted) or "file_import" (.txt / .md / QUORUM JSON).
    pub source: String,
    #[serde(default)]
    pub reference_url: String,
}

fn clean_reference_url(raw: &str) -> ApiResult<String> {
    let url = raw.trim();
    if url.is_empty() {
        return Ok(String::new());
    }
    let bad = || ApiError::Validation {
        field: "reference_url".into(),
        message: "must be an https:// address of at most 2000 characters".into(),
    };
    if url.len() > 2000
        || !url.starts_with("https://")
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(bad());
    }
    Ok(url.to_owned())
}

/// Text normalization for imports: unify line endings so the same answer
/// pasted on different systems hashes identically.
fn normalize_import(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned()
}

#[derive(Serialize)]
pub struct DuplicateBody {
    pub error: DuplicateError,
}

#[derive(Serialize)]
pub struct DuplicateError {
    pub code: &'static str,
    pub message: &'static str,
    pub existing_response_id: Uuid,
}

/// POST /api/v1/orgs/{org}/projects/{project}/responses
pub async fn import_response(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<ImportInput>,
) -> ApiResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let body = normalize_import(&req.text);
    let body = validate::multi_line("text", &body, 1, RESPONSE_MAX_CHARS)?;
    let source = match req.source.as_str() {
        "web_import" | "file_import" => req.source.clone(),
        _ => {
            return Err(ApiError::Validation {
                field: "source".into(),
                message: "must be web_import or file_import".into(),
            });
        }
    };
    let provider = match req.provider.as_str() {
        "other" => "other".to_owned(),
        p => ProviderId::parse(p)
            .ok_or(ApiError::Validation {
                field: "provider".into(),
                message: "unknown provider".into(),
            })?
            .as_str()
            .to_owned(),
    };
    let model = validate::single_line("model", &req.model, 0, 120)?;
    let reference_url = clean_reference_url(&req.reference_url)?;

    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    require_open_project(&mut scope, project).await?;
    let (brief, brief_version) = match req.handoff_id {
        Some(h) => {
            let handoff = store::get_handoff(&mut scope.tx, org, h)
                .await?
                .filter(|x| x.project_id == project)
                .ok_or(ApiError::NotFound("Handoff"))?;
            (Some(handoff.brief_id), Some(handoff.brief_version))
        }
        None => (None, None),
    };
    let sha = digest(&body);
    if let Some(existing) = store::find_duplicate(&mut scope.tx, org, project, &sha).await? {
        scope.commit().await?;
        return Ok((
            StatusCode::CONFLICT,
            Json(DuplicateBody {
                error: DuplicateError {
                    code: "duplicate_response",
                    message: "This exact response is already in the project.",
                    existing_response_id: existing,
                },
            }),
        )
            .into_response());
    }
    let id = Uuid::now_v7();
    store::insert_response(
        &mut scope.tx,
        &store::NewResponse {
            id,
            org,
            project,
            handoff: req.handoff_id,
            brief,
            brief_version,
            provider: &provider,
            model: &model,
            source_mode: &source,
            provenance: "user_declared",
            body: &body,
            content_sha256: &sha,
            reference_url: &reference_url,
            by: scope.user.as_uuid(),
        },
    )
    .await?;
    if let Some(h) = req.handoff_id {
        store::mark_imported(&mut scope.tx, org, h).await?;
    }
    scope
        .audit(
            "response.imported",
            "response",
            id,
            json!({ "provider_declared": provider, "source_mode": source, "handoff_id": req.handoff_id,
                    "characters": body.chars().count(), "sha256": hex(&sha) }),
        )
        .await?;
    let row = store::get_response(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(ResponseView::from(row))).into_response())
}

#[derive(Deserialize)]
pub struct ResponseQuery {
    pub brief: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /api/v1/orgs/{org}/projects/{project}/responses
pub async fn list_responses(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(q): ApiQuery<ResponseQuery>,
) -> ApiResult<Json<Paged<ResponseView>>> {
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if projects::get(&mut scope.tx, org, project).await?.is_none() {
        return Err(ApiError::NotFound("Project"));
    }
    let rows =
        store::list_responses(&mut scope.tx, org, project, q.brief, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter().map(Into::into).collect(),
        limit,
        offset,
    )))
}

/// GET /api/v1/orgs/{org}/responses/{response}
pub async fn get_response(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, response)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<ResponseView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let row = store::get_response(&mut scope.tx, org, response)
        .await?
        .ok_or(ApiError::NotFound("Response"))?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewInput {
    pub review_status: String,
}

/// PATCH /api/v1/orgs/{org}/responses/{response} — review status only; the
/// original text can never be changed.
pub async fn review_response(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, response)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<ReviewInput>,
) -> ApiResult<Json<ResponseView>> {
    if !matches!(
        req.review_status.as_str(),
        "unreviewed" | "accepted" | "rejected"
    ) {
        return Err(ApiError::Validation {
            field: "review_status".into(),
            message: "must be unreviewed, accepted or rejected".into(),
        });
    }
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if !(scope.role.allows(Permission::DecisionsWrite)
        || scope.role.allows(Permission::DecisionsDecide))
    {
        return Err(ApiError::Forbidden);
    }
    if !store::set_review_status(&mut scope.tx, org, response, &req.review_status).await? {
        return Err(ApiError::NotFound("Response"));
    }
    scope
        .audit(
            "response.reviewed",
            "response",
            response,
            json!({ "review_status": req.review_status }),
        )
        .await?;
    let row = store::get_response(&mut scope.tx, org, response)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_urls_must_be_https_and_clean() {
        assert_eq!(clean_reference_url("").expect("empty ok"), "");
        assert!(clean_reference_url("https://chatgpt.com/share/abc").is_ok());
        for bad in [
            "http://x.example",
            "javascript:alert(1)",
            "https://a b",
            "ftp://x",
        ] {
            assert!(clean_reference_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn imports_normalize_line_endings() {
        assert_eq!(normalize_import("  a\r\nb\rc  "), "a\nb\nc");
    }
}
