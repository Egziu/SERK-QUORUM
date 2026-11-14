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
