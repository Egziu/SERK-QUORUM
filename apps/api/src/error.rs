//! The API error type and its JSON envelope:
//!
//! ```json
//! {"error": {"code": "not_found", "message": "Project not found", "correlation_id": "…"}}
//! ```
//!
//! Internal details (SQL errors, stack traces, provider bodies) are logged
//! with the correlation identifier and never returned to the client.

use crate::request_id;
use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use quorum_domain::DomainError;
use quorum_storage::StorageError;
use serde::Serialize;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Validation {
        field: String,
        message: String,
    },
    Unauthenticated,
    Csrf,
    Forbidden,
    /// Refused by an organization policy (for example, external sharing limits).
    PolicyBlocked(String),
    NotFound(&'static str),
    Conflict(String),
    RateLimited(&'static str),
    PayloadTooLarge,
    Unsupported(&'static str),
    Provider(String),
    Internal,
}

#[derive(Serialize)]
struct Envelope<'a> {
    error: Body<'a>,
}

#[derive(Serialize)]
struct Body<'a> {
    code: &'a str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    correlation_id: String,
}

impl ApiError {
    fn parts(self) -> (StatusCode, &'static str, String, Option<String>) {
        use ApiError::*;
        match self {
            BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m, None),
            Validation { field, message } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
                format!("{field}: {message}"),
                Some(field),
            ),
            Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                "Sign in to continue.".into(),
                None,
            ),
            Csrf => (
                StatusCode::FORBIDDEN,
                "csrf_failed",
                "Request verification failed. Reload the page and try again.".into(),
                None,
            ),
            Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "Your role does not permit this action.".into(),
                None,
            ),
            PolicyBlocked(m) => (StatusCode::FORBIDDEN, "policy_blocked", m, None),
            NotFound(what) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("{what} not found."),
                None,
            ),
            Conflict(m) => (StatusCode::CONFLICT, "conflict", m, None),
            RateLimited(m) => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                m.into(),
                None,
            ),
            PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request body is too large.".into(),
                None,
            ),
            Unsupported(m) => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                m.into(),
                None,
            ),
            Provider(m) => (StatusCode::BAD_GATEWAY, "provider_failed", m, None),
            Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Something went wrong. Quote the correlation ID when reporting this.".into(),
                None,
            ),
        }
    }

    pub fn conflict(m: impl Into<String>) -> Self {
        Self::Conflict(m.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, field) = self.parts();
        let body = Envelope {
            error: Body {
                code,
                message,
                field,
                correlation_id: request_id::current(),
            },
        };
        (status, Json(body)).into_response()
    }
}

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::Validation { field, message } => ApiError::Validation {
                field: field.to_owned(),
                message,
            },
            DomainError::InvalidTransition { .. } | DomainError::Immutable(_) => {
                ApiError::Conflict(e.to_string())
            }
        }
    }
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::NotFound => ApiError::NotFound("Record"),
            StorageError::Conflict => {
                ApiError::conflict("A record with the same identifying values already exists.")
            }
            StorageError::Integrity => {
                ApiError::conflict("The change violates a data integrity rule.")
            }
            StorageError::Database(inner) => {
                tracing::error!(correlation_id = %request_id::current(), error = %inner, "database error");
                ApiError::Internal
            }
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        StorageError::from(e).into()
    }
}

impl From<JsonRejection> for ApiError {
    fn from(r: JsonRejection) -> Self {
        match r {
            JsonRejection::MissingJsonContentType(_) => {
                ApiError::Unsupported("Expected Content-Type: application/json.")
            }
            JsonRejection::BytesRejection(_) => ApiError::PayloadTooLarge,
            other => ApiError::BadRequest(format!("Invalid JSON body: {}", other.body_text())),
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        ApiError::NotFound("Resource")
    }
}

impl From<QueryRejection> for ApiError {
    fn from(r: QueryRejection) -> Self {
        ApiError::BadRequest(format!("Invalid query parameters: {}", r.body_text()))
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
