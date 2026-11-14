//! QUORUM Command provider runs: fixture mode, consent, failure handling
//! and usage recording. Live calls go to a local mock server only.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use quorum_providers::ProviderId;
use serde_json::json;

#[tokio::test]
async fn fixture_runs_are_labelled_and_recorded_without_prompt_text() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Org").await;

    let catalog = c.get(&format!("/api/v1/orgs/{org}/providers")).await;
    assert_eq!(catalog.status, StatusCode::OK);
    let providers = catalog.body["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 4);
    assert!(
        providers
            .iter()
            .all(|p| p["status"] == "disabled" && p["fixture_available"] == true)
    );
    assert_eq!(catalog.body["live_enabled"], false);

    let prompt = "CONFIDENTIAL-MARKER synthetic planning brief";
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/provider-runs"),
            json!({ "provider": "gemini", "task": "research", "prompt": prompt, "mode": "fixture" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["mode"], "fixture");
    assert_eq!(r.body["model"], "fixture");
    assert!(
        r.body["text"]
            .as_str()
            .unwrap()
            .starts_with("Illustrative fixture")
    );
    assert!(
        r.body["notice"]
            .as_str()
            .unwrap()
            .contains("No AI provider was contacted")
    );

    let usage = c.get(&format!("/api/v1/orgs/{org}/provider-runs")).await;
    let item = &usage.body["items"][0];
    assert_eq!(item["provider"], "gemini");
    assert_eq!(item["mode"], "fixture");
    assert_eq!(item["prompt_chars"], prompt.chars().count());

    // The brief itself is stored nowhere.
    let dump: String = sqlx::query_scalar(
        "SELECT (SELECT coalesce(string_agg(details::text, ' '), '') FROM audit_events) ||
                (SELECT coalesce(string_agg(row_to_json(p)::text, ' '), '') FROM provider_requests p)",
    )
    .fetch_one(&db.owner_pool)
    .await
    .unwrap();
    assert!(!dump.contains("CONFIDENTIAL-MARKER"));
    db.drop().await;
}

#[tokio::test]
async fn live_requests_need_consent_and_never_fall_back_to_fixtures() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut c, org) = signup(&app, "owner", "Org").await;
    let run = |ack: bool| {
        json!({ "provider": "openai", "task": "planning", "prompt": "Synthetic", "mode": "live",
                "acknowledge_external_transfer": ack })
    };
    let r = c
        .post(&format!("/api/v1/orgs/{org}/provider-runs"), run(false))
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["error"]["field"], "acknowledge_external_transfer");

    // Live disabled server-wide: an explicit failure, not fixture text.
    let r = c
        .post(&format!("/api/v1/orgs/{org}/provider-runs"), run(true))
        .await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
    assert_eq!(r.body["error"]["code"], "provider_failed");
    assert!(
        r.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No fallback")
    );
    assert!(r.body.get("text").is_none());
    let usage = c.get(&format!("/api/v1/orgs/{org}/provider-runs")).await;
    assert_eq!(usage.body["items"][0]["outcome"], "failed");

    // Bad inputs.
    for body in [
        json!({ "provider": "mystery", "task": "planning", "prompt": "x", "mode": "fixture" }),
        json!({ "provider": "openai", "task": "exploit", "prompt": "x", "mode": "fixture" }),
        json!({ "provider": "openai", "task": "planning", "prompt": "x", "mode": "agent" }),
    ] {
        let r = c
            .post(&format!("/api/v1/orgs/{org}/provider-runs"), body)
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text);
    }
    let r = c
        .post(
            &format!("/api/v1/orgs/{org}/provider-runs"),
            json!({ "provider": "openai", "task": "planning", "prompt": "x".repeat(8001), "mode": "fixture" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    db.drop().await;
}

#[tokio::test]
async fn live_request_through_mock_provider_records_usage() {
    use axum::routing::post;
    let mock = axum::Router::new().route(
        "/v1/messages",
        post(|| async {
            r#"{"content":[{"type":"text","text":"Mock live answer."}],"usage":{"input_tokens":11,"output_tokens":7}}"#
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });

    let db = TestDb::create().await;
    let mut config = test_config(&[
        ("QUORUM_PROVIDERS_LIVE", "1"),
        ("ANTHROPIC_API_KEY", "synthetic-test-key"),
        ("ANTHROPIC_MODEL", "mock-model"),
        ("QUORUM_PROVIDER_COOLDOWN_SECONDS", "60"),
    ]);
    config
        .providers
        .configs
        .get_mut(&ProviderId::Anthropic)
        .unwrap()
        .base_url = format!("http://{addr}");
    let app = app(&db, config);
    let (mut c, org) = signup(&app, "owner", "Org").await;

    let catalog = c.get(&format!("/api/v1/orgs/{org}/providers")).await;
    let anthropic = catalog.body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "anthropic")
        .unwrap()
        .clone();
    assert_eq!(
        anthropic["status"], "configured",
        "configured, never claimed as verified"
    );
    assert!(
        !catalog.text.contains("synthetic-test-key"),
        "keys never leave the server"
    );

    let body = json!({ "provider": "anthropic", "task": "review", "prompt": "Synthetic", "mode": "live",
                       "acknowledge_external_transfer": true });
    let r = c
        .post(&format!("/api/v1/orgs/{org}/provider-runs"), body.clone())
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["mode"], "live");
    assert_eq!(r.body["model"], "mock-model");
    assert_eq!(r.body["text"], "Mock live answer.");
    assert_eq!(r.body["usage"]["output_tokens"], 7);

    // Cooldown between live requests from the same user.
    let r = c
        .post(&format!("/api/v1/orgs/{org}/provider-runs"), body)
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);

    let usage = c.get(&format!("/api/v1/orgs/{org}/provider-runs")).await;
    let item = &usage.body["items"][0];
    assert_eq!(item["mode"], "live");
    assert_eq!(item["outcome"], "succeeded");
    assert_eq!(item["input_tokens"], 11);
    db.drop().await;
}
