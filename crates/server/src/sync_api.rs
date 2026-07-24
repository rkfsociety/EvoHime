//! Cloud sync push API (Stage 7.99, wave 1).
//!
//! Owner-only push of the operator's `BackupDump` to a trusted remote
//! endpoint configured via environment, with run history in `sync_runs`.

use crate::app::AppState;
use crate::auth::{require_owner, OperatorIdentity};
use crate::ApiError;
use axum::extract::{Extension, State};
use axum::Json;
use chrono::Duration;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use url::Url;

const SYNC_URL_ENV: &str = "EVOHIME_SYNC_URL";
const SYNC_TOKEN_ENV: &str = "EVOHIME_SYNC_TOKEN";
const CHECKSUM_HEADER: &str = "x-evohime-backup-checksum";
const PUSH_TIMEOUT_SECS: u64 = 30;
const ACTIVE_RUN_STALE_MINUTES: i64 = 10;
const REMOTE_ERROR_MAX_CHARS: usize = 512;
const STATUS_RUNS_LIMIT: i64 = 20;

#[derive(Debug, Clone)]
pub(crate) struct SyncConfig {
    pub url: Url,
    pub token: Option<String>,
}

impl SyncConfig {
    fn from_env() -> Result<Option<Self>, String> {
        let Some(raw) = std::env::var(SYNC_URL_ENV).ok().filter(|v| !v.trim().is_empty())
        else {
            return Ok(None);
        };
        let url = validate_sync_url(raw.trim())?;
        let token = std::env::var(SYNC_TOKEN_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty());
        Ok(Some(Self { url, token }))
    }
}

pub(crate) fn validate_sync_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "sync URL не разбирается".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("sync URL должен использовать http или https".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("sync URL не должен содержать credentials".into());
    }
    if url.host_str().is_none() {
        return Err("sync URL должен содержать host".into());
    }
    Ok(url)
}

/// Host shown in status responses; never includes credentials, path or query.
pub(crate) fn redacted_remote_host(url: &Url) -> String {
    match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    }
}

pub(crate) fn checksum_hex(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub(crate) fn truncate_remote_error(message: &str) -> String {
    if message.chars().count() <= REMOTE_ERROR_MAX_CHARS {
        return message.to_string();
    }
    let truncated: String = message.chars().take(REMOTE_ERROR_MAX_CHARS).collect();
    format!("{truncated}…")
}

fn feature_enabled() -> bool {
    crate::features::enabled("EVOHIME_FEATURE_CLOUD_SYNC", true)
}

fn require_feature() -> Result<(), ApiError> {
    if feature_enabled() {
        Ok(())
    } else {
        Err(ApiError::NotFound("cloud sync отключён feature flag".into()))
    }
}

fn require_sync_owner(identity: &OperatorIdentity) -> Result<(), ApiError> {
    require_owner(identity).map_err(|_| ApiError::Forbidden("требуется роль owner".into()))
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncStatusResponse {
    pub feature_enabled: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_error: Option<String>,
    pub runs: Vec<evohime_storage::SyncRunRow>,
}

pub(crate) async fn status(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SyncStatusResponse>, ApiError> {
    require_feature()?;
    require_sync_owner(&identity)?;
    let (configured, remote_host, config_error) = match SyncConfig::from_env() {
        Ok(Some(config)) => (true, Some(redacted_remote_host(&config.url)), None),
        Ok(None) => (false, None, None),
        Err(error) => (false, None, Some(error)),
    };
    let runs = evohime_storage::list_sync_runs(&state.pool, identity.id, STATUS_RUNS_LIMIT)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(SyncStatusResponse {
        feature_enabled: true,
        configured,
        remote_host,
        config_error,
        runs,
    }))
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncPushResponse {
    pub run: evohime_storage::SyncRunRow,
}

pub(crate) async fn push(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SyncPushResponse>, ApiError> {
    require_feature()?;
    require_sync_owner(&identity)?;
    let config = match SyncConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Err(ApiError::Unavailable(format!(
                "cloud sync не сконфигурирован: задайте {SYNC_URL_ENV}"
            )))
        }
        Err(error) => return Err(ApiError::Unavailable(format!("cloud sync config: {error}"))),
    };

    let stale_after = Duration::minutes(ACTIVE_RUN_STALE_MINUTES);
    if let Some(active) =
        evohime_storage::find_active_sync_run(&state.pool, identity.id, stale_after)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        return Err(ApiError::Conflict(format!(
            "push уже выполняется (run {})",
            active.id
        )));
    }

    let dump = evohime_storage::collect_backup(&state.pool, identity.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let payload =
        serde_json::to_vec(&dump).map_err(|error| ApiError::Internal(error.to_string()))?;
    let checksum = checksum_hex(&payload);
    let bytes_total = payload.len() as i64;

    let run = evohime_storage::start_sync_run(&state.pool, identity.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let outcome = send_backup(&config, payload, &checksum).await;
    let (status, error) = match &outcome {
        Ok(()) => (evohime_storage::SYNC_STATUS_SUCCESS, None),
        Err(message) => (evohime_storage::SYNC_STATUS_FAILED, Some(message.as_str())),
    };
    let finished = evohime_storage::finish_sync_run(
        &state.pool,
        run.id,
        status,
        Some(bytes_total),
        Some(&checksum),
        error,
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?
    .unwrap_or(run);

    tracing::info!(
        run_id = %finished.id,
        status = %finished.status,
        bytes_total,
        "cloud sync push finished"
    );
    Ok(Json(SyncPushResponse { run: finished }))
}

async fn send_backup(config: &SyncConfig, payload: Vec<u8>, checksum: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(PUSH_TIMEOUT_SECS))
        .build()
        .map_err(|error| truncate_remote_error(&error.to_string()))?;
    let mut request = client
        .put(config.url.clone())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(CHECKSUM_HEADER, checksum)
        .body(payload);
    if let Some(token) = &config.token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| truncate_remote_error(&error.without_url().to_string()))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(truncate_remote_error(&format!(
            "remote вернул статус {}",
            response.status()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_url_accepts_only_clean_http_endpoints() {
        assert!(validate_sync_url("https://backup.example.com/ingest").is_ok());
        assert!(validate_sync_url("http://10.0.0.5:8080/dump").is_ok());
        assert!(validate_sync_url("ftp://backup.example.com/ingest").is_err());
        assert!(validate_sync_url("https://user:pass@backup.example.com/x").is_err());
        assert!(validate_sync_url("not a url").is_err());
        assert!(validate_sync_url("file:///C:/dump.json").is_err());
    }

    #[test]
    fn redacted_host_drops_path_query_and_credentials() {
        let url = validate_sync_url("https://backup.example.com:8443/ingest?tenant=1").unwrap();
        assert_eq!(redacted_remote_host(&url), "backup.example.com:8443");
        let url = validate_sync_url("https://backup.example.com/ingest").unwrap();
        assert_eq!(redacted_remote_host(&url), "backup.example.com");
    }

    #[test]
    fn checksum_is_lowercase_sha256_hex() {
        let checksum = checksum_hex(b"evohime");
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(checksum, checksum_hex(b"evohime"));
        assert_ne!(checksum, checksum_hex(b"evohime2"));
    }

    #[test]
    fn remote_errors_are_truncated() {
        let short = "remote unavailable";
        assert_eq!(truncate_remote_error(short), short);
        let long = "x".repeat(2000);
        let truncated = truncate_remote_error(&long);
        assert!(truncated.chars().count() <= REMOTE_ERROR_MAX_CHARS + 1);
        assert!(truncated.ends_with('…'));
    }
}
