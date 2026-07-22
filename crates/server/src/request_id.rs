//! Per-request correlation context for HTTP responses and tracing.

use axum::{
    body::Body,
    http::{header::HeaderName, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

fn incoming_request_id(value: Option<&HeaderValue>) -> Option<String> {
    let value = value?.to_str().ok()?.trim();
    (!value.is_empty() && value.len() <= 128).then(|| value.to_string())
}

pub async fn request_id(request: Request<Body>, next: Next) -> Response {
    let id = incoming_request_id(request.headers().get(REQUEST_ID_HEADER))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut request = request;
    request.extensions_mut().insert(RequestId(id.clone()));
    let span = tracing::info_span!(
        "http.request",
        request_id = %id,
        method = %request.method(),
        path = %request.uri().path()
    );
    let mut response = next.run(request).instrument(span).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_incoming_request_id() {
        let value = HeaderValue::from_static("req-123");
        assert_eq!(incoming_request_id(Some(&value)), Some("req-123".into()));
    }

    #[test]
    fn rejects_missing_or_oversized_request_id() {
        assert_eq!(incoming_request_id(None), None);
        let value = HeaderValue::from_str(&"x".repeat(129)).unwrap();
        assert_eq!(incoming_request_id(Some(&value)), None);
    }
}
