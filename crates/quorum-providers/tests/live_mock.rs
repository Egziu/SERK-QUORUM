#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Exercises the live HTTP path against a local mock server. No real
//! provider is contacted and no credentials are used.

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use quorum_providers::*;
use std::collections::BTreeMap;
use std::time::Duration;

async fn spawn(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn settings(provider: ProviderId, base_url: String, timeout_ms: u64) -> ProviderSettings {
    let mut configs = BTreeMap::new();
    configs.insert(
        provider,
        ProviderConfig {
            api_key: ApiKey::new("mock-key".into()),
            model: "mock-model".into(),
            base_url,
        },
    );
    ProviderSettings {
        live_enabled: true,
        configs,
        max_output_tokens: 256,
        timeout: Duration::from_millis(timeout_ms),
    }
}

fn request(provider: ProviderId) -> CompletionRequest<'static> {
    CompletionRequest {
        provider,
        task: Task::Planning,
        prompt: "Synthetic brief",
    }
}

#[tokio::test]
async fn anthropic_round_trip_sends_key_and_parses_text() {
    let app = Router::new().route(
        "/v1/messages",
        post(|headers: HeaderMap, body: String| async move {
            assert_eq!(headers["x-api-key"], "mock-key");
            assert!(body.contains("Synthetic brief"));
            include_str!("../fixtures/anthropic_messages.json")
        }),
    );
    let s = settings(ProviderId::Anthropic, spawn(app).await, 5_000);
    let c = complete_live(&reqwest::Client::new(), &s, &request(ProviderId::Anthropic))
        .await
        .expect("completion");
    assert_eq!(c.mode, Mode::Live);
    assert_eq!(c.model, "mock-model");
    assert_eq!(c.text, "First block.\nSecond block.");
    assert_eq!(c.usage.and_then(|u| u.input_tokens), Some(120));
}

#[tokio::test]
async fn openai_round_trip() {
    let app = Router::new().route(
        "/v1/responses",
        post(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], "Bearer mock-key");
            include_str!("../fixtures/openai_responses.json")
        }),
    );
    let s = settings(ProviderId::Openai, spawn(app).await, 5_000);
    let c = complete_live(&reqwest::Client::new(), &s, &request(ProviderId::Openai))
        .await
        .expect("completion");
    assert_eq!(c.text, "Plan outline.");
}

#[tokio::test]
async fn provider_error_status_is_reported_not_replaced_with_fixture() {
    let app = Router::new().route(
        "/v1/responses",
        post(|| async { (StatusCode::TOO_MANY_REQUESTS, "{}") }),
    );
    let s = settings(ProviderId::Xai, spawn(app).await, 5_000);
    let e = complete_live(&reqwest::Client::new(), &s, &request(ProviderId::Xai)).await;
    assert_eq!(e.err(), Some(ProviderError::Rejected(429)));
}

#[tokio::test]
async fn slow_provider_times_out() {
    let app = Router::new().route(
        "/v1beta/models/{model}",
        post(|| async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            "{}"
        }),
    );
    let s = settings(ProviderId::Gemini, spawn(app).await, 100);
    let e = complete_live(&reqwest::Client::new(), &s, &request(ProviderId::Gemini)).await;
    assert_eq!(e.err(), Some(ProviderError::Timeout));
}

#[tokio::test]
async fn non_json_body_is_invalid_response() {
    let app = Router::new().route("/v1/messages", post(|| async { "<html>gateway</html>" }));
    let s = settings(ProviderId::Anthropic, spawn(app).await, 5_000);
    let e = complete_live(&reqwest::Client::new(), &s, &request(ProviderId::Anthropic)).await;
    assert_eq!(e.err(), Some(ProviderError::InvalidResponse));
}

#[tokio::test]
async fn unreachable_provider_is_network_error() {
    let s = settings(ProviderId::Anthropic, "http://127.0.0.1:9".into(), 2_000);
    let e = complete_live(&reqwest::Client::new(), &s, &request(ProviderId::Anthropic)).await;
    assert!(matches!(
        e.err(),
        Some(ProviderError::Network | ProviderError::Timeout)
    ));
}

#[tokio::test]
async fn connection_check_reports_working_and_sends_only_the_fixed_prompt() {
    let app = Router::new().route(
        "/v1/messages",
        post(|headers: HeaderMap, body: String| async move {
            assert_eq!(headers["x-api-key"], "mock-key");
            assert!(body.contains(CHECK_PROMPT));
            assert!(body.contains("\"max_tokens\":16"), "{body}");
            include_str!("../fixtures/anthropic_messages.json")
        }),
    );
    let s = settings(ProviderId::Anthropic, spawn(app).await, 5_000);
    let r = check_connection(&reqwest::Client::new(), &s, ProviderId::Anthropic).await;
    assert_eq!(r.outcome, CheckOutcome::Working);
    assert_eq!(r.http_status, Some(200));
    assert_eq!(r.model.as_deref(), Some("mock-model"));
    assert!(r.reply.is_some_and(|t| t.chars().count() <= 80));
}

#[tokio::test]
async fn connection_check_classifies_failures() {
    for (code, expected) in [
        (StatusCode::UNAUTHORIZED, CheckOutcome::KeyRejected),
        (StatusCode::NOT_FOUND, CheckOutcome::ModelOrEndpointNotFound),
        (StatusCode::TOO_MANY_REQUESTS, CheckOutcome::RateLimited),
        (StatusCode::BAD_GATEWAY, CheckOutcome::ProviderError),
    ] {
        let app = Router::new().route("/v1/responses", post(move || async move { (code, "{}") }));
        let s = settings(ProviderId::Openai, spawn(app).await, 5_000);
        let r = check_connection(&reqwest::Client::new(), &s, ProviderId::Openai).await;
        assert_eq!(r.outcome, expected, "{code}");
        assert_eq!(r.http_status, Some(code.as_u16()));
        assert!(r.reply.is_none());
    }
    // Timeout and unreachable.
    let app = Router::new().route(
        "/v1/responses",
        post(|| async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            "{}"
        }),
    );
    let s = settings(ProviderId::Xai, spawn(app).await, 50);
    let r = check_connection(&reqwest::Client::new(), &s, ProviderId::Xai).await;
    assert_eq!(r.outcome, CheckOutcome::Timeout);
    let s = settings(ProviderId::Gemini, "http://127.0.0.1:1".into(), 2_000);
    let r = check_connection(&reqwest::Client::new(), &s, ProviderId::Gemini).await;
    assert_eq!(r.outcome, CheckOutcome::Unreachable);
    // Live switched off or provider missing: no request at all.
    let mut s = settings(ProviderId::Gemini, "http://127.0.0.1:1".into(), 2_000);
    s.live_enabled = false;
    let r = check_connection(&reqwest::Client::new(), &s, ProviderId::Gemini).await;
    assert_eq!(r.outcome, CheckOutcome::LiveDisabled);
    let r = check_connection(
        &reqwest::Client::new(),
        &settings(ProviderId::Gemini, String::new(), 1),
        ProviderId::Openai,
    )
    .await;
    assert_eq!(r.outcome, CheckOutcome::NotConfigured);
}
