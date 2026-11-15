//! QUORUM Connect (Web Connect) and Council, end to end through the API with
//! no provider credentials configured. All content is synthetic.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};
use sqlx::Row;

const ROLES: [(&str, &str); 4] = [
    ("openai", "architect"),
    ("anthropic", "builder"),
    ("gemini", "scientist"),
    ("xai", "challenger"),
];

fn synthetic_answer(role: &str) -> String {
    format!(
        "Synthetic {role} answer.\r\nRecommendation: adopt option A for the fictional pump station.\r\nRisk: none known.\r\n"
    )
}

async fn create_brief(c: &mut Client, org: &str, project: &str, classification: &str) -> Value {
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/briefs"),
            json!({
                "title": "Pump station control upgrade",
                "mode": "professional",
                "classification": classification,
                "content": {
                    "request": "Choose a control architecture for a fictional pump station.",
                    "current_situation": "Legacy relay panel (synthetic).",
                    "desired_outcome": "A maintainable PLC-based design.",
                    "constraints": "Budget is fictional.",
                    "evaluation_criteria": "Safety, cost, maintainability"
                }
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "brief: {}", r.text);
    r.body
}

/// Prepares handoffs for four roles and imports four synthetic answers.
async fn four_imported_responses(
    c: &mut Client,
    org: &str,
    project: &str,
    brief: &Value,
) -> Vec<String> {
    let brief_id = brief["id"].as_str().unwrap();
    let mut ids = Vec::new();
    for (provider, role) in ROLES {
        let r = c
            .post(
                &format!("/api/v1/orgs/{org}/briefs/{brief_id}/handoffs"),
                json!({ "provider": provider, "role": role, "base_version": brief["version"] }),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "handoff: {}", r.text);
        let handoff = r.body["id"].as_str().unwrap().to_owned();
        let r = c
            .post(
                &format!("/api/v1/orgs/{org}/projects/{project}/responses"),
                json!({
                    "handoff_id": handoff,
                    "provider": provider,
                    "model": "declared-model",
                    "text": synthetic_answer(role),
                    "source": "web_import",
                }),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "import: {}", r.text);
        ids.push(r.body["id"].as_str().unwrap().to_owned());
    }
    ids
}

#[tokio::test]
async fn web_connect_works_without_any_api_credentials() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Synthetic Org").await;
    let project = create_project(&mut c, &org, "Pump station").await;

    // Catalog: official HTTPS destinations, honest states, no API keys needed.
    let r = c.get(&format!("/api/v1/orgs/{org}/connect")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let providers = r.body["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 4);
    for p in providers {
        let url = p["website_url"].as_str().unwrap();
        assert!(url.starts_with("https://") && !url.contains('?'), "{url}");
        assert_eq!(p["web_status"], "available");
        assert_ne!(
            p["api_status"], "configured",
            "no API key is configured: {p}"
        );
    }
    assert_eq!(r.body["roles"].as_array().unwrap().len(), 4);
    assert_eq!(r.body["external_share_limit"], "internal");

    let brief = create_brief(&mut c, &org, &project, "internal").await;
    let brief_id = brief["id"].as_str().unwrap();
    assert_eq!(brief["version"], 1);

    // Preview renders deterministic instructions and stores nothing.
    let preview = |c: &mut Client| {
        let mut c = c.clone();
        let path = format!("/api/v1/orgs/{org}/briefs/{brief_id}/preview");
        async move {
            c.post(
                &path,
                json!({ "assignments": ROLES.iter().map(|(p, r)| json!({"provider": p, "role": r})).collect::<Vec<_>>() }),
            )
            .await
        }
    };
    let p1 = preview(&mut c).await;
    let p2 = preview(&mut c).await;
    assert_eq!(p1.status, StatusCode::OK, "{}", p1.text);
    assert_eq!(
        p1.body["instructions"], p2.body["instructions"],
        "rendering is deterministic"
    );
    assert_eq!(p1.body["sharing_allowed"], true);
    let first = p1.body["instructions"][0]["instruction"].as_str().unwrap();
    assert!(first.contains("# Role: Architect"));
    assert!(first.contains("fictional pump station"));
    let r = c
        .get(&format!("/api/v1/orgs/{org}/briefs/{brief_id}/handoffs"))
        .await;
    assert_eq!(
        r.body.as_array().unwrap().len(),
        0,
        "preview stored nothing"
    );

    // Handoff with an edited instruction keeps the exact approved text and digest.
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{brief_id}/handoffs"),
            json!({ "provider": "openai", "role": "architect", "base_version": 1,
                    "instruction": "Edited synthetic instruction." }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["instruction"], "Edited synthetic instruction.");
    assert_eq!(r.body["status"], "prepared");
    assert_eq!(r.body["website_url"], "https://chatgpt.com/");
    assert_eq!(r.body["instruction_sha256"].as_str().unwrap().len(), 64);
    let handoff = r.body["id"].as_str().unwrap().to_owned();
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/handoffs/{handoff}/opened"),
            json!({}),
        )
        .await;
    assert_eq!(r.body["status"], "opened", "{}", r.text);

    // A stale brief version is refused.
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{brief_id}/handoffs"),
            json!({ "provider": "openai", "role": "architect", "base_version": 7 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Import: line endings normalized, user-declared provenance, handoff marked imported.
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/responses"),
            json!({ "handoff_id": handoff, "provider": "openai", "model": "declared",
                    "text": synthetic_answer("architect"), "source": "web_import",
                    "reference_url": "https://chatgpt.com/share/synthetic" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["provenance"], "user_declared");
    assert_eq!(r.body["source_mode"], "web_import");
    assert_eq!(r.body["brief_version"], 1);
    assert!(!r.body["body"].as_str().unwrap().contains('\r'));
    let first_response = r.body["id"].as_str().unwrap().to_owned();
    let r = c
        .get(&format!("/api/v1/orgs/{org}/briefs/{brief_id}/handoffs"))
        .await;
    assert_eq!(
        r.body[0]["status"], "imported",
        "handoffs are listed in creation order"
    );

    // The same text again (with different line endings) is a duplicate.
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/responses"),
            json!({ "provider": "openai", "text": synthetic_answer("architect").replace("\r\n", "\n"),
                    "source": "file_import" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text);
    assert_eq!(r.body["error"]["code"], "duplicate_response");
    assert_eq!(
        r.body["error"]["existing_response_id"],
        first_response.as_str()
    );

    // Input validation at the trust boundary.
    for (body, field) in [
        (
            json!({ "provider": "openai", "text": "x", "source": "api" }),
            "source",
        ),
        (
            json!({ "provider": "openai", "text": "", "source": "web_import" }),
            "text",
        ),
        (
            json!({ "provider": "acme", "text": "x", "source": "web_import" }),
            "provider",
        ),
        (
            json!({ "provider": "other", "text": "y", "source": "web_import", "reference_url": "javascript:alert(1)" }),
            "reference_url",
        ),
        (
            json!({ "provider": "other", "text": "z\u{0007}", "source": "web_import" }),
            "text",
        ),
    ] {
        let r = c
            .post(
                &format!("/api/v1/orgs/{org}/projects/{project}/responses"),
                body,
            )
            .await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}: {}",
            r.text
        );
        assert_eq!(r.body["error"]["field"], field, "{}", r.text);
    }
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/responses"),
            json!({ "provider": "other", "text": "a".repeat(200_001), "source": "web_import" }),
        )
        .await;
    assert_ne!(r.status, StatusCode::CREATED);

    // Review status changes; nothing else about the response can.
    let r = c
        .patch(
            &format!("/api/v1/orgs/{org}/responses/{first_response}"),
            json!({ "review_status": "accepted" }),
        )
        .await;
    assert_eq!(r.body["review_status"], "accepted", "{}", r.text);

    // Brief edits append versions with optimistic concurrency.
    let r = c
        .patch(
            &format!("/api/v1/orgs/{org}/briefs/{brief_id}"),
            json!({ "title": "Pump station v2", "mode": "quick", "classification": "internal",
                    "content": { "request": "Shorter synthetic request" }, "base_version": 1 }),
        )
        .await;
    assert_eq!(r.body["version"], 2, "{}", r.text);
    let r = c
        .patch(
            &format!("/api/v1/orgs/{org}/briefs/{brief_id}"),
            json!({ "title": "Stale", "mode": "quick", "classification": "internal",
                    "content": { "request": "x" }, "base_version": 1 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // The audit trail records metadata, never the instruction or response text.
    let r = c
        .get(&format!("/api/v1/orgs/{org}/audit-events?limit=100"))
        .await;
    for event in [
        "brief.created",
        "handoff.prepared",
        "handoff.website_opened",
        "response.imported",
        "response.reviewed",
    ] {
        assert!(r.text.contains(event), "missing {event}");
    }
    assert!(!r.text.contains("Edited synthetic instruction"));
    assert!(!r.text.contains("Recommendation: adopt option A"));
    db.drop().await;
}

#[tokio::test]
async fn classification_policy_blocks_external_sharing() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut c, &org, "P").await;
    let brief = create_brief(&mut c, &org, &project, "confidential").await;
    let id = brief["id"].as_str().unwrap();

    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{id}/preview"),
            json!({ "assignments": [{ "provider": "openai", "role": "architect" }] }),
        )
        .await;
    assert_eq!(r.body["sharing_allowed"], false, "{}", r.text);
    let handoff = json!({ "provider": "openai", "role": "architect", "base_version": 1 });
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{id}/handoffs"),
            handoff.clone(),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.text);
    assert_eq!(r.body["error"]["code"], "policy_blocked");

    // Only members.manage may raise the limit; then sharing is allowed and audited.
    let (mut contributor, _) = add_member(&app, &mut c, &org, "carol", "contributor").await;
    let r = contributor
        .put(
            &format!("/api/v1/orgs/{org}/connect/policy"),
            json!({ "external_share_limit": "restricted" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = c
        .put(
            &format!("/api/v1/orgs/{org}/connect/policy"),
            json!({ "external_share_limit": "confidential" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text);
    let r = c
        .post(&format!("/api/v1/orgs/{org}/briefs/{id}/handoffs"), handoff)
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let r = c
        .get(&format!("/api/v1/orgs/{org}/audit-events?limit=50"))
        .await;
    assert!(r.text.contains("policy.external_share_limit_changed"));
    db.drop().await;
}

#[tokio::test]
async fn context_is_limited_to_the_briefs_project_and_versioned() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut c, &org, "P").await;
    let other = create_project(&mut c, &org, "Other").await;
    let doc = create_document(&mut c, &org, &project, "Approved spec").await;
    let foreign = create_document(&mut c, &org, &other, "Other spec").await;
    let brief = create_brief(&mut c, &org, &project, "internal").await;
    let id = brief["id"].as_str().unwrap();

    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{id}/handoffs"),
            json!({ "provider": "anthropic", "role": "builder", "base_version": 1,
                    "context": [{ "kind": "document", "id": doc }] }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert!(
        r.body["instruction"]
            .as_str()
            .unwrap()
            .contains("Synthetic body")
    );
    assert_eq!(r.body["context_items"][0]["version"], 1);
    assert_eq!(r.body["context_items"][0]["id"], doc.as_str());

    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{id}/handoffs"),
            json!({ "provider": "anthropic", "role": "builder", "base_version": 1,
                    "context": [{ "kind": "document", "id": foreign }] }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text);
    db.drop().await;
}

#[tokio::test]
async fn roles_permissions_and_custom_role_templates() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut owner, &org, "P").await;
    let brief = create_brief(&mut owner, &org, &project, "internal").await;
    let id = brief["id"].as_str().unwrap();

    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/roles"),
            json!({ "name": "Safety reviewer", "focus": "Hazards", "guidance": "Focus on hazards and mitigations." }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let role_key = r.body["key"].as_str().unwrap().to_owned();
    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/roles"),
            json!({ "name": "safety REVIEWER", "guidance": "dup" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = owner
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{id}/preview"),
            json!({ "assignments": [{ "provider": "gemini", "role": role_key }] }),
        )
        .await;
    assert!(
        r.body["instructions"][0]["instruction"]
            .as_str()
            .unwrap()
            .contains("# Role: Safety reviewer")
    );

    // read_only can read but not prepare handoffs or import.
    let (mut reader, _) = add_member(&app, &mut owner, &org, "rita", "read_only").await;
    assert_eq!(
        reader
            .get(&format!("/api/v1/orgs/{org}/briefs/{id}"))
            .await
            .status,
        StatusCode::OK
    );
    let r = reader
        .post(
            &format!("/api/v1/orgs/{org}/briefs/{id}/handoffs"),
            json!({ "provider": "openai", "role": "architect", "base_version": 1 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = reader
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/responses"),
            json!({ "provider": "openai", "text": "x", "source": "web_import" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = reader
        .delete(&format!("/api/v1/orgs/{org}/roles/{role_key}"))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = owner
        .delete(&format!("/api/v1/orgs/{org}/roles/{role_key}"))
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    db.drop().await;
}

#[tokio::test]
async fn council_compares_responses_and_records_a_human_decision() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut c, &org, "Pump station").await;
    let brief = create_brief(&mut c, &org, &project, "internal").await;
    let responses = four_imported_responses(&mut c, &org, &project, &brief).await;

    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/councils"),
            json!({ "title": "Control architecture", "question": "Which option?", "response_ids": responses }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["status"], "ready_for_review");
    assert_eq!(r.body["members"].as_array().unwrap().len(), 4);
    assert_eq!(r.body["members"][1]["response"]["role_name"], "Builder");
    let council = r.body["id"].as_str().unwrap().to_owned();
    let base = format!("/api/v1/orgs/{org}/councils/{council}");

    let r = c
        .post(
            &format!("{base}/transition"),
            json!({ "to": "under_review", "base_version": r.body["version"] }),
        )
        .await;
    assert_eq!(r.body["status"], "under_review", "{}", r.text);

    let r = c
        .post(
            &format!("{base}/annotations"),
            json!({ "response_id": responses[3], "kind": "risk", "quote": "Risk: none known.", "body": "Challenger understates commissioning risk." }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let r = c
        .put(
            &format!("{base}/ratings"),
            json!({ "response_id": responses[0], "criterion": "technical_feasibility", "score": 4, "reason": "Plausible" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    // Rating again updates in place (one rating per person/criterion/response).
    let r = c
        .put(&format!("{base}/ratings"), json!({ "response_id": responses[0], "criterion": "technical_feasibility", "score": 5 }))
        .await;
    assert_eq!(r.body["ratings"].as_array().unwrap().len(), 1);
    assert_eq!(r.body["ratings"][0]["score"], 5);
    let r = c
        .put(
            &format!("{base}/ratings"),
            json!({ "response_id": responses[0], "criterion": "clarity", "score": 9 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Recording the decision requires explicit confirmation and DecisionsDecide.
    let version = c.get(&base).await.body["version"].clone();
    let r = c
        .post(
            &format!("{base}/decision"),
            json!({ "summary": "Adopt option A", "base_version": version }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let (mut contributor, _) = add_member(&app, &mut c, &org, "carol", "contributor").await;
    let r = contributor
        .post(
            &format!("{base}/decision"),
            json!({ "summary": "x", "base_version": version, "confirm": true }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = c
        .post(
            &format!("{base}/decision"),
            json!({ "summary": "Adopt option A", "base_version": version, "confirm": true }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["status"], "decision_recorded");
    assert_eq!(r.body["decision"]["reference"], "DEC-0001");
    assert_eq!(r.body["can_edit"], false);
    let decision = r.body["decision"]["id"].as_str().unwrap().to_owned();

    // The decision record lands in the register, owned by a human.
    let r = c
        .get(&format!("/api/v1/orgs/{org}/decisions/{decision}"))
        .await;
    assert_eq!(r.body["outcome"], "Adopt option A");
    assert_eq!(r.body["status"], "draft");
    assert!(
        r.body["alternatives"]
            .as_str()
            .unwrap()
            .contains("Architect")
    );
    assert!(
        r.body["risks"]
            .as_str()
            .unwrap()
            .contains("commissioning risk")
    );

    // Completed councils are frozen.
    let frozen = [
        c.post(
            &format!("{base}/annotations"),
            json!({ "kind": "note", "body": "late" }),
        )
        .await,
        c.put(
            &format!("{base}/ratings"),
            json!({ "response_id": responses[1], "criterion": "clarity", "score": 3 }),
        )
        .await,
        c.patch(&base, json!({ "title": "Renamed", "base_version": 99 }))
            .await,
        c.delete(&format!("{base}/responses/{}", responses[2]))
            .await,
    ];
    for r in frozen {
        assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text);
    }
    let r = sqlx::query("UPDATE councils SET title = 'tamper' WHERE id = $1::uuid")
        .bind(&council)
        .execute(&db.owner_pool)
        .await;
    assert!(
        r.is_err(),
        "the database refuses edits to completed councils"
    );

    // Export: Markdown with provenance, and a Studio document.
    let r = c.get(&format!("{base}/export.md")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/markdown")
    );
    for needle in [
        "# Council: Control architecture",
        "Adopt option A",
        "user_declared",
        "SHA-256",
        "technical_feasibility: 5",
    ] {
        assert!(r.text.contains(needle), "export lacks {needle}");
    }
    let r = c.post(&format!("{base}/export-document"), json!({})).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let doc = r.body["document_id"].as_str().unwrap();
    let r = c.get(&format!("/api/v1/orgs/{org}/documents/{doc}")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert!(r.text.contains("Council report: Control architecture"));

    // Archive needs DecisionsDecide; archived is final.
    let version = c.get(&base).await.body["version"].clone();
    let r = c
        .post(
            &format!("{base}/transition"),
            json!({ "to": "archived", "base_version": version }),
        )
        .await;
    assert_eq!(r.body["status"], "archived", "{}", r.text);
    db.drop().await;
}

#[tokio::test]
async fn council_replacement_keeps_history_and_rejects_foreign_responses() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut c, &org, "P").await;
    let other = create_project(&mut c, &org, "Other").await;
    let brief = create_brief(&mut c, &org, &project, "internal").await;
    let ids = four_imported_responses(&mut c, &org, &project, &brief).await;
    let foreign = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{other}/responses"),
            json!({ "provider": "other", "text": "Other project answer", "source": "web_import" }),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/projects/{project}/councils"),
            json!({ "title": "Review" }),
        )
        .await;
    assert_eq!(r.body["status"], "draft");
    let base = format!(
        "/api/v1/orgs/{org}/councils/{}",
        r.body["id"].as_str().unwrap()
    );

    let r = c
        .post(
            &format!("{base}/responses"),
            json!({ "response_id": ids[0] }),
        )
        .await;
    assert_eq!(r.body["status"], "awaiting_responses", "{}", r.text);
    let r = c
        .post(
            &format!("{base}/responses"),
            json!({ "response_id": ids[1] }),
        )
        .await;
    assert_eq!(r.body["status"], "ready_for_review");
    let r = c
        .post(
            &format!("{base}/responses"),
            json!({ "response_id": foreign }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.text);
    let r = c
        .post(
            &format!("{base}/responses"),
            json!({ "response_id": ids[0] }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Replacing position 1 retires the old response but keeps the record.
    let r = c
        .post(
            &format!("{base}/responses"),
            json!({ "response_id": ids[2], "position": 1 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["members"][0]["response"]["id"], ids[2].as_str());
    assert_eq!(r.body["retired"][0]["response_id"], ids[0].as_str());
    assert_eq!(r.body["retired"][0]["replaced_by"], ids[2].as_str());

    // The original response itself is immutable at the database level.
    let err = sqlx::query("UPDATE responses SET body = 'tampered' WHERE id = $1::uuid")
        .bind(&ids[0])
        .execute(&db.owner_pool)
        .await;
    assert!(err.is_err(), "original response text must be immutable");
    let row = sqlx::query("SELECT body FROM responses WHERE id = $1::uuid")
        .bind(&ids[0])
        .fetch_one(&db.owner_pool)
        .await
        .unwrap();
    assert!(
        row.get::<String, _>("body")
            .starts_with("Synthetic architect answer")
    );
    db.drop().await;
}

#[tokio::test]
async fn connect_and_council_data_is_tenant_isolated() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut a, org_a) = signup(&app, "alice", "Tenant A").await;
    let project_a = create_project(&mut a, &org_a, "Secret A").await;
    let brief = create_brief(&mut a, &org_a, &project_a, "internal").await;
    let ids = four_imported_responses(&mut a, &org_a, &project_a, &brief).await;
    let council = a
        .post(
            &format!("/api/v1/orgs/{org_a}/projects/{project_a}/councils"),
            json!({ "title": "Secret council", "response_ids": ids }),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let brief_id = brief["id"].as_str().unwrap();
    let (mut b, org_b) = signup(&app, "bob", "Tenant B").await;
    let project_b = create_project(&mut b, &org_b, "B").await;

    for org in [&org_a, &org_b] {
        for path in [
            format!("/api/v1/orgs/{org}/briefs/{brief_id}"),
            format!("/api/v1/orgs/{org}/briefs/{brief_id}/handoffs"),
            format!("/api/v1/orgs/{org}/responses/{}", ids[0]),
            format!("/api/v1/orgs/{org}/councils/{council}"),
            format!("/api/v1/orgs/{org}/councils/{council}/export.md"),
        ] {
            let r = b.get(&path).await;
            assert_eq!(r.status, StatusCode::NOT_FOUND, "GET {path}: {}", r.text);
            assert!(!r.text.contains("Synthetic") && !r.text.contains("Secret"));
        }
    }
    for path in [
        format!("/api/v1/orgs/{org_a}/projects/{project_a}/briefs"),
        format!("/api/v1/orgs/{org_a}/projects/{project_a}/responses"),
        format!("/api/v1/orgs/{org_a}/projects/{project_a}/councils"),
        format!("/api/v1/orgs/{org_a}/connect"),
    ] {
        assert_eq!(b.get(&path).await.status, StatusCode::NOT_FOUND, "{path}");
    }
    // Confused deputy: A's ids used inside B's own organization.
    let r = b
        .post(
            &format!("/api/v1/orgs/{org_b}/projects/{project_b}/councils"),
            json!({ "title": "Steal", "response_ids": [ids[0]] }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.text);
    let r = b
        .post(
            &format!("/api/v1/orgs/{org_b}/projects/{project_b}/responses"),
            json!({ "handoff_id": ids[0], "provider": "openai", "text": "x", "source": "web_import" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // Row-level security as the application role, without application filters.
    // Static SQL only (sqlx refuses dynamic strings by design).
    let tables = [
        "SELECT count(*) FROM briefs",
        "SELECT count(*) FROM brief_versions",
        "SELECT count(*) FROM handoffs",
        "SELECT count(*) FROM responses",
        "SELECT count(*) FROM councils",
        "SELECT count(*) FROM council_responses",
        "SELECT count(*) FROM council_annotations",
        "SELECT count(*) FROM council_ratings",
        "SELECT count(*) FROM role_templates",
    ];
    let mut tx = db.app_pool.begin().await.unwrap();
    for t in tables {
        let n: i64 = sqlx::query_scalar(t).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(n, 0, "{t} visible without tenant context");
    }
    sqlx::query("SELECT set_config('app.org_id', $1, true)")
        .bind(&org_b)
        .execute(&mut *tx)
        .await
        .unwrap();
    for t in tables {
        let n: i64 = sqlx::query_scalar(t).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(n, 0, "{t} leaks tenant A rows into tenant B");
    }
    let err = sqlx::query(
        "INSERT INTO responses (id, organization_id, project_id, provider, source_mode, provenance, body, content_sha256, imported_by)
         VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'other', 'web_import', 'user_declared', 'x', '\\x00'::bytea, gen_random_uuid())",
    )
    .bind(&org_a)
    .bind(&project_a)
    .execute(&mut *tx)
    .await;
    assert!(err.is_err(), "writing into another tenant must be refused");
    tx.rollback().await.unwrap();
    db.drop().await;
}
