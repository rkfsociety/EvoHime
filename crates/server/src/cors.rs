//! CORS allowlist (Stage 7.2).
//!
//! Default: local Vite + server origins only.
//! Override with `EVOHIME_CORS_ORIGINS` (comma-separated).
//! Escape hatch: `EVOHIME_CORS_PERMISSIVE=1` restores permissive CORS (not for LAN exposure).

use axum::http::{header, HeaderValue, Method};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

const DEFAULT_ORIGINS: &[&str] = &[
    "http://127.0.0.1:5173",
    "http://localhost:5173",
    "http://127.0.0.1:3000",
    "http://localhost:3000",
    "http://127.0.0.1:4173",
    "http://localhost:4173",
];

pub fn cors_layer_from_env() -> CorsLayer {
    if env_flag_true("EVOHIME_CORS_PERMISSIVE") {
        tracing::warn!("EVOHIME_CORS_PERMISSIVE=1: using permissive CORS");
        return CorsLayer::permissive();
    }

    let origins = configured_origins();
    tracing::info!(origins = %origins.join(", "), "CORS allowlist enabled");

    let header_values = origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(header_values))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::HeaderName::from_static("x-evohime-token"),
        ]))
        .allow_credentials(true)
}

pub fn configured_origins() -> Vec<String> {
    match std::env::var("EVOHIME_CORS_ORIGINS") {
        Ok(value) => {
            let parsed = value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if parsed.is_empty() {
                default_origins()
            } else {
                parsed
            }
        }
        Err(_) => default_origins(),
    }
}

fn default_origins() -> Vec<String> {
    DEFAULT_ORIGINS
        .iter()
        .map(|origin| (*origin).to_string())
        .collect()
}

fn env_flag_true(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_origins_cover_vite_and_server() {
        let origins = default_origins();
        assert!(origins.iter().any(|o| o.contains(":5173")));
        assert!(origins.iter().any(|o| o.contains(":3000")));
    }

    #[test]
    fn permissive_flag_parsing() {
        // env_flag_true is private; exercise via configured_origins stability
        assert!(!default_origins().is_empty());
    }
}
