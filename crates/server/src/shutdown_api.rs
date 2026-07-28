//! Graceful shutdown endpoint для Launcher'а (раздел IV плана
//! Installer/Launcher/Update): Windows не имеет надёжного эквивалента
//! POSIX-сигналов для дочернего процесса без общей консоли с родителем —
//! `CTRL_C_EVENT` нестабилен в этом сценарии. Launcher вместо этого
//! останавливает компоненты через `POST /shutdown`.
//!
//! Эндпоинт защищён отдельным секретом (`EVOHIME_LOCAL_TOKEN`,
//! `AppState::local_shutdown_secret`) — не общим `EVOHIME_API_TOKEN`.
//! Секрет генерируется Launcher'ом заново при каждом запуске (раздел XV
//! плана: локальные HTTP-эндпоинты не значит "доверенные" — любой сайт в
//! браузере может дёрнуть `127.0.0.1`). Если секрет не сконфигурирован
//! (сервер запущен не через Launcher, например через `start-dev.ps1`),
//! эндпоинт полностью отключён — `404`, а не "принимаем без токена".

use crate::app::AppState;
use crate::auth::{extract_header_token, tokens_equal};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use std::sync::Arc;
use tracing::info;

pub(crate) async fn shutdown(State(state): State<Arc<AppState>>, headers: HeaderMap) -> StatusCode {
    let Some(expected) = state.local_shutdown_secret.as_deref() else {
        return StatusCode::NOT_FOUND;
    };

    match extract_header_token(&headers) {
        Some(presented) if tokens_equal(expected, &presented) => {
            info!("shutdown requested via authenticated /shutdown endpoint");
            state.shutdown_token.cancel();
            StatusCode::OK
        }
        _ => StatusCode::UNAUTHORIZED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_bearer(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        headers
    }

    #[test]
    fn tokens_equal_rejects_mismatch_used_by_shutdown() {
        // Проверяем именно тот компаратор, который использует shutdown-
        // эндпоинт — регрессия здесь означала бы, что чужой токен проходит.
        assert!(!tokens_equal("expected-secret", "wrong-secret"));
        assert!(tokens_equal("expected-secret", "expected-secret"));
    }

    #[test]
    fn extract_header_token_reads_bearer_header() {
        let headers = headers_with_bearer("abc123");
        assert_eq!(extract_header_token(&headers).as_deref(), Some("abc123"));
    }
}
