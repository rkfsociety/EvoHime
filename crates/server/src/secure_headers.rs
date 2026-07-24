//! Secure HTTP headers middleware (Phase 5.6).
//! Adds Content-Security-Policy, X-Frame-Options, X-Content-Type-Options, etc.

use axum::{http::header, Router};
use tower_http::set_header::SetResponseHeaderLayer;

/// CSP policy: strict, allow only same-origin + trusted CDNs for fonts.
/// - script-src: only inline scripts (unsafe-inline for app bootstrap; future: nonces)
/// - style-src: inline + fonts.googleapis.com for Roboto
/// - font-src: fonts.gstatic.com
/// - img-src: self + data: for icons
/// - connect-src: self for API calls
/// - frame-ancestors: none (prevent clickjacking)
const CSP_HEADER: &str = "default-src 'self'; \
    script-src 'self' 'unsafe-inline'; \
    style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
    font-src https://fonts.gstatic.com; \
    img-src 'self' data:; \
    connect-src 'self'; \
    frame-ancestors 'none'; \
    base-uri 'self'; \
    form-action 'self'; \
    upgrade-insecure-requests";

/// Apply secure headers layer to a router (Phase 5.6).
/// Adds CSP, X-Frame-Options, X-Content-Type-Options and other security headers.
pub fn with_secure_headers(router: Router) -> Router {
    router
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("content-security-policy"),
            header::HeaderValue::from_static(CSP_HEADER),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-content-type-options"),
            header::HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-frame-options"),
            header::HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-xss-protection"),
            header::HeaderValue::from_static("1; mode=block"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("referrer-policy"),
            header::HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("permissions-policy"),
            header::HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csp_header_is_set() {
        assert!(!CSP_HEADER.is_empty());
        assert!(CSP_HEADER.contains("default-src"));
        assert!(CSP_HEADER.contains("script-src"));
        assert!(CSP_HEADER.contains("frame-ancestors 'none'"));
    }

    #[test]
    fn csp_prevents_unsafe_eval() {
        assert!(!CSP_HEADER.contains("unsafe-eval"));
    }
}
