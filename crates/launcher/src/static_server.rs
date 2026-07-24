//! Встроенный статический файловый сервер для React-фронтенда (раздел II
//! плана): раздаёт `versions/<current>/dist/` вместо Vite dev-сервера —
//! Node.js/npm не требуются на машине пользователя.
//!
//! Инъецирует токен сессии в `index.html` "на лету" при каждой раздаче
//! (раздел XV плана), а не зашивает его в статический JS-бандл на диске —
//! иначе токен был бы читаем кем угодно, кто получит доступ к файловой
//! системе, даже после того как сессия Launcher'а завершится.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;

/// Плейсхолдер, который сборка `dist/index.html` должна содержать вместо
/// реального токена — заменяется этой строкой при каждой раздаче.
pub const TOKEN_PLACEHOLDER: &str = "__EVOHIME_TOKEN_PLACEHOLDER__";

#[derive(Clone)]
struct StaticServerState {
    dist_dir: PathBuf,
    session_token: Arc<str>,
}

/// Собирает роутер: `/` и `/index.html` проходят через инъекцию токена,
/// все прочие пути (JS/CSS/ассеты) отдаются как есть через `ServeDir`.
pub fn build_static_router(dist_dir: PathBuf, session_token: String) -> Router {
    let state = StaticServerState {
        dist_dir: dist_dir.clone(),
        session_token: session_token.into(),
    };

    Router::new()
        .route("/", get(serve_index))
        .route("/index.html", get(serve_index))
        .fallback_service(ServeDir::new(dist_dir))
        .with_state(state)
}

async fn serve_index(State(state): State<StaticServerState>) -> Response {
    let index_path = state.dist_dir.join("index.html");
    let raw = match tokio::fs::read_to_string(&index_path).await {
        Ok(contents) => contents,
        Err(_) => return (StatusCode::NOT_FOUND, "index.html not found").into_response(),
    };

    let injected = raw.replace(TOKEN_PLACEHOLDER, &state.session_token);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        // Раздел XV плана: nonce-based CSP было бы точнее для строгого
        // Content-Security-Policy, но этот сервер раздаёт только
        // статические ассеты локального однопользовательского приложения —
        // сознательно не отдаём CSP-заголовок здесь (в отличие от
        // основного server.exe, где CSP защищает от других угроз).
        .body(Body::from(injected))
        .expect("building a response from a known-valid header set cannot fail")
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

    #[tokio::test]
    async fn injects_token_into_index_html() {
        let dist_dir = tempfile::tempdir().unwrap();
        tokio::fs::write(
            dist_dir.path().join("index.html"),
            format!(
                "<html><script>window.__EVOHIME_TOKEN__ = \"{}\";</script></html>",
                TOKEN_PLACEHOLDER
            ),
        )
        .await
        .unwrap();

        let router = build_static_router(
            dist_dir.path().to_path_buf(),
            "real-secret-token".to_string(),
        );
        let base_url = spawn_router(router).await;

        let body = reqwest::get(&base_url).await.unwrap().text().await.unwrap();
        assert!(body.contains("real-secret-token"));
        assert!(!body.contains(TOKEN_PLACEHOLDER));
    }

    #[tokio::test]
    async fn serves_static_assets_unmodified() {
        let dist_dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dist_dir.path().join("index.html"), "<html></html>")
            .await
            .unwrap();
        tokio::fs::write(
            dist_dir.path().join("app.js"),
            format!("console.log('{TOKEN_PLACEHOLDER}');"), // should NOT be replaced here
        )
        .await
        .unwrap();

        let router = build_static_router(
            dist_dir.path().to_path_buf(),
            "real-secret-token".to_string(),
        );
        let base_url = spawn_router(router).await;

        let body = reqwest::get(format!("{base_url}/app.js"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            body.contains(TOKEN_PLACEHOLDER),
            "non-HTML assets must be served as-is, without token substitution"
        );
        assert!(!body.contains("real-secret-token"));
    }

    #[tokio::test]
    async fn different_paths_serving_index_both_get_token() {
        let dist_dir = tempfile::tempdir().unwrap();
        tokio::fs::write(
            dist_dir.path().join("index.html"),
            format!("<html>{TOKEN_PLACEHOLDER}</html>"),
        )
        .await
        .unwrap();

        let router = build_static_router(dist_dir.path().to_path_buf(), "tok-1".to_string());
        let base_url = spawn_router(router).await;

        let root_body = reqwest::get(&base_url).await.unwrap().text().await.unwrap();
        let explicit_body = reqwest::get(format!("{base_url}/index.html"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(root_body.contains("tok-1"));
        assert!(explicit_body.contains("tok-1"));
    }
}
