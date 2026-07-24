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
use uuid::Uuid;

const SYNC_URL_ENV: &str = "EVOHIME_SYNC_URL";
const SYNC_TOKEN_ENV: &str = "EVOHIME_SYNC_TOKEN";
const SYNC_AUTO_MINUTES_ENV: &str = "EVOHIME_SYNC_AUTO_MINUTES";
const MIN_AUTO_SYNC_MINUTES: u64 = 5;
const CHECKSUM_HEADER: &str = "x-evohime-backup-checksum";
const PUSH_TIMEOUT_SECS: u64 = 30;
const PULL_TIMEOUT_SECS: u64 = 60;
const PULL_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
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
    pub auto_minutes: Option<u64>,
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
        auto_minutes: auto_sync_minutes(),
        runs,
    }))
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncPushResponse {
    pub run: evohime_storage::SyncRunRow,
}

async fn guarded_sync_config(
    state: &AppState,
    identity: &OperatorIdentity,
) -> Result<SyncConfig, ApiError> {
    require_feature()?;
    require_sync_owner(identity)?;
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
            "sync уже выполняется (run {})",
            active.id
        )));
    }
    Ok(config)
}

pub(crate) async fn push(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SyncPushResponse>, ApiError> {
    let config = guarded_sync_config(&state, &identity).await?;
    let finished = perform_push(&state, identity.id, &config).await?;
    Ok(Json(SyncPushResponse { run: finished }))
}

pub(crate) async fn perform_push(
    state: &AppState,
    operator_id: Uuid,
    config: &SyncConfig,
) -> Result<evohime_storage::SyncRunRow, ApiError> {
    let dump = evohime_storage::collect_backup(&state.pool, operator_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let payload =
        serde_json::to_vec(&dump).map_err(|error| ApiError::Internal(error.to_string()))?;
    let checksum = checksum_hex(&payload);
    let bytes_total = payload.len() as i64;

    let run = evohime_storage::start_sync_run(
        &state.pool,
        operator_id,
        evohime_storage::SYNC_DIRECTION_PUSH,
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    let outcome = send_backup(config, payload, &checksum).await;
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
    Ok(finished)
}

/// Auto-sync period from `EVOHIME_SYNC_AUTO_MINUTES`; `None` disables the loop.
/// Values below 5 minutes are raised to 5 so a config typo cannot hammer the receiver.
pub(crate) fn auto_sync_minutes() -> Option<u64> {
    parse_auto_sync_minutes(std::env::var(SYNC_AUTO_MINUTES_ENV).ok().as_deref())
}

pub(crate) fn parse_auto_sync_minutes(raw: Option<&str>) -> Option<u64> {
    match raw?.trim().parse::<u64>() {
        Ok(0) | Err(_) => None,
        Ok(minutes) => Some(minutes.max(MIN_AUTO_SYNC_MINUTES)),
    }
}

/// Background loop: pushes the bootstrap owner's backup every `interval`.
/// The first tick is skipped so restart loops cannot stampede the receiver.
pub(crate) async fn auto_sync_loop(state: Arc<AppState>, interval: std::time::Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if !feature_enabled() {
            continue;
        }
        let config = match SyncConfig::from_env() {
            Ok(Some(config)) => config,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(%error, "auto sync skipped: invalid sync config");
                continue;
            }
        };
        let operator_id = evohime_storage::BOOTSTRAP_OWNER_ID;
        match evohime_storage::find_active_sync_run(
            &state.pool,
            operator_id,
            Duration::minutes(ACTIVE_RUN_STALE_MINUTES),
        )
        .await
        {
            Ok(Some(active)) => {
                tracing::info!(run_id = %active.id, "auto sync skipped: run already active");
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "auto sync skipped: active run check failed");
                continue;
            }
        }
        match perform_push(&state, operator_id, &config).await {
            Ok(run) => {
                tracing::info!(run_id = %run.id, status = %run.status, "auto sync push recorded");
            }
            Err(error) => {
                tracing::warn!(%error, "auto sync push failed");
            }
        }
    }
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

#[derive(Debug, Serialize)]
pub(crate) struct SyncPullResponse {
    pub run: evohime_storage::SyncRunRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<evohime_storage::RestoreReport>,
}

pub(crate) async fn pull(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SyncPullResponse>, ApiError> {
    let config = guarded_sync_config(&state, &identity).await?;

    let run = evohime_storage::start_sync_run(
        &state.pool,
        identity.id,
        evohime_storage::SYNC_DIRECTION_PULL,
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    let outcome = pull_and_restore(&state, identity.id, &config).await;
    let (status, bytes_total, checksum, error, report) = match &outcome {
        Ok((bytes_total, checksum, report)) => (
            evohime_storage::SYNC_STATUS_SUCCESS,
            Some(*bytes_total),
            Some(checksum.as_str()),
            None,
            Some(*report),
        ),
        Err(message) => (
            evohime_storage::SYNC_STATUS_FAILED,
            None,
            None,
            Some(message.as_str()),
            None,
        ),
    };
    let finished = evohime_storage::finish_sync_run(
        &state.pool,
        run.id,
        status,
        bytes_total,
        checksum,
        error,
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?
    .unwrap_or(run);

    tracing::info!(
        run_id = %finished.id,
        status = %finished.status,
        "cloud sync pull finished"
    );
    Ok(Json(SyncPullResponse { run: finished, report }))
}

async fn pull_and_restore(
    state: &AppState,
    operator_id: Uuid,
    config: &SyncConfig,
) -> Result<(i64, String, evohime_storage::RestoreReport), String> {
    let (payload, remote_checksum) = fetch_backup(config).await?;
    let checksum = checksum_hex(&payload);
    verify_remote_checksum(remote_checksum.as_deref(), &checksum)?;

    let dump: evohime_storage::BackupDump = serde_json::from_slice(&payload)
        .map_err(|error| truncate_remote_error(&format!("backup parse: {error}")))?;
    let report = evohime_storage::restore_backup(&state.pool, operator_id, &dump)
        .await
        .map_err(|error| truncate_remote_error(&format!("restore: {error}")))?;
    Ok((payload.len() as i64, checksum, report))
}

pub(crate) fn verify_remote_checksum(remote: Option<&str>, actual: &str) -> Result<(), String> {
    match remote {
        Some(remote) if !remote.trim().is_empty() && !remote.trim().eq_ignore_ascii_case(actual) => {
            Err("checksum remote-дампа не совпадает с телом ответа".into())
        }
        _ => Ok(()),
    }
}

pub(crate) fn ensure_body_within_limit(len: usize, limit: usize) -> Result<(), String> {
    if len > limit {
        return Err(format!(
            "remote-дамп превышает лимит {} MiB",
            limit / (1024 * 1024)
        ));
    }
    Ok(())
}

async fn fetch_backup(config: &SyncConfig) -> Result<(Vec<u8>, Option<String>), String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(PULL_TIMEOUT_SECS))
        .build()
        .map_err(|error| truncate_remote_error(&error.to_string()))?;
    let mut request = client.get(config.url.clone());
    if let Some(token) = &config.token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| truncate_remote_error(&error.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(truncate_remote_error(&format!(
            "remote вернул статус {}",
            response.status()
        )));
    }
    if let Some(length) = response.content_length() {
        ensure_body_within_limit(length as usize, PULL_BODY_LIMIT_BYTES)?;
    }
    let remote_checksum = response
        .headers()
        .get(CHECKSUM_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .bytes()
        .await
        .map_err(|error| truncate_remote_error(&error.without_url().to_string()))?;
    ensure_body_within_limit(body.len(), PULL_BODY_LIMIT_BYTES)?;
    Ok((body.to_vec(), remote_checksum))
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
    fn auto_sync_minutes_parses_and_clamps() {
        assert_eq!(parse_auto_sync_minutes(None), None);
        assert_eq!(parse_auto_sync_minutes(Some("")), None);
        assert_eq!(parse_auto_sync_minutes(Some("0")), None);
        assert_eq!(parse_auto_sync_minutes(Some("garbage")), None);
        assert_eq!(parse_auto_sync_minutes(Some("-5")), None);
        assert_eq!(parse_auto_sync_minutes(Some("3")), Some(MIN_AUTO_SYNC_MINUTES));
        assert_eq!(parse_auto_sync_minutes(Some("5")), Some(5));
        assert_eq!(parse_auto_sync_minutes(Some("30")), Some(30));
    }

    #[test]
    fn pull_body_limit_is_enforced() {
        assert!(ensure_body_within_limit(1024, PULL_BODY_LIMIT_BYTES).is_ok());
        assert!(ensure_body_within_limit(PULL_BODY_LIMIT_BYTES, PULL_BODY_LIMIT_BYTES).is_ok());
        assert!(ensure_body_within_limit(PULL_BODY_LIMIT_BYTES + 1, PULL_BODY_LIMIT_BYTES).is_err());
    }

    #[test]
    fn remote_checksum_must_match_when_present() {
        let actual = checksum_hex(b"dump");
        assert!(verify_remote_checksum(None, &actual).is_ok());
        assert!(verify_remote_checksum(Some(""), &actual).is_ok());
        assert!(verify_remote_checksum(Some(&actual), &actual).is_ok());
        assert!(verify_remote_checksum(Some(&actual.to_uppercase()), &actual).is_ok());
        assert!(verify_remote_checksum(Some("deadbeef"), &actual).is_err());
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
