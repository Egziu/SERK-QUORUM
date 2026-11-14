pub mod audit;
pub mod auth;
pub mod decisions;
pub mod documents;
pub mod health;
pub mod orgs;
pub mod projects;
pub mod providers;

use crate::error::ApiError;
use axum::extract::{FromRequest, FromRequestParts};

/// JSON body extractor whose rejections use the API error envelope.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct ApiJson<T>(pub T);

/// Path extractor whose rejections use the API error envelope.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

/// Query extractor whose rejections use the API error envelope.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

pub(crate) mod ts {
    pub use time::serde::rfc3339;
}
