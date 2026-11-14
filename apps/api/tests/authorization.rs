//! Server-side role enforcement for every role, plus membership rules.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

/// Expected outcome of each guarded action for each role.
/// (create project, create document, approve document, create decision,
///  read audit, run provider, invite member, read usage)
const MATRIX: &[(&str, [bool; 8])] = &[
    (
        "administrator",
        [true, true, true, true, true, true, true, true],
    ),
    (
        "project_manager",
        [true, true, true, true, false, true, false, true],
    ),
    (
        "contributor",
        [false, true, false, true, false, true, false, false],
    ),
    (
        "reviewer",
        [false, false, true, false, false, false, false, false],
    ),
    (
        "billing_administrator",
        [false, false, false, false, false, false, false, true],
    ),
    (
        "auditor",
        [false, false, false, false, true, false, false, true],
    ),
    (
        "read_only",
        [false, false, false, false, false, false, false, false],
    ),
];

#[tokio::test]
async fn every_role_is_enforced_server_side() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Matrix Org").await;
    let project = create_project(&mut owner, &org, "Matrix").await;

    for (i, (role, expected)) in MATRIX.iter().enumerate() {
        let name = format!("user{i}");
        let (mut c, _) = add_member(&app, &mut owner, &org, &name, role).await;
        // Fresh draft document for the approval check.
        let doc = create_document(&mut owner, &org, &project, &format!("Doc for {role}")).await;

        let results = [
            c.post(&format!("/api/v1/orgs/{org}/projects"), json!({ "name": format!("P {role}") })).await,
            c.post(
                &format!("/api/v1/orgs/{org}/projects/{project}/documents"),
                json!({ "title": "T", "category": "general" }),
            )
            .await,
            c.post(&format!("/api/v1/orgs/{org}/documents/{doc}/approve"), json!({ "version": 1 })).await,
            c.post(
                &format!("/api/v1/orgs/{org}/projects/{project}/decisions"),
                json!({ "title": "D", "problem_statement": "P" }),
            )
            .await,
            c.get(&format!("/api/v1/orgs/{org}/audit-events")).await,
            c.post(
                &format!("/api/v1/orgs/{org}/provider-runs"),
                json!({ "provider": "anthropic", "task": "planning", "prompt": "Synthetic", "mode": "fixture" }),
            )
            .await,
            c.post(
                &format!("/api/v1/orgs/{org}/invitations"),
                json!({ "email": format!("guest{i}@example.test"), "role": "read_only" }),
            )
            .await,
            c.get(&format!("/api/v1/orgs/{org}/provider-runs")).await,
        ];
        for (j, (r, allowed)) in results.iter().zip(expected.iter()).enumerate() {
            if *allowed {
                assert!(
                    r.status.is_success(),
                    "{role} action {j} should succeed: {} {}",
                    r.status,
                    r.text
                );
            } else {
                assert_eq!(
                    r.status,
                    StatusCode::FORBIDDEN,
                    "{role} action {j} should be forbidden: {}",
                    r.text
                );
                assert_eq!(r.body["error"]["code"], "forbidden");
            }
        }
        // Every role can read the workspace.
        assert_eq!(
            c.get(&format!("/api/v1/orgs/{org}/projects/{project}"))
                .await
                .status,
            StatusCode::OK
        );
    }

    // Denials are audited.
    let audit = owner
        .get(&format!("/api/v1/orgs/{org}/audit-events?limit=100"))
        .await;
    assert!(
        audit.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event_type"] == "access.denied" && e["outcome"] == "denied")
    );
    db.drop().await;
}

