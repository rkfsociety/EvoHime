//! Local operator auth for HTTP + WebSocket (Stage 7.1).
//!
//! Policy:
//! - `/health` and `/api/auth/status` are always public (launcher + UI bootstrap).
//! - Valid `Authorization: Bearer <token>` / `X-EvoHime-Token` / `?access_token=` unlocks any peer.
//! - Loopback peers are allowed when `EVOHIME_API_TOKEN` is **unset** (local DX).
//! - Non-loopback peers without a matching token are rejected (401).
//! - When `EVOHIME_API_TOKEN` is set, it is required even from loopback.

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use crate::app::AppState;

const PUBLIC_PATHS: &[&str] = &["/health", "/api/auth/status"];

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// When set, requests must present this bearer token (even from loopback).
    pub api_token: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let api_token = std::env::var("EVOHIME_API_TOKEN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Self { api_token }
    }

    pub fn token_configured(&self) -> bool {
        self.api_token.is_some()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub token_configured: bool,
    pub mode: &'static str,
    pub hint: &'static str,
}

pub fn status_payload(config: &AuthConfig) -> AuthStatus {
    if config.token_configured() {
        AuthStatus {
            token_configured: true,
            mode: "bearer",
            hint: "Send Authorization: Bearer <EVOHIME_API_TOKEN> (or X-EvoHime-Token / ?access_token=).",
        }
    } else {
        AuthStatus {
            token_configured: false,
            mode: "loopback_open",
            hint: "Loopback clients allowed without token; set EVOHIME_API_TOKEN to require bearer auth.",
        }
    }
}

pub fn is_loopback_addr(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

pub fn extract_token(headers: &axum::http::HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let value = value.trim();
        if let Some(token) = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
        {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    if let Some(value) = headers
        .get("x-evohime-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_string());
    }
    query.and_then(token_from_query)
}

fn token_from_query(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next().unwrap_or("");
        if key == "access_token" || key == "token" {
            let decoded = urlencoding_decode(value);
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

fn urlencoding_decode(value: &str) -> String {
    // Minimal decode for token query values (percent-encoding).
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn authorize_request(
    config: &AuthConfig,
    path: &str,
    peer: Option<SocketAddr>,
    presented: Option<&str>,
) -> Result<(), StatusCode> {
    if PUBLIC_PATHS.contains(&path) {
        return Ok(());
    }

    if let (Some(expected), Some(got)) = (config.api_token.as_deref(), presented) {
        if tokens_equal(expected, got) {
            return Ok(());
        }
    }

    if config.api_token.is_some() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match peer {
        Some(addr) if is_loopback_addr(&addr) => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn tokens_equal(left: &str, right: &str) -> bool {
    // Constant-time-ish compare for equal lengths; length leak is acceptable for local tokens.
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

pub async fn require_local_auth(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(str::to_string);
    let presented = extract_token(request.headers(), query.as_deref());
    match authorize_request(&state.auth, &path, Some(addr), presented.as_deref()) {
        Ok(()) => next.run(request).await,
        Err(status) => (status, Json(crate::api_error::unauthorized_json())).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

    #[test]
    fn loopback_detection() {
        let v4 = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3000));
        let lan = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 5), 3000));
        let v6 = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 3000, 0, 0));
        assert!(is_loopback_addr(&v4));
        assert!(is_loopback_addr(&v6));
        assert!(!is_loopback_addr(&lan));
    }

    #[test]
    fn public_paths_always_open() {
        let config = AuthConfig {
            api_token: Some("secret".into()),
        };
        let remote = "192.168.1.5:9".parse().unwrap();
        assert!(authorize_request(&config, "/health", Some(remote), None).is_ok());
        assert!(authorize_request(&config, "/api/auth/status", Some(remote), None).is_ok());
    }

    #[test]
    fn loopback_open_without_token() {
        let config = AuthConfig::default();
        let local = "127.0.0.1:9".parse().unwrap();
        let remote = "10.0.0.2:9".parse().unwrap();
        assert!(authorize_request(&config, "/api/sessions", Some(local), None).is_ok());
        assert!(authorize_request(&config, "/api/sessions", Some(remote), None).is_err());
    }

    #[test]
    fn bearer_required_when_configured() {
        let config = AuthConfig {
            api_token: Some("s3cret".into()),
        };
        let local = "127.0.0.1:9".parse().unwrap();
        assert!(authorize_request(&config, "/api/sessions", Some(local), None).is_err());
        assert!(authorize_request(&config, "/api/sessions", Some(local), Some("wrong")).is_err());
        assert!(authorize_request(&config, "/api/sessions", Some(local), Some("s3cret")).is_ok());
    }

    #[test]
    fn extracts_bearer_and_query_token() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer abc".parse().unwrap());
        assert_eq!(extract_token(&headers, None).as_deref(), Some("abc"));

        headers.clear();
        headers.insert("x-evohime-token", "xyz".parse().unwrap());
        assert_eq!(extract_token(&headers, None).as_deref(), Some("xyz"));

        assert_eq!(
            extract_token(&axum::http::HeaderMap::new(), Some("access_token=tok%2Ben")).as_deref(),
            Some("tok+en")
        );
    }
}
