//! Integration test harness.
//!
//! Each test gets a fresh PostgreSQL database, migrated by the owner role.
//! The API under test connects as the *application* role (not the owner),
//! so row-level security and grants apply exactly as in production.
//!
//! Required environment (see docs/TESTING_GUIDE.md):
//!   QUORUM_TEST_DATABASE_URL  owner-role URL, e.g.
//!       postgres://quorum_owner:quorum_owner_dev@127.0.0.1:5432/postgres
//!   QUORUM_TEST_APP_USER      default quorum_api
//!   QUORUM_TEST_APP_PASSWORD  default quorum_api_dev
//!
//! The tests fail (not skip) when the database is unavailable: a skipped
//! isolation test must never be mistaken for a passing one.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use quorum_api::{AppState, Config};
use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::str::FromStr;
use tower::ServiceExt;

pub struct TestDb {
    pub name: String,
    owner: PgConnectOptions,
    /// Pool connected as the owner role (for assertions that need to see
    /// everything, and to prove what the app role cannot do).
    pub owner_pool: PgPool,
    /// Pool connected as the application role.
    pub app_pool: PgPool,
}

fn owner_options() -> PgConnectOptions {
    let url = std::env::var("QUORUM_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "QUORUM_TEST_DATABASE_URL is not set. Integration tests need PostgreSQL; \
             see docs/TESTING_GUIDE.md (they fail rather than skip by design)."
        )
    });
    PgConnectOptions::from_str(&url).expect("valid QUORUM_TEST_DATABASE_URL")
}

impl TestDb {
    pub async fn create() -> Self {
        Self::create_with_pool_size(5).await
    }

    pub async fn create_with_pool_size(app_connections: u32) -> Self {
        let owner = owner_options();
        let name = format!("quorum_test_{}", uuid::Uuid::new_v4().simple());
        let mut admin = owner
            .connect()
            .await
            .expect("connect to PostgreSQL as owner role");
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut admin)
            .await
            .expect("create test database");
        let owner_db = owner.clone().database(&name);
        let owner_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(owner_db.clone())
            .await
            .expect("owner pool");
        quorum_storage::MIGRATOR
            .run(&owner_pool)
            .await
            .expect("migrations apply");

        let app_user =
            std::env::var("QUORUM_TEST_APP_USER").unwrap_or_else(|_| "quorum_api".into());
        let app_password =
            std::env::var("QUORUM_TEST_APP_PASSWORD").unwrap_or_else(|_| "quorum_api_dev".into());
        let app_pool = PgPoolOptions::new()
            .max_connections(app_connections)
            .connect_with(owner_db.username(&app_user).password(&app_password))
            .await
            .expect("app role pool (run scripts/bootstrap-db.sql)");
        Self {
            name,
            owner,
            owner_pool,
            app_pool,
        }
    }

    pub async fn drop(self) {
        self.app_pool.close().await;
        self.owner_pool.close().await;
        if let Ok(mut admin) = self.owner.connect().await {
            let _ = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )))
            .execute(&mut admin)
            .await;
        }
    }
}

pub fn test_config(overrides: &[(&str, &str)]) -> Config {
    let mut pairs: Vec<(String, String)> = vec![
        ("DATABASE_URL".into(), "postgres://unused".into()),
        ("QUORUM_ALLOW_SIGNUP".into(), "1".into()),
        ("QUORUM_COOKIE_SECURE".into(), "0".into()),
        ("QUORUM_PROVIDER_COOLDOWN_SECONDS".into(), "0".into()),
    ];
    for (k, v) in overrides {
        pairs.retain(|(key, _)| key != k);
        pairs.push(((*k).into(), (*v).into()));
    }
    Config::from_lookup(move |k| {
        pairs
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    })
    .expect("config")
}

pub fn app(db: &TestDb, config: Config) -> Router {
    let state = AppState::with_http_client(config, db.app_pool.clone(), reqwest::Client::new());
    quorum_api::router(state)
}

pub struct Response {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Value,
    pub text: String,
}

