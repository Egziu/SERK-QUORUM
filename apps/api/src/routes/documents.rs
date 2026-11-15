//! QUORUM Studio documents with append-only version history.

use super::projects::require_open_project;
use super::{ApiJson, ApiPath, ApiQuery, ts};
use crate::context::{OrgScope, Page, Paged, Session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use quorum_domain::Permission;
use quorum_domain::document::{self, DocumentCategory, DocumentStatus};
use quorum_domain::ids::DocumentId;
use quorum_storage::{documents, projects};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Serialize)]
pub struct DocumentSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub category: String,
    pub status: String,
    pub current_version: i32,
    pub approved_version: Option<i32>,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "ts::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<documents::DocumentRow> for DocumentSummary {
    fn from(r: documents::DocumentRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            title: r.title,
            category: r.category,
            status: r.status,
            current_version: r.current_version,
            approved_version: r.approved_version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct VersionView {
    pub version: i32,
    pub title: String,
    pub body: String,
    pub ai_assisted: bool,
    pub change_note: String,
    pub created_by: Uuid,
    pub created_by_name: String,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<documents::VersionRow> for VersionView {
    fn from(v: documents::VersionRow) -> Self {
        Self {
            version: v.version,
            title: v.title,
            body: v.body,
            ai_assisted: v.ai_assisted,
            change_note: v.change_note,
            created_by: v.created_by,
            created_by_name: v.created_by_name,
            created_at: v.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct DocumentDetail {
    #[serde(flatten)]
    pub document: DocumentSummary,
    pub current: VersionView,
}

/// GET /api/v1/orgs/{org}/projects/{project}/documents
pub async fn list(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(page): ApiQuery<Page>,
) -> ApiResult<Json<Paged<DocumentSummary>>> {
    let (limit, offset) = page.bounds()?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if projects::get(&mut scope.tx, org, project).await?.is_none() {
        return Err(ApiError::NotFound("Project"));
    }
    let rows = documents::list(&mut scope.tx, org, project, limit + 1, offset).await?;
    scope.commit().await?;
    Ok(Json(Paged::from_overfetch(
        rows.into_iter().map(Into::into).collect(),
        limit,
        offset,
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDocument {
    pub title: String,
    pub category: DocumentCategory,
    #[serde(default)]
    pub body: String,
    /// Set when the body contains text produced by an AI provider.
    #[serde(default)]
    pub ai_assisted: bool,
}

async fn detail(scope: &mut OrgScope, id: Uuid) -> ApiResult<DocumentDetail> {
    let org = scope.org.as_uuid();
    let doc = documents::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Document"))?;
    let current = documents::get_version(&mut scope.tx, org, id, doc.current_version)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok(DocumentDetail {
        document: doc.into(),
        current: current.into(),
    })
}

/// POST /api/v1/orgs/{org}/projects/{project}/documents
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, project)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<CreateDocument>,
) -> ApiResult<(StatusCode, Json<DocumentDetail>)> {
    let title = document::title(&req.title)?;
    let body = document::body(&req.body)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::DocumentsWrite).await?;
    require_open_project(&mut scope, project).await?;
    let id = DocumentId::new().as_uuid();
    documents::insert(
        &mut scope.tx,
        id,
        org,
        project,
        &title,
        req.category.as_str(),
        &body,
        req.ai_assisted,
        scope.user.as_uuid(),
    )
    .await?;
    projects::touch(&mut scope.tx, org, project).await?;
    scope
        .audit(
            "document.created",
            "document",
            id,
            json!({ "project_id": project, "category": req.category, "ai_assisted": req.ai_assisted }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// GET /api/v1/orgs/{org}/documents/{id}
pub async fn get(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<DocumentDetail>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewVersion {
    /// The version the editor started from. A mismatch means someone else
    /// saved in the meantime; the request is refused instead of overwriting.
    pub base_version: i32,
    pub title: String,
    pub category: Option<DocumentCategory>,
    pub body: String,
    #[serde(default)]
    pub ai_assisted: bool,
    #[serde(default)]
    pub change_note: String,
}

/// POST /api/v1/orgs/{org}/documents/{id}/versions — saves a new version.
pub async fn save_version(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<NewVersion>,
) -> ApiResult<(StatusCode, Json<DocumentDetail>)> {
    let title = document::title(&req.title)?;
    let body = document::body(&req.body)?;
    let note = document::change_note(&req.change_note)?;
    let mut scope = OrgScope::open(&state, &session, org, Permission::DocumentsWrite).await?;
    let doc = documents::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Document"))?;
    require_open_project(&mut scope, doc.project_id).await?;
    let doc = documents::get_for_update(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Document"))?;
    if doc.current_version != req.base_version {
        return Err(ApiError::conflict(format!(
            "The document changed since you opened it (now version {}). Reload and reapply your edits.",
            doc.current_version
        )));
    }
    let category = match req.category {
        Some(c) => c.as_str().to_owned(),
        None => doc.category.clone(),
    };
    let version = documents::append_version(
        &mut scope.tx,
        org,
        id,
        doc.current_version,
        &title,
        &category,
        &body,
        req.ai_assisted,
        &note,
        scope.user.as_uuid(),
    )
    .await?;
    projects::touch(&mut scope.tx, org, doc.project_id).await?;
    scope
        .audit(
            "document.version_created",
            "document",
            id,
            json!({ "version": version, "ai_assisted": req.ai_assisted }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approve {
    pub version: i32,
}

/// POST /api/v1/orgs/{org}/documents/{id}/approve — a human approves the
/// current version.
pub async fn approve(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<Approve>,
) -> ApiResult<Json<DocumentDetail>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::DocumentsApprove).await?;
    let doc = documents::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Document"))?;
    require_open_project(&mut scope, doc.project_id).await?;
    let doc = documents::get_for_update(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Document"))?;
    if doc.current_version != req.version {
        return Err(ApiError::conflict(
            "Only the current version can be approved. Reload and review it.",
        ));
    }
    if doc.status == DocumentStatus::Approved.as_str() {
        return Err(ApiError::conflict("This version is already approved."));
    }
    documents::approve(&mut scope.tx, org, id, req.version).await?;
    scope
        .audit(
            "document.approved",
            "document",
            id,
            json!({ "version": req.version }),
        )
        .await?;
    let view = detail(&mut scope, id).await?;
    scope.commit().await?;
    Ok(Json(view))
}

#[derive(Serialize)]
pub struct VersionSummary {
    pub version: i32,
    pub title: String,
    pub ai_assisted: bool,
    pub change_note: String,
    pub created_by_name: String,
    #[serde(with = "ts::rfc3339")]
    pub created_at: OffsetDateTime,
    pub body_chars: i32,
}

/// GET /api/v1/orgs/{org}/documents/{id}/versions
pub async fn versions(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<VersionSummary>>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    if documents::get(&mut scope.tx, org, id).await?.is_none() {
        return Err(ApiError::NotFound("Document"));
    }
    let rows = documents::list_versions(&mut scope.tx, org, id).await?;
    scope.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|v| VersionSummary {
                version: v.version,
                title: v.title,
                ai_assisted: v.ai_assisted,
                change_note: v.change_note,
                created_by_name: v.created_by_name,
                created_at: v.created_at,
                body_chars: v.body_chars,
            })
            .collect(),
    ))
}

/// GET /api/v1/orgs/{org}/documents/{id}/versions/{version}
pub async fn version(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id, version)): ApiPath<(Uuid, Uuid, i32)>,
) -> ApiResult<Json<VersionView>> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let row = documents::get_version(&mut scope.tx, org, id, version)
        .await?
        .ok_or(ApiError::NotFound("Document version"))?;
    scope.commit().await?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
pub struct ExportQuery {
    pub version: Option<i32>,
}

/// GET /api/v1/orgs/{org}/documents/{id}/export.md — Markdown download.
pub async fn export_markdown(
    State(state): State<AppState>,
    session: Session,
    ApiPath((org, id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(q): ApiQuery<ExportQuery>,
) -> ApiResult<impl IntoResponse> {
    let mut scope = OrgScope::open(&state, &session, org, Permission::WorkspaceRead).await?;
    let doc = documents::get(&mut scope.tx, org, id)
        .await?
        .ok_or(ApiError::NotFound("Document"))?;
    let version = q.version.unwrap_or(doc.current_version);
    let v = documents::get_version(&mut scope.tx, org, id, version)
        .await?
        .ok_or(ApiError::NotFound("Document version"))?;
    let status = if doc.approved_version == Some(version) {
        DocumentStatus::Approved
    } else {
        DocumentStatus::Draft
    };
    let category = DocumentCategory::parse(&doc.category)?;
    let markdown =
        document::to_markdown(&v.title, category, version, status, v.ai_assisted, &v.body);
    scope
        .audit(
            "document.exported",
            "document",
            id,
            json!({ "version": version, "format": "markdown" }),
        )
        .await?;
    scope.commit().await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    let filename = format!("document-{id}-v{version}.md");
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| ApiError::Internal)?,
    );
    Ok((headers, markdown))
}
