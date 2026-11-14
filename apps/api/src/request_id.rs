//! Correlation identifiers. Each request gets an `x-request-id`: the
//! client's value if it is short and well-formed, otherwise a new UUID. The
//! identifier is echoed in the response header, included in error bodies and
//! recorded in audit events and logs.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

tokio::task_local! {
    static CORRELATION_ID: String;
}

pub const HEADER: &str = "x-request-id";

fn acceptable(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub async fn middleware(req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| acceptable(v))
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let span = tracing::info_span!("request", correlation_id = %id, method = %req.method(), path = %req.uri().path());
    let mut response = CORRELATION_ID
        .scope(
            id.clone(),
            tracing::Instrument::instrument(next.run(req), span),
        )
        .await;
    if let Ok(v) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(HEADER, v);
    }
    response
}

/// The current request's correlation identifier, or `"none"` outside a request.
pub fn current() -> String {
    CORRELATION_ID
        .try_with(Clone::clone)
        .unwrap_or_else(|_| "none".to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_unsafe_ids() {
        assert!(super::acceptable("abc-123_X"));
        assert!(!super::acceptable(""));
        assert!(!super::acceptable("a b"));
        assert!(!super::acceptable("<script>"));
        assert!(!super::acceptable(&"a".repeat(65)));
    }
}
