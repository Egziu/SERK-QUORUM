//! QUORUM Command: one user-initiated, text-only request to one provider.

use super::{ApiJson, ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use quorum_domain::Permission;
use quorum_providers::{
    Completion, CompletionRequest, Mode, PROMPT_MAX_CHARS, ProviderError, ProviderId,
    ProviderStatus, Task,
};
use quorum_storage::{projects, usage};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ProviderView {
    pub id: ProviderId,
    pub vendor: &'static str,
    pub product: &'static str,
    pub perspective: &'static str,
    pub status: ProviderStatus,
    pub model: Option<String>,
    pub capabilities: [&'static str; 1],
    pub data_handling: &'static str,
    pub fixture_available: bool,
}

#[derive(Serialize)]
pub struct TaskView {
    pub id: Task,
    pub label: &'static str,
    pub instruction: &'static str,
}

#[derive(Serialize)]
pub struct Catalog {
    pub providers: Vec<ProviderView>,
    pub tasks: Vec<TaskView>,
    pub prompt_max_chars: usize,
    pub live_enabled: bool,
}

/// GET /api/v1/orgs/{org}/providers
pub async fn catalog(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
) -> ApiResult<Json<Catalog>> {
    let scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    scope.commit().await?;
    let settings = &state.config().providers;
    Ok(Json(Catalog {
        providers: ProviderId::ALL
            .into_iter()
            .map(|p| ProviderView {
                id: p,
                vendor: p.vendor(),
                product: p.product(),
                perspective: p.perspective(),
                status: settings.status(p),
                model: settings.model(p).map(str::to_owned),
                capabilities: ["text"],
                data_handling: p.data_handling(),
                fixture_available: true,
            })
            .collect(),
        tasks: Task::ALL
            .into_iter()
            .map(|t| TaskView {
                id: t,
                label: t.label(),
                instruction: t.instruction(),
            })
            .collect(),
        prompt_max_chars: PROMPT_MAX_CHARS,
        live_enabled: settings.live_enabled,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub provider: ProviderId,
    pub task: Task,
    pub prompt: String,
    pub mode: Mode,
    pub project_id: Option<Uuid>,
    /// Must be true for live mode: the user confirms the brief will be sent
    /// to the named external provider.
    #[serde(default)]
    pub acknowledge_external_transfer: bool,
}

#[derive(Serialize)]
pub struct RunResponse {
    pub request_id: Uuid,
    #[serde(flatten)]
    pub completion: Completion,
    pub notice: &'static str,
}

/// POST /api/v1/orgs/{org}/provider-runs
pub async fn run(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiJson(req): ApiJson<RunRequest>,
) -> ApiResult<Json<RunResponse>> {
    let prompt = quorum_domain::validate::multi_line("prompt", &req.prompt, 1, PROMPT_MAX_CHARS)?;
    if prompt.trim().is_empty() {
        return Err(ApiError::Validation {
            field: "prompt".into(),
            message: "must not be empty".into(),
        });
    }
    if req.mode == Mode::Live && !req.acknowledge_external_transfer {
        return Err(ApiError::Validation {
            field: "acknowledge_external_transfer".into(),
            message: "confirm that this brief may be sent to the external provider".into(),
        });
    }

    // Authorize and validate in a short transaction that is closed before
    // the (possibly slow) provider call, so no connection is held open.
    {
        let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
        if let Some(project) = req.project_id
            && projects::get(&mut scope.tx, org, project).await?.is_none()
        {
            return Err(ApiError::NotFound("Project"));
        }
        scope.commit().await?;
    }

    let completion_req = CompletionRequest {
        provider: req.provider,
        task: req.task,
        prompt: &prompt,
    };
    let result: Result<Completion, ProviderError> = match req.mode {
        Mode::Fixture => Ok(quorum_providers::fixture(&completion_req)),
        Mode::Live => {
            let throttle_key = format!("provider:{}", session.user_id);
            if state.inner.provider_throttle.is_limited(&throttle_key) {
                return Err(ApiError::RateLimited(
                    "Please wait a few seconds between live requests.",
                ));
            }
            let Ok(_permit) = state.inner.provider_slots.try_acquire() else {
                return Err(ApiError::RateLimited(
                    "The server is handling other live requests. Try again shortly.",
                ));
            };
            state.inner.provider_throttle.hit(&throttle_key);
            quorum_providers::complete_live(
                &state.inner.http,
                &state.config().providers,
                &completion_req,
            )
            .await
        }
    };

    let request_id = Uuid::now_v7();
    let (outcome, model, input_tokens, output_tokens, latency) = match &result {
        Ok(c) => (
            "succeeded",
            c.model.clone(),
            c.usage.as_ref().and_then(|u| u.input_tokens),
            c.usage.as_ref().and_then(|u| u.output_tokens),
            c.latency_ms,
        ),
        Err(_) => (
            "failed",
            state
                .config()
                .providers
                .model(req.provider)
                .unwrap_or("unconfigured")
                .to_owned(),
            None,
            None,
            0,
        ),
    };
    let to_i32 = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProvidersUse).await?;
    usage::record(
        &mut scope.tx,
        &usage::NewProviderRequest {
            id: request_id,
            organization_id: org,
            user_id: scope.user.as_uuid(),
            project_id: req.project_id,
            provider: req.provider.as_str(),
            model: &model,
            task: req.task.as_str(),
            mode: req.mode.as_str(),
            outcome,
            prompt_chars: i32::try_from(prompt.chars().count()).unwrap_or(i32::MAX),
            input_tokens: input_tokens.map(to_i32),
            output_tokens: output_tokens.map(to_i32),
            latency_ms: i32::try_from(latency).unwrap_or(i32::MAX),
        },
    )
    .await?;
    // Metadata only: the brief and the response text are not audited.
    scope
        .audit(
            "provider.requested",
            "provider_request",
            request_id,
            json!({ "provider": req.provider, "mode": req.mode, "task": req.task, "outcome": outcome }),
        )
        .await?;
    scope.commit().await?;

    match result {
        Ok(completion) => Ok(Json(RunResponse {
            request_id,
            notice: match completion.mode {
                Mode::Fixture => "Illustrative fixture. No AI provider was contacted.",
                Mode::Live => {
                    "AI-generated text. Verify it before relying on it; model output is not evidence."
                }
            },
            completion,
        })),
        Err(e) => Err(ApiError::Provider(format!(
            "{} — {}. No fallback response was substituted.",
            req.provider.vendor(),
            e
        ))),
    }
}

#[derive(Serialize)]
pub struct UsageView {
    pub id: Uuid,
    pub user_name: String,
    pub project_id: Option<Uuid>,
    pub provider: String,
    pub model: String,
    pub task: String,
    pub mode: String,
    pub outcome: String,
    pub prompt_chars: i32,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: i32,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// GET /api/v1/orgs/{org}/provider-runs — usage records (metadata only).
pub async fn usage_list(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiQuery(page): ApiQuery<Page>,
) -> ApiResult<Json<Paged<UsageView>>> {
    let (limit, offset) = page.bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::UsageRead).await?;
    let rows = usage::list(&mut scope.tx, org, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter()
            .map(|r| UsageView {
                id: r.id,
                user_name: r.user_name,
                project_id: r.project_id,
                provider: r.provider,
                model: r.model,
                task: r.task,
                mode: r.mode,
                outcome: r.outcome,
                prompt_chars: r.prompt_chars,
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                latency_ms: r.latency_ms,
                created_at: r.created_at,
            })
            .collect(),
        limit,
        offset,
    )))
}
