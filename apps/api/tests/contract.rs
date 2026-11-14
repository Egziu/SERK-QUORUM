//! Keeps contracts/openapi.json and the router in sync.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

#[test]
fn openapi_contract_matches_router() {
    let spec: serde_json::Value =
        serde_json::from_str(include_str!("../../../contracts/openapi.json")).expect("valid JSON");
    assert_eq!(spec["openapi"], "3.1.0");
    let mut documented = BTreeSet::new();
    for (path, ops) in spec["paths"].as_object().expect("paths") {
        for method in ops.as_object().expect("ops").keys() {
            documented.insert((method.to_uppercase(), path.clone()));
        }
    }
    let routed: BTreeSet<(String, String)> = quorum_api::app::ROUTES
        .iter()
        .map(|(m, p)| ((*m).to_owned(), (*p).to_owned()))
        .collect();
    let missing: Vec<_> = routed.difference(&documented).collect();
    let stale: Vec<_> = documented.difference(&routed).collect();
    assert!(missing.is_empty(), "undocumented routes: {missing:?}");
    assert!(stale.is_empty(), "documented but not routed: {stale:?}");
}

/// Every declared route is really wired into the router with that method.
/// Uses a lazy pool that never connects: unauthenticated requests are
/// rejected before any database access.
#[tokio::test]
async fn declared_routes_are_wired() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let config = quorum_api::Config::from_lookup(|k| {
        (k == "DATABASE_URL").then(|| "postgres://unused@127.0.0.1:1/x".into())
    })
    .expect("config");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1:1/x")
        .expect("lazy pool");
    let app = quorum_api::router(quorum_api::AppState::with_http_client(
        config,
        pool,
        reqwest::Client::new(),
    ));
    let nil = "00000000-0000-0000-0000-000000000000";
    for (method, path) in quorum_api::app::ROUTES {
        let concrete = path
            .replace("{version}", "1")
            .split('/')
            .map(|seg| if seg.starts_with('{') { nil } else { seg })
            .collect::<Vec<_>>()
            .join("/");
        let req = Request::builder()
            .method(*method)
            .uri(&concrete)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let body = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes())
            .into_owned();
        assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "{method} {path}");
        assert!(
            !body.contains("Endpoint not found"),
            "{method} {path} is not routed"
        );
    }
}