/// A browser-like client: keeps the session cookie and sends the CSRF token.
#[derive(Clone)]
pub struct Client {
    app: Router,
    pub cookie: Option<String>,
    pub csrf: Option<String>,
}

impl Client {
    pub fn new(app: &Router) -> Self {
        Self {
            app: app.clone(),
            cookie: None,
            csrf: None,
        }
    }

    pub async fn request(&mut self, method: Method, path: &str, body: Option<Value>) -> Response {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(c) = &self.cookie {
            req = req.header(header::COOKIE, c);
        }
        if let Some(t) = &self.csrf {
            req = req.header("x-csrf-token", t);
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .expect("request");
        let res = self.app.clone().oneshot(req).await.expect("response");
        let status = res.status();
        let headers = res.headers().clone();
        if let Some(set) = headers
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
        {
            let pair = set.split(';').next().unwrap_or_default().to_owned();
            if pair.ends_with('=') {
                self.cookie = None;
            } else {
                self.cookie = Some(pair);
            }
        }
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if let Some(t) = body.get("csrf_token").and_then(Value::as_str) {
            self.csrf = Some(t.to_owned());
        }
        Response {
            status,
            headers,
            body,
            text,
        }
    }

    pub async fn get(&mut self, path: &str) -> Response {
        self.request(Method::GET, path, None).await
    }
    pub async fn post(&mut self, path: &str, body: Value) -> Response {
        self.request(Method::POST, path, Some(body)).await
    }
    pub async fn patch(&mut self, path: &str, body: Value) -> Response {
        self.request(Method::PATCH, path, Some(body)).await
    }
    pub async fn delete(&mut self, path: &str) -> Response {
        self.request(Method::DELETE, path, None).await
    }
}

pub const PASSWORD: &str = "synthetic-password-0001";

/// Signs up a synthetic user with their own organization. Returns the
/// client and the organization id.
pub async fn signup(app: &Router, name: &str, org: &str) -> (Client, String) {
    let mut c = Client::new(app);
    let r = c
        .post(
            "/api/v1/auth/signup",
            json!({
                "email": format!("{name}@example.test"),
                "display_name": name,
                "password": PASSWORD,
                "organization_name": org,
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "signup {name}: {}", r.text);
    let org_id = r.body["memberships"][0]["organization_id"]
        .as_str()
        .expect("org id")
        .to_owned();
    (c, org_id)
}

/// Invites `name` into `org` with `role` and has them accept. The invitee
/// signs up first (getting their own organization, as signup always does).
pub async fn add_member(
    app: &Router,
    owner: &mut Client,
    org: &str,
    name: &str,
    role: &str,
) -> (Client, String) {
    let (mut member, _) = signup(app, name, &format!("{name} personal")).await;
    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/invitations"),
            json!({ "email": format!("{name}@example.test"), "role": role }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "invite: {}", r.text);
    let token = r.body["token"].as_str().expect("token").to_owned();
    let r = member
        .post("/api/v1/invitations/accept", json!({ "token": token }))
        .await;
    assert_eq!(r.status, StatusCode::OK, "accept: {}", r.text);
    let me = member.get("/api/v1/auth/session").await;
    let user_id = me.body["user"]["id"].as_str().expect("user id").to_owned();
    (member, user_id)
}

pub async fn create_project(c: &mut Client, org: &str, name: &str) -> String {
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects"),
            json!({ "name": name, "description": "Synthetic" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "project: {}", r.text);
    r.body["id"].as_str().expect("id").to_owned()
}

pub async fn create_document(c: &mut Client, org: &str, project: &str, title: &str) -> String {
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/documents"),
            json!({ "title": title, "category": "project_plan", "body": "Synthetic body" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "document: {}", r.text);
    r.body["id"].as_str().expect("id").to_owned()
}

pub async fn create_decision(c: &mut Client, org: &str, project: &str, title: &str) -> Value {
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/decisions"),
            json!({
                "title": title,
                "problem_statement": "Synthetic problem",
                "alternatives": "Option A; Option B",
                "outcome": "Option A",
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "decision: {}", r.text);
    r.body
}
