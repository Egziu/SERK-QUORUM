//! Router and middleware stack.

use crate::routes::{audit, auth, decisions, documents, health, orgs, projects, providers};
use crate::state::AppState;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::routing::{get, post};
use std::time::Duration;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// 1 MiB: fits the largest permitted document body (200 000 characters).
pub const BODY_LIMIT: usize = 1024 * 1024;

pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

/// Every route of the versioned API, as `(method, path)`. The OpenAPI
/// contract test checks this list against `contracts/openapi.json`.
pub const ROUTES: &[(&str, &str)] = &[
    ("POST", "/api/v1/auth/signup"),
    ("POST", "/api/v1/auth/login"),
    ("POST", "/api/v1/auth/logout"),
    ("POST", "/api/v1/auth/logout-all"),
    ("GET", "/api/v1/auth/session"),
    ("POST", "/api/v1/orgs"),
    ("GET", "/api/v1/orgs/{org}"),
    ("GET", "/api/v1/orgs/{org}/members"),
    ("PATCH", "/api/v1/orgs/{org}/members/{user}"),
    ("DELETE", "/api/v1/orgs/{org}/members/{user}"),
    ("GET", "/api/v1/orgs/{org}/invitations"),
    ("POST", "/api/v1/orgs/{org}/invitations"),
    ("DELETE", "/api/v1/orgs/{org}/invitations/{invitation}"),
    ("POST", "/api/v1/invitations/accept"),
    ("GET", "/api/v1/orgs/{org}/projects"),
    ("POST", "/api/v1/orgs/{org}/projects"),
    ("GET", "/api/v1/orgs/{org}/projects/{project}"),
    ("PATCH", "/api/v1/orgs/{org}/projects/{project}"),
    ("GET", "/api/v1/orgs/{org}/projects/{project}/documents"),
    ("POST", "/api/v1/orgs/{org}/projects/{project}/documents"),
    ("GET", "/api/v1/orgs/{org}/documents/{document}"),
    ("GET", "/api/v1/orgs/{org}/documents/{document}/versions"),
    ("POST", "/api/v1/orgs/{org}/documents/{document}/versions"),
    (
        "GET",
        "/api/v1/orgs/{org}/documents/{document}/versions/{version}",
    ),
    ("POST", "/api/v1/orgs/{org}/documents/{document}/approve"),
    ("GET", "/api/v1/orgs/{org}/documents/{document}/export.md"),
    ("GET", "/api/v1/orgs/{org}/projects/{project}/decisions"),
    ("POST", "/api/v1/orgs/{org}/projects/{project}/decisions"),
    ("GET", "/api/v1/orgs/{org}/decisions/{decision}"),
    ("PATCH", "/api/v1/orgs/{org}/decisions/{decision}"),
    ("POST", "/api/v1/orgs/{org}/decisions/{decision}/transition"),
    ("GET", "/api/v1/orgs/{org}/decisions/{decision}/versions"),
    ("GET", "/api/v1/orgs/{org}/providers"),
    ("GET", "/api/v1/orgs/{org}/provider-runs"),
    ("POST", "/api/v1/orgs/{org}/provider-runs"),
    ("GET", "/api/v1/orgs/{org}/audit-events"),
];

fn api() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(auth::signup))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/logout-all", post(auth::logout_all))
        .route("/auth/session", get(auth::current))
        .route("/orgs", post(orgs::create))
        .route("/orgs/{org}", get(orgs::get))
        .route("/orgs/{org}/members", get(orgs::members))
        .route(
            "/orgs/{org}/members/{user}",
            axum::routing::patch(orgs::change_role).delete(orgs::remove_member),
        )
        .route(
            "/orgs/{org}/invitations",
            get(orgs::invitations).post(orgs::invite),
        )
        .route(
            "/orgs/{org}/invitations/{invitation}",
            axum::routing::delete(orgs::revoke_invitation),
        )
        .route("/invitations/accept", post(orgs::accept))
        .route(
            "/orgs/{org}/projects",
            get(projects::list).post(projects::create),
        )
        .route(
            "/orgs/{org}/projects/{project}",
            get(projects::get).patch(projects::update),
        )
        .route(
            "/orgs/{org}/projects/{project}/documents",
            get(documents::list).post(documents::create),
        )
        .route("/orgs/{org}/documents/{document}", get(documents::get))
        .route(
            "/orgs/{org}/documents/{document}/versions",
            get(documents::versions).post(documents::save_version),
        )
        .route(
            "/orgs/{org}/documents/{document}/versions/{version}",
            get(documents::version),
        )
        .route(
            "/orgs/{org}/documents/{document}/approve",
            post(documents::approve),
        )
        .route(
            "/orgs/{org}/documents/{document}/export.md",
            get(documents::export_markdown),
        )
        .route(
            "/orgs/{org}/projects/{project}/decisions",
            get(decisions::list).post(decisions::create),
        )
        .route(
            "/orgs/{org}/decisions/{decision}",
            get(decisions::get).patch(decisions::update),
        )
        .route(
            "/orgs/{org}/decisions/{decision}/transition",
            post(decisions::transition),
        )
        .route(
            "/orgs/{org}/decisions/{decision}/versions",
            get(decisions::versions),
        )
        .route("/orgs/{org}/providers", get(providers::catalog))
        .route(
            "/orgs/{org}/provider-runs",
            get(providers::usage_list).post(providers::run),
        )
        .route("/orgs/{org}/audit-events", get(audit::list))
        .fallback(|| async { crate::error::ApiError::NotFound("Endpoint") })
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}

pub fn router(state: AppState) -> Router {
    let request_timeout = state.config().providers.timeout + Duration::from_secs(15);
    let mut app = Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .nest("/api/v1", api());

    if let Some(dir) = state.config().web_dir.clone()
        && dir.join("index.html").is_file()
    {
        // Client-side routes fall back to index.html.
        let index = ServeFile::new(dir.join("index.html"));
        app = app.fallback_service(ServeDir::new(dir).fallback(index));
    }

    let hsts = state.config().cookie_secure;
    let mut app = app
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-opener-policy"),
            HeaderValue::from_static("same-origin"),
        ));
    if hsts {
        app = app.layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ));
    }
    app.layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(crate::request_id::middleware))
        .with_state(state)
}
