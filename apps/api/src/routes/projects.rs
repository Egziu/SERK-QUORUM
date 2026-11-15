use super::{ApiJson, ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use quorum_domain::Permission;
use quorum_domain::ids::ProjectId;
use quorum_domain::project::{self, ProjectStatus};
use quorum_storage::projects;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ProjectView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub created_by: Uuid,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "ts::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub document_count: i64,
    pub decision_count: i64,
}

impl From<projects::ProjectRow> for ProjectView {
    fn from(r: projects::ProjectRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            status: r.status,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            document_count: r.document_count,
            decision_count: r.decision_count,
        }
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub include_archived: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /api/v1/orgs/{org}/projects
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Paged<ProjectView>>> {
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let rows = projects::list(&mut scope.tx, org, q.include_archived, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter().map(Into::into).collect(),
        limit,
        offset,
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// POST /api/v1/orgs/{org}/projects
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    ApiPath(org): ApiPath<Uuid>,
    ApiJson(req): ApiJson<CreateProject>,
) -> ApiResult<(StatusCode, Json<ProjectView>)> {
    let name = project::name(&req.name)?;
    let description = project::description(&req.description)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProjectsManage).await?;
    let id = ProjectId::new().as_uuid();
    projects::insert(
        &mut scope.tx,
        id,
        org,
        &name,
        &description,
        scope.user.as_uuid(),
    )
    .await
    .map_err(|e| match e {
        quorum_storage::StorageError::Conflict => {
            ApiError::conflict("A project with this name already exists.")
        }
        other => other.into(),
    })?;
    scope
        .audit("project.created", "project", id, json!({}))
        .await?;
    let row = projects::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// GET /api/v1/orgs/{org}/projects/{id}
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<ProjectView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let row = projects::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Project"))?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<ProjectStatus>,
}

/// PATCH /api/v1/orgs/{org}/projects/{id}
pub async fn update(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<UpdateProject>,
) -> ApiResult<Json<ProjectView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::ProjectsManage).await?;
    // Lock first so concurrent updates serialize.
    projects::lock_status(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Project"))?;
    let current = projects::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Project"))?;
    let name = match &req.name {
        Some(n) => project::name(n)?,
        None => current.name.clone(),
    };
    let description = match &req.description {
        Some(d) => project::description(d)?,
        None => current.description.clone(),
    };
    let status = req
        .status
        .map(ProjectStatus::as_str)
        .unwrap_or(current.status.as_str())
        .to_owned();
    projects::update(&mut scope.tx, org, id, &name, &description, &status)
        .await
        .map_err(|e| match e {
            quorum_storage::StorageError::Conflict => {
                ApiError::conflict("A project with this name already exists.")
            }
            other => other.into(),
        })?;
    let mut changed = Vec::new();
    if name != current.name {
        changed.push("name");
    }
    if description != current.description {
        changed.push("description");
    }
    if status != current.status {
        changed.push("status");
    }
    scope
        .audit(
            "project.updated",
            "project",
            id,
            json!({ "changed": changed, "status": status }),
        )
        .await?;
    let row = projects::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::Internal)?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

/// Locks a project and verifies it accepts new content. Used by document
/// and decision handlers.
pub async fn require_open_project(scope: &mut OrgScope, project: Uuid) -> ApiResult<()> {
    let status = projects::lock_status(&mut scope.tx, scope.org.as_uuid(), project)
        .await?
        .ok_or(ApiError::NotFound("Project"))?;
    if !ProjectStatus::parse(&status)?.accepts_changes() {
        return Err(ApiError::conflict(
            "The project is archived. Reactivate it to make changes.",
        ));
    }
    Ok(())
}
