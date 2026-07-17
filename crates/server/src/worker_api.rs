//! Worker jobs HTTP API and background orchestration.
use crate::app::AppState;
use crate::rate_limit;
use crate::worker;
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(crate) struct WorkerJobRequest {
    task: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListWorkerJobsQuery {
    #[serde(default)]
    limit: Option<i64>,
}

pub(crate) async fn worker_status(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let counts = evohime_storage::count_worker_jobs_by_status(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let mut by_status = serde_json::Map::new();
    for (status, count) in counts {
        by_status.insert(status, json!(count));
    }
    Ok(Json(json!({
        "metrics": state.worker_metrics.snapshot(),
        "db_status_counts": by_status,
    })))
}

pub(crate) async fn list_worker_jobs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListWorkerJobsQuery>,
) -> Result<Json<Vec<evohime_storage::WorkerJobRow>>, ApiError> {
    let jobs = evohime_storage::list_recent_worker_jobs(&state.pool, query.limit.unwrap_or(50))
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(jobs))
}

pub(crate) async fn enforce_worker_job_limits(state: &AppState) -> Result<(), ApiError> {
    let counts = evohime_storage::count_worker_jobs_by_status(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let active = rate_limit::active_worker_job_count(&counts);
    state
        .rate_limiter
        .allow_worker_job(active)
        .map_err(|error| ApiError::TooManyRequests(error.message))
}

pub(crate) async fn create_worker_job(
    State(state): State<Arc<AppState>>,
    Json(request): Json<WorkerJobRequest>,
) -> Result<(StatusCode, Json<evohime_storage::WorkerJobRow>), ApiError> {
    if let Err(error) = worker::validate_task_payload(&request.task, &request.payload) {
        return Err(ApiError::BadRequest(error));
    }
    enforce_worker_job_limits(&state).await?;
    let row = evohime_storage::create_worker_job(&state.pool, &request.task, &request.payload)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let worker_job = match state.worker.submit(&request.task, &request.payload).await {
        Ok(job) => job,
        Err(error) => {
            let _ = evohime_storage::complete_worker_job(
                &state.pool,
                row.id,
                "failed",
                None,
                Some(&error.to_string()),
            )
            .await;
            return Err(ApiError::Unavailable(error.to_string()));
        }
    };
    let claim = evohime_storage::claim_worker_job_attempt(
        &state.pool,
        row.id,
        &worker_job.id,
        1,
        None,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::Conflict("worker job lease was taken by another poller".into()))?;
    state.worker_metrics.job_submitted(row.id, &request.task);
    spawn_worker_poll(state.clone(), row.id, worker_job, claim);
    let updated = evohime_storage::load_worker_job(&state.pool, row.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("worker job disappeared after submit".into()))?;
    Ok((StatusCode::ACCEPTED, Json(updated)))
}

pub(crate) async fn get_worker_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<evohime_storage::WorkerJobRow>, ApiError> {
    evohime_storage::load_worker_job(&state.pool, job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .map(Json)
        .ok_or_else(|| ApiError::BadRequest("worker job not found".into()))
}

pub(crate) async fn retry_worker_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<evohime_storage::WorkerJobRow>, ApiError> {
    let row = evohime_storage::load_worker_job(&state.pool, job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("worker job not found".into()))?;
    if row.attempts >= row.max_attempts {
        return Err(ApiError::BadRequest(
            "worker job retry limit has been reached".into(),
        ));
    }
    enforce_worker_job_limits(&state).await?;
    let worker_job = match state.worker.submit(&row.task, &row.payload_json).await {
        Ok(job) => job,
        Err(error) => {
            let _ = evohime_storage::complete_worker_job(
                &state.pool,
                row.id,
                "failed",
                None,
                Some(&error.to_string()),
            )
            .await;
            return Err(ApiError::Unavailable(error.to_string()));
        }
    };
    let claim = evohime_storage::force_claim_worker_job_attempt(
        &state.pool,
        row.id,
        &worker_job.id,
        row.attempts + 1,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| {
        ApiError::Conflict("worker job could not be reclaimed for retry (limit or race)".into())
    })?;
    state.worker_metrics.job_retried(row.id, &row.task, "manual retry");
    spawn_worker_poll(state.clone(), row.id, worker_job, claim);
    let updated = evohime_storage::load_worker_job(&state.pool, row.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("worker job disappeared after retry".into()))?;
    Ok(Json(updated))
}

pub(crate) fn spawn_worker_poll(
    state: Arc<AppState>,
    id: Uuid,
    worker_job: worker::WorkerJob,
    claim_token: Uuid,
) {
    tokio::spawn(async move {
        if let Err(error) = run_worker_job(&state, id, worker_job, claim_token).await {
            let _ = evohime_storage::complete_worker_job_claimed(
                &state.pool,
                id,
                claim_token,
                "failed",
                None,
                Some(&error),
            )
            .await;
            state.worker_metrics.job_finished(id, "unknown", false);
        }
    });
}

pub(crate) async fn recover_worker_jobs(state: Arc<AppState>) {
    let jobs = match evohime_storage::list_recoverable_worker_jobs(&state.pool).await {
        Ok(jobs) => jobs,
        Err(error) => {
            warn!(%error, "worker job recovery query failed");
            return;
        }
    };
    if !jobs.is_empty() {
        info!(
            count = jobs.len(),
            "recovering worker jobs after server restart"
        );
        state
            .worker_metrics
            .recovery(jobs.len(), "recoverable jobs");
    }
    for job in jobs {
        if job.attempts >= job.max_attempts {
            let _ = evohime_storage::complete_worker_job(
                &state.pool,
                job.id,
                "failed",
                None,
                Some("worker job exceeded retry limit during recovery"),
            )
            .await;
            continue;
        }
        let Some(claim) = (match evohime_storage::steal_worker_job_claim(&state.pool, job.id).await
        {
            Ok(claim) => claim,
            Err(error) => {
                warn!(%error, job_id = %job.id, "worker job steal failed");
                None
            }
        }) else {
            continue;
        };
        spawn_worker_recovery(state.clone(), job, claim);
    }
}

pub(crate) fn spawn_worker_recovery(
    state: Arc<AppState>,
    job: evohime_storage::WorkerJobRow,
    claim_token: Uuid,
) {
    tokio::spawn(async move {
        if let Ok(Some((worker_job, claim))) = retry_worker_job_after_error(
            &state,
            job.id,
            claim_token,
            "server restart recovery".to_string(),
        )
        .await
        {
            if let Err(error) = run_worker_job(&state, job.id, worker_job, claim).await {
                let _ = evohime_storage::complete_worker_job_claimed(
                    &state.pool,
                    job.id,
                    claim,
                    "failed",
                    None,
                    Some(&error),
                )
                .await;
            }
        }
    });
}

pub(crate) async fn run_worker_job(
    state: &AppState,
    id: Uuid,
    mut worker_job: worker::WorkerJob,
    mut claim_token: Uuid,
) -> Result<(), String> {
    let task_name = evohime_storage::load_worker_job(&state.pool, id)
        .await
        .ok()
        .flatten()
        .map(|row| row.task)
        .unwrap_or_else(|| "unknown".into());
    loop {
        for _ in 0..120 {
            if worker::is_terminal_status(&worker_job.status) {
                let ok = worker_job.status == "completed";
                match evohime_storage::complete_worker_job_claimed(
                    &state.pool,
                    id,
                    claim_token,
                    &worker_job.status,
                    worker_job.result.as_ref(),
                    worker_job.error.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    Some(_) => {
                        state.worker_metrics.job_finished(id, &task_name, ok);
                    }
                    None => {
                        tracing::debug!(
                            job_id = %id,
                            "stale worker poller lost claim; ignoring terminal result"
                        );
                    }
                }
                return Ok(());
            }
            if worker_job.status == "running"
                && worker::heartbeat_is_stale(
                    worker_job.heartbeat_at.as_deref(),
                    chrono::Utc::now(),
                    state.worker_job_stall,
                )
            {
                state.worker_metrics.job_stalled(id, &task_name);
                match retry_worker_job_after_error(
                    state,
                    id,
                    claim_token,
                    "worker job heartbeat stalled".to_string(),
                )
                .await?
                {
                    Some((job, new_claim)) => {
                        worker_job = job;
                        claim_token = new_claim;
                        continue;
                    }
                    None => {
                        state.worker_metrics.job_finished(id, &task_name, false);
                        return Ok(());
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            match state.worker.get(&worker_job.id).await {
                Ok(job) => worker_job = job,
                Err(error) => {
                    match retry_worker_job_after_error(state, id, claim_token, error.to_string())
                        .await?
                    {
                        Some((job, new_claim)) => {
                            worker_job = job;
                            claim_token = new_claim;
                        }
                        None => {
                            state.worker_metrics.job_finished(id, &task_name, false);
                            return Ok(());
                        }
                    }
                }
            }
        }
        match retry_worker_job_after_error(
            state,
            id,
            claim_token,
            "worker polling timed out".to_string(),
        )
        .await?
        {
            Some((job, new_claim)) => {
                worker_job = job;
                claim_token = new_claim;
            }
            None => {
                state.worker_metrics.job_finished(id, &task_name, false);
                return Ok(());
            }
        }
    }
}

pub(crate) async fn worker_health_loop(state: Arc<AppState>, interval: Duration, stale_after: Duration) {
    let mut ticker = tokio::time::interval(interval);
    let mut last_started_at: Option<String> = None;
    let mut last_ok_at = tokio::time::Instant::now();
    let mut recovery_inflight = false;
    loop {
        ticker.tick().await;
        match state.worker.health().await {
            Ok(health) => {
                let restarted = last_started_at
                    .as_ref()
                    .is_some_and(|previous| previous != &health.started_at);
                state.worker_metrics.health_ok(
                    health.started_at.clone(),
                    health.pid,
                    health.queue_depth,
                    health.active_jobs,
                );
                last_started_at = Some(health.started_at);
                last_ok_at = tokio::time::Instant::now();
                recovery_inflight = false;
                if restarted {
                    info!("python worker restarted; recovering durable jobs");
                    recover_worker_jobs(state.clone()).await;
                }
            }
            Err(error) => {
                warn!(%error, "python worker health check failed");
                state.worker_metrics.health_failed(&error.to_string());
                if !recovery_inflight && last_ok_at.elapsed() >= stale_after {
                    recovery_inflight = true;
                    warn!(
                        stale_secs = stale_after.as_secs(),
                        "python worker unhealthy; recovering durable jobs"
                    );
                    recover_worker_jobs(state.clone()).await;
                }
            }
        }
    }
}

pub(crate) async fn retry_worker_job_after_error(
    state: &AppState,
    id: Uuid,
    claim_token: Uuid,
    error: String,
) -> Result<Option<(worker::WorkerJob, Uuid)>, String> {
    let row = evohime_storage::load_worker_job(&state.pool, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "worker job disappeared during retry".to_string())?;
    if row.claim_token != Some(claim_token) {
        tracing::debug!(job_id = %id, "lost worker job claim before retry");
        return Ok(None);
    }
    if row.attempts >= row.max_attempts {
        let _ = evohime_storage::complete_worker_job_claimed(
            &state.pool,
            id,
            claim_token,
            "failed",
            None,
            Some(&error),
        )
        .await
        .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let mut attempts = row.attempts;
    loop {
        tokio::time::sleep(worker::retry_delay(attempts)).await;
        let row = evohime_storage::load_worker_job(&state.pool, id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "worker job disappeared during retry".to_string())?;
        if row.claim_token != Some(claim_token) {
            tracing::debug!(job_id = %id, "lost worker job claim while waiting to retry");
            return Ok(None);
        }
        match state.worker.submit(&row.task, &row.payload_json).await {
            Ok(worker_job) => {
                match evohime_storage::claim_worker_job_attempt(
                    &state.pool,
                    id,
                    &worker_job.id,
                    attempts + 1,
                    Some(claim_token),
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    Some(new_claim) => {
                        state.worker_metrics.job_retried(id, &row.task, &error);
                        return Ok(Some((worker_job, new_claim)));
                    }
                    None => {
                        tracing::debug!(
                            job_id = %id,
                            "worker job claim raced during retry; abandoning orphan python job"
                        );
                        return Ok(None);
                    }
                }
            }
            Err(submit_error) if attempts + 1 >= row.max_attempts => {
                let _ = evohime_storage::complete_worker_job_claimed(
                    &state.pool,
                    id,
                    claim_token,
                    "failed",
                    None,
                    Some(&submit_error.to_string()),
                )
                .await
                .map_err(|e| e.to_string())?;
                return Ok(None);
            }
            Err(_) => attempts += 1,
        }
    }
}

pub(crate) async fn worker_retention_loop(state: Arc<AppState>, retention_days: i64) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
        match evohime_storage::prune_worker_jobs(&state.pool, cutoff).await {
            Ok(count) if count > 0 => info!(count, retention_days, "pruned old worker jobs"),
            Ok(_) => {}
            Err(error) => warn!(%error, "worker job retention cleanup failed"),
        }
    }
}