#[tokio::test]
async fn decision_transitions_respect_author_and_decider_roles() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut owner, &org, "P").await;
    let (mut contributor, _) = add_member(&app, &mut owner, &org, "contrib", "contributor").await;
    let (mut reviewer, _) = add_member(&app, &mut owner, &org, "rev", "reviewer").await;

    let d = create_decision(&mut contributor, &org, &project, "Choose a database").await;
    let path = format!(
        "/api/v1/orgs/{org}/decisions/{}/transition",
        d["id"].as_str().unwrap()
    );
    // Reviewer cannot submit (authoring), contributor can.
    let r = reviewer
        .post(&path, json!({ "to": "under_review", "base_version": 1 }))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = contributor
        .post(&path, json!({ "to": "under_review", "base_version": 1 }))
        .await;
    assert_eq!(r.status, StatusCode::OK);
    // Contributor cannot approve their own decision; reviewer can.
    let r = contributor
        .post(
            &path,
            json!({ "to": "approved", "base_version": 2, "confirm": true }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = reviewer
        .post(
            &path,
            json!({ "to": "approved", "base_version": 2, "confirm": true }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);

    // A decision maker must be a member who may decide.
    let r = contributor
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/decisions"),
            json!({ "title": "X", "problem_statement": "Y", "decision_maker_id": d["created_by"] }),
        )
        .await;
    assert_eq!(
        r.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a contributor cannot be the decision maker"
    );

    // Submitting an incomplete decision is refused.
    let incomplete = contributor
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/decisions"),
            json!({ "title": "Incomplete", "problem_statement": "Y" }),
        )
        .await;
    let r = contributor
        .post(
            &format!(
                "/api/v1/orgs/{org}/decisions/{}/transition",
                incomplete.body["id"].as_str().unwrap()
            ),
            json!({ "to": "under_review", "base_version": 1 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["error"]["field"], "outcome");
    db.drop().await;
}

#[tokio::test]
async fn membership_management_rules() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Org").await;
    let owner_id = owner.get("/api/v1/auth/session").await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (mut admin, admin_id) = add_member(&app, &mut owner, &org, "admin", "administrator").await;
    let (_member, member_id) = add_member(&app, &mut owner, &org, "member", "contributor").await;

    // Administrators cannot grant, change or remove the owner role.
    let r = admin
        .post(
            &format!("/api/v1/orgs/{org}/invitations"),
            json!({ "email": "x@example.test", "role": "owner" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = admin
        .patch(
            &format!("/api/v1/orgs/{org}/members/{member_id}"),
            json!({ "role": "owner" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = admin
        .patch(
            &format!("/api/v1/orgs/{org}/members/{owner_id}"),
            json!({ "role": "read_only" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = admin
        .delete(&format!("/api/v1/orgs/{org}/members/{owner_id}"))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Administrators can manage ordinary roles.
    let r = admin
        .patch(
            &format!("/api/v1/orgs/{org}/members/{member_id}"),
            json!({ "role": "reviewer" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    // The last owner cannot be demoted or removed, even by themselves.
    let r = owner
        .patch(
            &format!("/api/v1/orgs/{org}/members/{owner_id}"),
            json!({ "role": "administrator" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    // After promoting another owner, demotion works.
    let r = owner
        .patch(
            &format!("/api/v1/orgs/{org}/members/{admin_id}"),
            json!({ "role": "owner" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = owner
        .patch(
            &format!("/api/v1/orgs/{org}/members/{owner_id}"),
            json!({ "role": "administrator" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    // Removal ends access immediately.
    let r = admin
        .delete(&format!("/api/v1/orgs/{org}/members/{member_id}"))
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let members = admin.get(&format!("/api/v1/orgs/{org}/members")).await;
    assert_eq!(members.body.as_array().unwrap().len(), 2);
    db.drop().await;
}

#[tokio::test]
async fn invitations_are_single_use_and_bound_to_the_invited_email() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Org").await;
    let (mut eve, _) = signup(&app, "eve", "Eve Org").await;
    let (mut carol, _) = signup(&app, "carol", "Carol Org").await;

    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/invitations"),
            json!({ "email": "carol@example.test", "role": "contributor" }),
        )
        .await;
    let token = r.body["token"].as_str().unwrap().to_owned();
    assert_eq!(r.body["invitation"]["status"], "pending");

    // Someone else holding the token cannot use it.
    let r = eve
        .post("/api/v1/invitations/accept", json!({ "token": token }))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Garbage tokens are rejected.
    let r = carol
        .post(
            "/api/v1/invitations/accept",
            json!({ "token": "not-a-token" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // The invitee can, once.
    let r = carol
        .post("/api/v1/invitations/accept", json!({ "token": token }))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let r = carol
        .post("/api/v1/invitations/accept", json!({ "token": token }))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // Revoked invitations cannot be used.
    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/invitations"),
            json!({ "email": "eve@example.test", "role": "read_only" }),
        )
        .await;
    let token = r.body["token"].as_str().unwrap().to_owned();
    let id = r.body["invitation"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        owner
            .delete(&format!("/api/v1/orgs/{org}/invitations/{id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let r = eve
        .post("/api/v1/invitations/accept", json!({ "token": token }))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // Tokens are stored only as digests.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invitations WHERE encode(token_digest, 'escape') LIKE $1",
    )
    .bind(format!("%{token}%"))
    .fetch_one(&db.owner_pool)
    .await
    .unwrap();
    assert_eq!(stored, 0);
    db.drop().await;
}
