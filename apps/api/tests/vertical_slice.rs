//! The 0.2 vertical slice end to end:
//! authentication → organization → project → document → decision record.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn health_endpoints_and_security_headers() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let mut c = Client::new(&app);

    let r = c.get("/health/live").await;
    assert_eq!(r.status, StatusCode::OK);
    let r = c.get("/health/ready").await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(r.headers["x-content-type-options"], "nosniff");
    assert_eq!(r.headers["referrer-policy"], "no-referrer");
    assert!(r.headers.contains_key("x-request-id"));

    // Unknown API path: JSON error envelope with a correlation id.
    let r = c.get("/api/v1/nope").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.body["error"]["code"], "not_found");
    assert_eq!(
        r.body["error"]["correlation_id"].as_str().unwrap(),
        r.headers["x-request-id"].to_str().unwrap()
    );
    assert_eq!(r.headers["cache-control"], "no-store");
    db.drop().await;
}

#[tokio::test]
async fn signup_login_logout_and_session_revocation() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut ada, _org) = signup(&app, "ada", "Analytical Engines").await;

    let r = ada.get("/api/v1/auth/session").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["user"]["email"], "ada@example.test");
    assert_eq!(r.body["memberships"][0]["role"], "owner");
    // The session cookie is HttpOnly and SameSite=Strict.
    let mut fresh = Client::new(&app);
    let r = fresh
        .post(
            "/api/v1/auth/login",
            json!({ "email": "ADA@example.test", "password": PASSWORD }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let cookie = r.headers["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    assert!(
        !r.text.contains("password"),
        "no password material in responses"
    );

    // Duplicate signup is refused.
    let mut dup = Client::new(&app);
    let r = dup
        .post(
            "/api/v1/auth/signup",
            json!({ "email": "ada@example.test", "display_name": "x", "password": PASSWORD, "organization_name": "y" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Wrong password and unknown account give the same answer.
    let mut anon = Client::new(&app);
    let wrong = anon
        .post(
            "/api/v1/auth/login",
            json!({ "email": "ada@example.test", "password": "wrong-password-123" }),
        )
        .await;
    let unknown = anon
        .post(
            "/api/v1/auth/login",
            json!({ "email": "nobody@example.test", "password": "wrong-password-123" }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong.body["error"]["message"],
        unknown.body["error"]["message"]
    );

    // Logout revokes the session server-side: replaying the old cookie fails.
    let stolen = ada.cookie.clone();
    let r = ada.post("/api/v1/auth/logout", json!({})).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let mut replay = Client::new(&app);
    replay.cookie = stolen;
    assert_eq!(
        replay.get("/api/v1/auth/session").await.status,
        StatusCode::UNAUTHORIZED
    );

    // logout-all revokes every session of the user.
    let mut s1 = Client::new(&app);
    let mut s2 = Client::new(&app);
    for s in [&mut s1, &mut s2] {
        let r = s
            .post(
                "/api/v1/auth/login",
                json!({ "email": "ada@example.test", "password": PASSWORD }),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
    }
    assert_eq!(
        s1.post("/api/v1/auth/logout-all", json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        s2.get("/api/v1/auth/session").await.status,
        StatusCode::UNAUTHORIZED
    );
    db.drop().await;
}

#[tokio::test]
async fn login_is_throttled_after_repeated_failures() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    signup(&app, "grace", "Compilers Ltd").await;
    let mut c = Client::new(&app);
    for _ in 0..5 {
        let r = c
            .post(
                "/api/v1/auth/login",
                json!({ "email": "grace@example.test", "password": "not-the-password" }),
            )
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    // Even the correct password is refused while throttled.
    let r = c
        .post(
            "/api/v1/auth/login",
            json!({ "email": "grace@example.test", "password": PASSWORD }),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    db.drop().await;
}

#[tokio::test]
async fn signup_is_disabled_by_default_and_input_is_validated() {
    let db = TestDb::create().await;
    let closed = app(&db, test_config(&[("QUORUM_ALLOW_SIGNUP", "0")]));
    let mut c = Client::new(&closed);
    let r = c
        .post(
            "/api/v1/auth/signup",
            json!({ "email": "a@example.test", "display_name": "A", "password": PASSWORD, "organization_name": "O" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    let open = app(&db, test_config(&[]));
    let mut c = Client::new(&open);
    let r = c
        .post(
            "/api/v1/auth/signup",
            json!({ "email": "a@example.test", "display_name": "A", "password": "short", "organization_name": "O" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["error"]["field"], "password");
    let r = c
        .post(
            "/api/v1/auth/signup",
            json!({ "email": "a@example.test", "display_name": "A", "password": PASSWORD, "organization_name": "O", "admin": true }),
        )
        .await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "unknown fields are rejected"
    );
    db.drop().await;
}

#[tokio::test]
async fn state_changes_require_csrf_token_and_json() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut ada, org) = signup(&app, "ada", "Engines").await;

    let token = ada.csrf.take();
    let r = ada
        .post(
            &format!("/api/v1/orgs/{org}/projects"),
            json!({ "name": "X" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["error"]["code"], "csrf_failed");
    ada.csrf = Some("forged".into());
    let r = ada
        .post(
            &format!("/api/v1/orgs/{org}/projects"),
            json!({ "name": "X" }),
        )
        .await;
    assert_eq!(r.body["error"]["code"], "csrf_failed");
    ada.csrf = token;
    // Reads do not need the token.
    let saved = ada.csrf.take();
    assert_eq!(
        ada.get(&format!("/api/v1/orgs/{org}/projects"))
            .await
            .status,
        StatusCode::OK
    );
    ada.csrf = saved;

    // Non-JSON bodies are refused (blocks HTML form posts).
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/v1/orgs/{org}/projects"))
        .header("cookie", ada.cookie.clone().unwrap())
        .header("x-csrf-token", ada.csrf.clone().unwrap())
        .header("content-type", "application/x-www-form-urlencoded")
        .body(axum::body::Body::from("name=X"))
        .unwrap();
    let res = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    db.drop().await;
}

#[tokio::test]
async fn full_vertical_slice() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Atlas Engineering").await;
    let (mut reviewer, reviewer_id) =
        add_member(&app, &mut owner, &org, "reviewer", "reviewer").await;

    // Organization
    let r = owner.get(&format!("/api/v1/orgs/{org}")).await;
    assert_eq!(r.body["name"], "Atlas Engineering");
    assert_eq!(r.body["role"], "owner");

    // Project create / edit / list
    let project = create_project(&mut owner, &org, "Bridge retrofit").await;
    let r = owner
        .patch(
            &format!("/api/v1/orgs/{org}/projects/{project}"),
            json!({ "description": "Seismic retrofit of a synthetic bridge", "status": "on_hold" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["status"], "on_hold");
    let dup = owner
        .post(
            &format!("/api/v1/orgs/{org}/projects"),
            json!({ "name": "BRIDGE RETROFIT" }),
        )
        .await;
    assert_eq!(
        dup.status,
        StatusCode::CONFLICT,
        "names are unique per organization, case-insensitively"
    );
    let list = owner.get(&format!("/api/v1/orgs/{org}/projects")).await;
    assert_eq!(list.body["items"].as_array().unwrap().len(), 1);

    // Document with version history and optimistic concurrency
    let doc = create_document(&mut owner, &org, &project, "Retrofit plan").await;
    let base = format!("/api/v1/orgs/{org}/documents/{doc}");
    let r = owner
        .post(
            &format!("{base}/versions"),
            json!({ "base_version": 1, "title": "Retrofit plan", "body": "Revised body", "change_note": "Scope update" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["current_version"], 2);
    let stale = owner
        .post(
            &format!("{base}/versions"),
            json!({ "base_version": 1, "title": "Retrofit plan", "body": "Lost update" }),
        )
        .await;
    assert_eq!(
        stale.status,
        StatusCode::CONFLICT,
        "stale edits never overwrite"
    );
    let versions = owner.get(&format!("{base}/versions")).await;
    assert_eq!(versions.body.as_array().unwrap().len(), 2);
    let v1 = owner.get(&format!("{base}/versions/1")).await;
    assert_eq!(
        v1.body["body"], "Synthetic body",
        "old versions are preserved"
    );

    // Approval by a reviewer
    let r = reviewer
        .post(&format!("{base}/approve"), json!({ "version": 2 }))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["status"], "approved");
    assert_eq!(r.body["approved_version"], 2);

    // Markdown export
    let md = owner.get(&format!("{base}/export.md")).await;
    assert_eq!(md.status, StatusCode::OK);
    assert!(
        md.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/markdown")
    );
    assert!(md.text.starts_with("# Retrofit plan\n"));
    assert!(md.text.contains("Status: approved"));

    // Decision: draft → under_review → approved, with history
    let d = create_decision(&mut owner, &org, &project, "Use base isolation").await;
    let id = d["id"].as_str().unwrap();
    assert_eq!(d["reference"], "DEC-0001");
    assert_eq!(d["status"], "draft");
    let dpath = format!("/api/v1/orgs/{org}/decisions/{id}");
    let r = owner
        .patch(
            &dpath,
            json!({ "base_version": 1, "title": "Use base isolation", "problem_statement": "Seismic risk",
                    "alternatives": "Isolation; Dampers", "outcome": "Base isolation",
                    "decision_maker_id": reviewer_id, "review_date": "2027-03-01" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["version"], 2);
    assert_eq!(r.body["review_date"], "2027-03-01");
    assert_eq!(r.body["decision_maker_name"], "reviewer");

    let r = owner
        .post(
            &format!("{dpath}/transition"),
            json!({ "to": "under_review", "base_version": 2 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    // Content is frozen while under review.
    let r = owner
        .patch(
            &dpath,
            json!({ "base_version": 3, "title": "Changed", "problem_statement": "x" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    // Approval needs explicit confirmation.
    let r = reviewer
        .post(
            &format!("{dpath}/transition"),
            json!({ "to": "approved", "base_version": 3 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = reviewer
        .post(
            &format!("{dpath}/transition"),
            json!({ "to": "approved", "base_version": 3, "confirm": true, "note": "Reviewed with team" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["status"], "approved");
    // Approved decisions cannot be edited or re-approved.
    let r = owner
        .patch(
            &dpath,
            json!({ "base_version": 4, "title": "Rewrite history", "problem_statement": "x" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = reviewer
        .post(
            &format!("{dpath}/transition"),
            json!({ "to": "draft", "base_version": 4 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Supersede with a newer approved decision.
    let d2 = create_decision(&mut owner, &org, &project, "Use viscous dampers").await;
    let id2 = d2["id"].as_str().unwrap();
    assert_eq!(d2["reference"], "DEC-0002");
    let d2path = format!("/api/v1/orgs/{org}/decisions/{id2}");
    owner
        .post(
            &format!("{d2path}/transition"),
            json!({ "to": "under_review", "base_version": 1 }),
        )
        .await;
    let r = reviewer
        .post(
            &format!("{d2path}/transition"),
            json!({ "to": "approved", "base_version": 2, "confirm": true }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let r = reviewer
        .post(
            &format!("{dpath}/transition"),
            json!({ "to": "superseded", "base_version": 4, "confirm": true, "superseded_by": id2 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["superseded_by"], id2);

    let history = owner.get(&format!("{dpath}/versions")).await;
    let history = history.body.as_array().unwrap();
    assert_eq!(
        history.len(),
        5,
        "created, edited, submitted, approved, superseded"
    );
    assert_eq!(history[0]["status"], "superseded");
    assert_eq!(
        history.last().unwrap()["snapshot"]["title"],
        "Use base isolation"
    );
    assert_eq!(history.last().unwrap()["snapshot"]["status"], "draft");

    // Archived projects reject new content.
    owner
        .patch(
            &format!("/api/v1/orgs/{org}/projects/{project}"),
            json!({ "status": "archived" }),
        )
        .await;
    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/documents"),
            json!({ "title": "Late", "category": "general" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = owner.get(&format!("/api/v1/orgs/{org}/projects")).await;
    assert_eq!(
        r.body["items"].as_array().unwrap().len(),
        0,
        "archived hidden by default"
    );
    let r = owner
        .get(&format!(
            "/api/v1/orgs/{org}/projects?include_archived=true"
        ))
        .await;
    assert_eq!(r.body["items"].as_array().unwrap().len(), 1);

    // Every change above was audited.
    let audit = owner
        .get(&format!("/api/v1/orgs/{org}/audit-events?limit=100"))
        .await;
    let types: Vec<&str> = audit.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    for expected in [
        "organization.created",
        "invitation.created",
        "invitation.accepted",
        "project.created",
        "project.updated",
        "document.created",
        "document.version_created",
        "document.approved",
        "document.exported",
        "decision.created",
        "decision.updated",
        "decision.status_changed",
    ] {
        assert!(types.contains(&expected), "missing audit event {expected}");
    }
    db.drop().await;
}
