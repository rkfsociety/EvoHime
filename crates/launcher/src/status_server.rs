//! REST-эндпоинт статуса Launcher'а (раздел VII плана): простой HTTP
//! polling вместо WebSocket — окно управления и React-панель основного
//! приложения опрашивают этот эндпоинт вместо подписки на отдельный
//! протокол. Защищён токеном сессии (раздел XV плана) тем же способом, что
//! и static file server, и `/shutdown` в `evohime-server`.

use crate::token::tokens_equal;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentStatus {
    pub name: String,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LauncherStatus {
    pub components: Vec<ComponentStatus>,
    pub update_available: bool,
}

/// Поставщик текущего статуса — развязывает HTTP-слой от реального
/// менеджера процессов, который заполняется по мере расширения Launcher'а.
pub type StatusProvider = Arc<dyn Fn() -> LauncherStatus + Send + Sync>;

#[derive(Clone)]
pub struct StatusServerState {
    pub session_token: Arc<str>,
    pub status_provider: StatusProvider,
}

pub fn build_status_router(state: StatusServerState) -> Router {
    Router::new()
        .route("/status", get(get_status))
        .with_state(state)
}

async fn get_status(
    State(state): State<StatusServerState>,
    headers: HeaderMap,
) -> Result<Json<LauncherStatus>, StatusCode> {
    let presented = extract_bearer_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !tokens_equal(&state.session_token, &presented) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json((state.status_provider)()))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    async fn spawn_router(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn fixed_status_router(token: &str) -> Router {
        let state = StatusServerState {
            session_token: token.into(),
            status_provider: Arc::new(|| LauncherStatus {
                components: vec![ComponentStatus {
                    name: "server".to_string(),
                    online: true,
                }],
                update_available: false,
            }),
        };
        build_status_router(state)
    }

    #[tokio::test]
    async fn rejects_request_without_token() {
        let base_url = spawn_router(fixed_status_router("secret")).await;
        let response = reqwest::get(format!("{base_url}/status")).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_wrong_token() {
        let base_url = spawn_router(fixed_status_router("secret")).await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{base_url}/status"))
            .bearer_auth("wrong-token")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn accepts_correct_token_and_returns_status() {
        let base_url = spawn_router(fixed_status_router("secret")).await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{base_url}/status"))
            .bearer_auth("secret")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let body: LauncherStatus = response.json().await.unwrap();
        assert_eq!(body.components.len(), 1);
        assert_eq!(body.components[0].name, "server");
        assert!(body.components[0].online);
        assert!(!body.update_available);
    }
}
