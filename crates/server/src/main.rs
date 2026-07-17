mod app;
mod auth;
mod cors;
mod memory_api;
mod observability;
mod otel;
mod plugins;
mod rate_limit;
mod worker;
mod worker_observability;
mod workspace;

use anyhow::Context;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use evohime_agent_runtime::{
    run_agent_loop, run_agent_loop_resumed, AgentConfig, AgentError, AgentResumeContext,
};
use evohime_model_gateway::providers::{ChatMessage, ChatRole, ProviderKind};
use evohime_model_gateway::{ModelGateway, ModelRouteConfig};
use evohime_permissions::{Permission, PermissionMode};
use evohime_protocol::{ClientCommand, HistoryItem, PlanStep, ServerEvent, SessionBootstrap};
use evohime_task_engine::{
    complete_task, fail_task, pause_task, resume_task, retry_task, start_task,
};
use evohime_tool_runtime::ToolError;
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use sqlx::PgPool;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::app::{AppConfig, AppState, McpServerConfig};

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("approval required for {tool}: {approval_id}")]
    ApprovalRequired { tool: String, approval_id: Uuid },
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{0}")]
    Internal(String),
    #[error("service unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ModelSettingsRequest {
    default_route: String,
    routes: Vec<ModelRouteRequest>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ModelRouteRequest {
    name: String,
    provider: String,
    model: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default = "default_billing_mode")]
    billing_mode: String,
}

fn default_billing_mode() -> String {
    "free".to_string()
}

impl From<(Uuid, ApiError)> for ApiError {
    fn from((_, error): (Uuid, ApiError)) -> Self {
        error
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::ApprovalRequired { tool, approval_id } => (
                StatusCode::CONFLICT,
                format!("approval required for {tool}: {approval_id}"),
            ),
            Self::TooManyRequests(message) => (StatusCode::TOO_MANY_REQUESTS, message),
            Self::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
            Self::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = otel::init_tracing()?;

    let config = AppConfig::from_env()?;
    let pool_config = evohime_storage::PoolConfig::from_env();
    info!(
        max_connections = pool_config.max_connections,
        min_connections = pool_config.min_connections,
        acquire_timeout_secs = pool_config.acquire_timeout.as_secs(),
        idle_timeout_secs = ?pool_config.idle_timeout.map(|d| d.as_secs()),
        max_lifetime_secs = ?pool_config.max_lifetime.map(|d| d.as_secs()),
        "postgres pool configured"
    );
    let pool = evohime_storage::connect_pool(&config.database_url, &pool_config)
        .await
        .context("connect to postgres")?;

    evohime_storage::run_migrations(&pool)
        .await
        .context("run migrations")?;

    let active_model_config = match evohime_storage::load_setting(&pool, "model_config").await? {
        Some(value) => match serde_json::from_value::<ModelSettingsRequest>(value) {
            Ok(request) => build_model_config(request, &config.model_config).unwrap_or_else(|error| {
                warn!(error = %error, "stored model settings are invalid; using environment defaults");
                config.model_config.clone()
            }),
            Err(error) => {
                warn!(error = %error, "stored model settings could not be read; using environment defaults");
                config.model_config.clone()
            }
        },
        None => config.model_config.clone(),
    };
    let default_route_name = active_model_config.default_route.clone();
    let default_route_config = active_model_config.routes.get(&default_route_name).cloned();
    let default_model_name = default_route_config
        .as_ref()
        .map(|route| route.literouter.model.clone())
        .unwrap_or_default();
    let default_provider_name = default_route_config
        .as_ref()
        .map(|route| route.provider.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let model_gateway = match default_route_config {
        Some(ref route) if !route.configured() => {
            warn!(
                "default model route is not configured — LLM requests will fail until a key is set"
            );
            None
        }
        Some(_) => Some(Arc::new(
            ModelGateway::from_config(&active_model_config).context("init model gateway")?,
        )),
        None => {
            warn!("default model route '{}' is missing", default_route_name);
            None
        }
    };

    let permissions = evohime_permissions::PermissionEngine::new();
    let state = Arc::new(AppState {
        pool,
        workspace_root: config.workspace_root.clone(),
        auth: config.auth.clone(),
        tools: evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions.clone()),
        permissions,
        model_gateway: Arc::new(RwLock::new(model_gateway)),
        model_config: Arc::new(RwLock::new(active_model_config)),
        mcp_servers: Arc::new(Mutex::new(config.mcp_servers.clone())),
        session_buses: Arc::new(Mutex::new(HashMap::new())),
        task_cancellations: Arc::new(Mutex::new(HashMap::new())),
        worker: worker::WorkerClient::new(config.worker_url.clone())?,
        worker_job_stall: config.worker_job_stall,
        plugin_catalog_cache: plugins::PluginCatalogCache::default(),
        metrics: Arc::new(observability::PipelineMetrics::new()),
        worker_metrics: Arc::new(worker_observability::WorkerMetrics::new()),
        rate_limiter: Arc::new(rate_limit::RateLimiter::from_env()),
    });

    let retention_state = state.clone();
    let retention_days = config.worker_retention_days;
    tokio::spawn(async move {
        worker_retention_loop(retention_state, retention_days).await;
    });
    let health_state = state.clone();
    let health_interval = config.worker_health_interval;
    let health_stale = config.worker_health_stale;
    tokio::spawn(async move {
        worker_health_loop(health_state, health_interval, health_stale).await;
    });
    recover_worker_jobs(state.clone()).await;
    if let Some(value) = evohime_storage::load_setting(&state.pool, "permissions").await? {
        if let Some(settings) = value.as_object() {
            for (name, mode) in settings {
                if let (Some(permission), Ok(mode)) = (
                    parse_permission_name(name),
                    serde_json::from_value::<PermissionMode>(mode.clone()),
                ) {
                    state.permissions.set_mode(permission, mode).await;
                }
            }
        }
    }
    if let Some(value) = evohime_storage::load_setting(&state.pool, "permission_scopes").await? {
        match serde_json::from_value::<evohime_permissions::PermissionScopesSnapshot>(value) {
            Ok(snapshot) => {
                let sessions = snapshot.session_overrides.len();
                let grants = snapshot.path_grants.len();
                state.permissions.import_scopes(snapshot).await;
                info!(sessions, grants, "restored permission session/path scopes");
            }
            Err(error) => {
                warn!(error = %error, "stored permission_scopes could not be read; ignoring");
            }
        }
    }
    if let Some(value) = evohime_storage::load_setting(&state.pool, "mcp_servers").await? {
        if let Ok(servers) = serde_json::from_value::<Vec<McpServerConfig>>(value) {
            *state.mcp_servers.lock().await = servers;
        }
    }

    let recovered = evohime_task_engine::recover_after_restart(&state.pool)
        .await
        .context("recover tasks after restart")?;
    let resume_policy = evohime_task_engine::RestartResumePolicy::from_env();
    if resume_policy.auto_resume_mutating {
        info!("EVOHIME_AUTO_RESUME_ON_RESTART enabled: mutating tasks may auto-resume");
    }
    if !recovered.is_empty() {
        info!(count = recovered.len(), "tasks paused after crash (were running/cancelling)");
        for task in recovered {
            let task_id = task.id;
            let session_id = task.session_id;
            let mutating = evohime_task_engine::task_has_mutating_work(&state.pool, task_id)
                .await
                .unwrap_or(true);
            let _ = evohime_storage::merge_checkpoint(
                &state.pool,
                task_id,
                None,
                &json!({
                    "pause_reason": "server_restart",
                    "mutating": mutating,
                }),
            )
            .await;

            if !evohime_task_engine::should_auto_resume_after_restart(resume_policy, mutating) {
                warn!(
                    %task_id,
                    "deferring auto-resume for mutating task; set EVOHIME_AUTO_RESUME_ON_RESTART=1 to override"
                );
                emit_event(
                    &state,
                    session_id,
                    Some(task_id),
                    ServerEvent::TaskStatusChanged {
                        task_id,
                        status: "paused".to_string(),
                    },
                )
                .await
                .map_err(|(_, error)| error)?;
                emit_event(
                    &state,
                    session_id,
                    Some(task_id),
                    ServerEvent::ActionLogged {
                        task_id,
                        action: "task.recovery_deferred".to_string(),
                        detail: "Mutating task left paused after server restart; resume manually or set EVOHIME_AUTO_RESUME_ON_RESTART=1".to_string(),
                        created_at: chrono::Utc::now(),
                    },
                )
                .await
                .map_err(|(_, error)| error)?;
                continue;
            }

            let _ = resume_task(&state.pool, task_id).await;
            emit_event(
                &state,
                session_id,
                Some(task_id),
                ServerEvent::TaskStatusChanged {
                    task_id,
                    status: "running".to_string(),
                },
            )
            .await
            .map_err(|(_, error)| error)?;
            emit_event(
                &state,
                session_id,
                Some(task_id),
                ServerEvent::ActionLogged {
                    task_id,
                    action: "task.recovered".to_string(),
                    detail: if mutating {
                        "Mutating task auto-resumed after server restart (EVOHIME_AUTO_RESUME_ON_RESTART)".to_string()
                    } else {
                        "Task restored after server restart".to_string()
                    },
                    created_at: chrono::Utc::now(),
                },
            )
            .await
            .map_err(|(_, error)| error)?;
            let state_for_task = state.clone();
            let token = CancellationToken::new();
            state
                .task_cancellations
                .lock()
                .await
                .insert(task_id, token.clone());
            tokio::spawn(async move {
                if let Err((failed_task_id, error)) =
                    resume_task_run(&state_for_task, task, token, false).await
                {
                    let _ = emit_event(
                        &state_for_task,
                        session_id,
                        Some(failed_task_id),
                        ServerEvent::TaskFailed {
                            task_id: failed_task_id,
                            error: error.to_string(),
                        },
                    )
                    .await;
                    let _ = fail_task(&state_for_task.pool, failed_task_id).await;
                }
                state_for_task
                    .task_cancellations
                    .lock()
                    .await
                    .remove(&task_id);
            });
        }
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/status", get(auth_status))
        .route(
            "/api/models/config",
            get(model_config).put(update_model_config),
        )
        .route("/api/models/available", get(available_models))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/archived", get(list_archived_sessions))
        .route("/api/sessions/:session_id", delete(delete_session))
        .route("/api/sessions/:session_id/archive", post(archive_session))
        .route("/api/sessions/:session_id/history", get(session_history))
        .route("/api/auth/github", get(github_auth))
        .route(
            "/api/github/pull-requests",
            get(list_pull_requests).post(create_pull_request),
        )
        .route("/api/github/pull-requests/:number", get(get_pull_request))
        .route("/api/files", get(workspace::list_files))
        .route(
            "/api/projects",
            get(workspace::list_projects).post(workspace::create_project),
        )
        .route(
            "/api/files/content",
            get(workspace::read_file)
                .put(workspace::save_file)
                .post(workspace::save_file),
        )
        .route("/api/git/status", get(workspace::git_status))
        .route("/api/git/diff", get(workspace::git_diff))
        .route("/api/git/commit", post(workspace::git_commit))
        .route("/api/git/pull", post(workspace::git_pull))
        .route("/api/git/push", post(workspace::git_push))
        .route("/api/tasks", get(list_tasks))
        .route("/api/worker/jobs", get(list_worker_jobs).post(create_worker_job))
        .route("/api/worker/jobs/:job_id", get(get_worker_job))
        .route("/api/worker/jobs/:job_id/retry", post(retry_worker_job))
        .route("/api/worker/status", get(worker_status))
        .route("/api/permissions", get(list_permissions))
        .route("/api/permissions/audit", get(list_permission_audit))
        .route("/api/permissions/scopes", get(list_permission_scopes))
        .route("/api/permissions/:permission", put(update_permission))
        .route("/api/tools", get(list_tools))
        .route("/api/memory", get(memory_api::list_memory))
        .route(
            "/api/memory/:id",
            get(memory_api::get_memory)
                .patch(memory_api::update_memory)
                .delete(memory_api::delete_memory),
        )
        .route("/api/metrics", get(pipeline_metrics))
        .route("/api/plugins", get(plugins::list_plugins))
        .route("/api/plugins/catalog", get(plugins::list_plugin_catalog))
        .route("/api/plugins/install", post(plugins::install_plugin))
        .route(
            "/api/mcp/servers",
            get(list_mcp_servers).put(update_mcp_servers),
        )
        .route("/ws/:session_id", get(ws_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_local_auth,
        ))
        .layer(cors::cors_layer_from_env())
        .with_state(state.clone());

    let addr: SocketAddr = config.bind_addr.parse().context("parse bind address")?;
    let auth_info = auth::status_payload(&config.auth);
    info!(
        workspace_root = %config.workspace_root.display(),
        demo_file = %config.demo_file_path.display(),
        model = %default_model_name,
        provider = %default_provider_name,
        llm_configured = %state.model_gateway.read().await.is_some(),
        auth_mode = auth_info.mode,
        token_configured = auth_info.token_configured,
        "listening on {}",
        addr
    );
    if !auth_info.token_configured {
        warn!(
            "EVOHIME_API_TOKEN unset: non-loopback clients get 401; set a token before exposing the bind address"
        );
    }
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

async fn auth_status(State(state): State<Arc<AppState>>) -> Json<auth::AuthStatus> {
    Json(auth::status_payload(&state.auth))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn pipeline_metrics(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "pipeline": state.metrics.snapshot(),
        "worker": state.worker_metrics.snapshot(),
    }))
}

#[derive(Debug, Deserialize)]
struct WorkerJobRequest {
    task: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct ListWorkerJobsQuery {
    #[serde(default)]
    limit: Option<i64>,
}

async fn worker_status(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
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

async fn list_worker_jobs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListWorkerJobsQuery>,
) -> Result<Json<Vec<evohime_storage::WorkerJobRow>>, ApiError> {
    let jobs = evohime_storage::list_recent_worker_jobs(&state.pool, query.limit.unwrap_or(50))
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(jobs))
}

async fn enforce_worker_job_limits(state: &AppState) -> Result<(), ApiError> {
    let counts = evohime_storage::count_worker_jobs_by_status(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let active = rate_limit::active_worker_job_count(&counts);
    state
        .rate_limiter
        .allow_worker_job(active)
        .map_err(|error| ApiError::TooManyRequests(error.message))
}

async fn create_worker_job(
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
    evohime_storage::set_worker_job_submitted(&state.pool, row.id, &worker_job.id, 1)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    state.worker_metrics.job_submitted(row.id, &request.task);
    spawn_worker_poll(state.clone(), row.id, worker_job);
    let updated = evohime_storage::load_worker_job(&state.pool, row.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("worker job disappeared after submit".into()))?;
    Ok((StatusCode::ACCEPTED, Json(updated)))
}

async fn get_worker_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<evohime_storage::WorkerJobRow>, ApiError> {
    evohime_storage::load_worker_job(&state.pool, job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .map(Json)
        .ok_or_else(|| ApiError::BadRequest("worker job not found".into()))
}

async fn retry_worker_job(
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
    evohime_storage::set_worker_job_submitted(
        &state.pool,
        row.id,
        &worker_job.id,
        row.attempts + 1,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    state.worker_metrics.job_retried(row.id, &row.task, "manual retry");
    spawn_worker_poll(state.clone(), row.id, worker_job);
    let updated = evohime_storage::load_worker_job(&state.pool, row.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("worker job disappeared after retry".into()))?;
    Ok(Json(updated))
}

fn spawn_worker_poll(state: Arc<AppState>, id: Uuid, worker_job: worker::WorkerJob) {
    tokio::spawn(async move {
        if let Err(error) = run_worker_job(&state, id, worker_job).await {
            let _ =
                evohime_storage::complete_worker_job(&state.pool, id, "failed", None, Some(&error))
                    .await;
            // Best-effort task name for metrics when poll fails hard.
            state.worker_metrics.job_finished(id, "unknown", false);
        }
    });
}

async fn recover_worker_jobs(state: Arc<AppState>) {
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
        spawn_worker_recovery(state.clone(), job);
    }
}

fn spawn_worker_recovery(state: Arc<AppState>, job: evohime_storage::WorkerJobRow) {
    tokio::spawn(async move {
        if let Ok(Some(worker_job)) =
            retry_worker_job_after_error(&state, job.id, "server restart recovery".to_string())
                .await
        {
            if let Err(error) = run_worker_job(&state, job.id, worker_job).await {
                let _ = evohime_storage::complete_worker_job(
                    &state.pool,
                    job.id,
                    "failed",
                    None,
                    Some(&error),
                )
                .await;
            }
        }
    });
}

async fn run_worker_job(
    state: &AppState,
    id: Uuid,
    mut worker_job: worker::WorkerJob,
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
                evohime_storage::complete_worker_job(
                    &state.pool,
                    id,
                    &worker_job.status,
                    worker_job.result.as_ref(),
                    worker_job.error.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?;
                state.worker_metrics.job_finished(id, &task_name, ok);
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
                    "worker job heartbeat stalled".to_string(),
                )
                .await?
                {
                    Some(job) => {
                        worker_job = job;
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
                    match retry_worker_job_after_error(state, id, error.to_string()).await? {
                        Some(job) => worker_job = job,
                        None => {
                            state.worker_metrics.job_finished(id, &task_name, false);
                            return Ok(());
                        }
                    }
                }
            }
        }
        match retry_worker_job_after_error(state, id, "worker polling timed out".to_string())
            .await?
        {
            Some(job) => worker_job = job,
            None => {
                state.worker_metrics.job_finished(id, &task_name, false);
                return Ok(());
            }
        }
    }
}

async fn worker_health_loop(state: Arc<AppState>, interval: Duration, stale_after: Duration) {
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

async fn retry_worker_job_after_error(
    state: &AppState,
    id: Uuid,
    error: String,
) -> Result<Option<worker::WorkerJob>, String> {
    let row = evohime_storage::load_worker_job(&state.pool, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "worker job disappeared during retry".to_string())?;
    if row.attempts >= row.max_attempts {
        evohime_storage::complete_worker_job(&state.pool, id, "failed", None, Some(&error))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let mut attempts = row.attempts;
    loop {
        tokio::time::sleep(worker::retry_delay(attempts)).await;
        match state.worker.submit(&row.task, &row.payload_json).await {
            Ok(worker_job) => {
                evohime_storage::set_worker_job_submitted(
                    &state.pool,
                    id,
                    &worker_job.id,
                    attempts + 1,
                )
                .await
                .map_err(|e| e.to_string())?;
                state
                    .worker_metrics
                    .job_retried(id, &row.task, &error);
                return Ok(Some(worker_job));
            }
            Err(submit_error) if attempts + 1 >= row.max_attempts => {
                evohime_storage::complete_worker_job(
                    &state.pool,
                    id,
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

async fn worker_retention_loop(state: Arc<AppState>, retention_days: i64) {
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

async fn model_config(
    State(state): State<Arc<AppState>>,
) -> Json<evohime_model_gateway::ModelConfigResponse> {
    Json(state.model_config_response().await)
}

#[derive(Debug, Serialize)]
struct AvailableModelsResponse {
    route: String,
    provider: String,
    billing_mode: String,
    models: Vec<String>,
}

async fn available_models(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<AvailableModelsResponse>, ApiError> {
    let default_route = state.model_config.read().await.default_route.clone();
    let route_name = query.get("route").cloned().unwrap_or(default_route);
    let config = state.model_config.read().await;
    let effective_route_name =
        if route_name == "orchestrator" && !config.routes.contains_key(&route_name) {
            config.default_route.clone()
        } else {
            route_name.clone()
        };
    let route = config
        .routes
        .get(&effective_route_name)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown model route: {route_name}")))?;
    let provider = route.provider.as_str().to_string();
    let billing_mode = if route.provider == ProviderKind::LiteRouter
        && route.literouter.model.ends_with(":free")
    {
        "free"
    } else {
        "paid"
    };
    let models = if route.provider == ProviderKind::Mock {
        Vec::new()
    } else {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/models",
                route.literouter.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&route.literouter.api_key)
            .send()
            .await
            .map_err(|error| {
                ApiError::Internal(format!("не удалось получить список моделей: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(ApiError::BadRequest(format!(
                "провайдер вернул ошибку списка моделей: {}",
                response.status()
            )));
        }
        let payload: OpenAiModelsResponse = response.json().await.map_err(|error| {
            ApiError::Internal(format!("некорректный ответ списка моделей: {error}"))
        })?;
        let mut models = payload
            .data
            .into_iter()
            .map(|model| model.id)
            .filter(|model| {
                if billing_mode == "free" {
                    model.ends_with(":free")
                } else {
                    !model.ends_with(":free")
                }
            })
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        models
    };
    Ok(Json(AvailableModelsResponse {
        route: route_name,
        provider,
        billing_mode: billing_mode.to_string(),
        models,
    }))
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    id: String,
}

async fn update_model_config(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ModelSettingsRequest>,
) -> Result<Json<evohime_model_gateway::ModelConfigResponse>, ApiError> {
    let current = state.model_config.read().await;
    let config = build_model_config(request.clone(), &current)?;
    drop(current);
    let gateway = if config.routes.values().all(ModelRouteConfig::configured) {
        Some(Arc::new(
            ModelGateway::from_config(&config)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        ))
    } else {
        None
    };
    let mut persisted_request = request;
    for route in &mut persisted_request.routes {
        if route
            .api_key
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            route.api_key = config
                .routes
                .get(&route.name)
                .map(|item| item.literouter.api_key.clone())
                .filter(|key| !key.trim().is_empty());
        }
    }
    let persisted_value = serde_json::to_value(persisted_request)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::save_setting(&state.pool, "model_config", &persisted_value)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    *state.model_config.write().await = config;
    *state.model_gateway.write().await = gateway;

    Ok(Json(state.model_config_response().await))
}

fn build_model_config(
    request: ModelSettingsRequest,
    current: &evohime_model_gateway::ModelGatewayConfig,
) -> Result<evohime_model_gateway::ModelGatewayConfig, ApiError> {
    if request.default_route.trim().is_empty() || request.routes.is_empty() {
        return Err(ApiError::BadRequest(
            "Нужен хотя бы один маршрут и маршрут по умолчанию".to_string(),
        ));
    }
    let mut routes = HashMap::new();
    let requested_default_key = request
        .routes
        .iter()
        .find(|route| route.name.trim() == request.default_route)
        .and_then(|route| route.api_key.as_deref())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            current
                .routes
                .get(&request.default_route)
                .map(|route| route.literouter.api_key.clone())
        })
        .unwrap_or_default();
    for route in request.routes {
        let name = route.name.trim().to_string();
        let model = route.model.trim().to_string();
        let base_url = route.base_url.trim().to_string();
        if name.is_empty() || model.is_empty() {
            return Err(ApiError::BadRequest(
                "Название маршрута и модель не могут быть пустыми".to_string(),
            ));
        }
        if routes.contains_key(&name) {
            return Err(ApiError::BadRequest(format!(
                "Маршрут '{name}' указан несколько раз"
            )));
        }
        let provider = ProviderKind::parse(&route.provider).ok_or_else(|| {
            ApiError::BadRequest(format!("Неизвестный провайдер: {}", route.provider))
        })?;
        let billing_mode = if provider == ProviderKind::LiteRouter {
            route.billing_mode.as_str()
        } else {
            "paid"
        };
        if !matches!(billing_mode, "free" | "paid") {
            return Err(ApiError::BadRequest(
                "Режим LiteRouter должен быть free или paid".to_string(),
            ));
        }
        if provider == ProviderKind::LiteRouter {
            let is_free_model = model.ends_with(":free");
            if billing_mode == "free" && !is_free_model {
                return Err(ApiError::BadRequest(
                    "В бесплатном режиме LiteRouter доступны только модели с суффиксом :free"
                        .to_string(),
                ));
            }
            if billing_mode == "paid" && is_free_model {
                return Err(ApiError::BadRequest(
                    "В платном режиме выбери модель без суффикса :free".to_string(),
                ));
            }
        }
        let existing_key = current
            .routes
            .get(&name)
            .map(|item| item.literouter.api_key.clone())
            .or_else(|| {
                (name == "orchestrator").then(|| {
                    current
                        .routes
                        .get(&current.default_route)
                        .map(|item| item.literouter.api_key.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| requested_default_key.clone())
                })
            })
            .unwrap_or_default();
        let api_key = if name == "orchestrator" {
            requested_default_key.clone()
        } else {
            route
                .api_key
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(existing_key)
        };
        let config = match provider {
            ProviderKind::LiteRouter => ModelRouteConfig::literouter(api_key, base_url, model),
            ProviderKind::OpenAICompatible => {
                ModelRouteConfig::openai_compatible(api_key, base_url, model)
            }
            ProviderKind::Mock => ModelRouteConfig::mock(model),
        };
        routes.insert(name, config);
    }
    if !routes.contains_key(&request.default_route) {
        return Err(ApiError::BadRequest(
            "Маршрут по умолчанию должен существовать в списке маршрутов".to_string(),
        ));
    }
    Ok(evohime_model_gateway::ModelGatewayConfig {
        default_route: request.default_route,
        routes,
    })
}

async fn list_permissions(State(state): State<Arc<AppState>>) -> Json<Value> {
    let names = [
        ("filesystem_read", Permission::FilesystemRead),
        ("filesystem_write", Permission::FilesystemWrite),
        ("shell_execute", Permission::ShellExecute),
        ("git_read", Permission::GitRead),
        ("git_write", Permission::GitWrite),
        ("browser_access", Permission::BrowserAccess),
        ("mcp_call", Permission::McpCall),
    ];
    let mut result = serde_json::Map::new();
    for (name, permission) in names {
        result.insert(
            name.into(),
            json!({"mode": state.permissions.mode(permission).await}),
        );
    }
    Json(Value::Object(result))
}

async fn list_permission_audit(State(state): State<Arc<AppState>>) -> Json<Value> {
    let entries = state.permissions.audit_log().await;
    Json(json!({ "entries": entries }))
}

async fn list_permission_scopes(State(state): State<Arc<AppState>>) -> Json<Value> {
    let session_overrides = state.permissions.list_session_overrides().await;
    let path_grants = state.permissions.list_path_grants().await;
    Json(json!({
        "session_overrides": session_overrides,
        "path_grants": path_grants,
    }))
}

#[derive(serde::Serialize)]
struct ToolSummary {
    name: String,
    description: String,
    permissions: Vec<String>,
    timeout_ms: u64,
}

async fn list_tools(State(state): State<Arc<AppState>>) -> Json<Vec<ToolSummary>> {
    let tools = state
        .tools
        .list()
        .into_iter()
        .map(|tool| ToolSummary {
            name: tool.name.to_string(),
            description: tool.description.to_string(),
            permissions: tool
                .permissions
                .iter()
                .map(|permission| permission_name(*permission).to_string())
                .collect(),
            timeout_ms: duration_to_ms(tool.timeout),
        })
        .collect();
    Json(tools)
}

async fn list_mcp_servers(State(state): State<Arc<AppState>>) -> Json<Vec<McpServerConfig>> {
    Json(state.mcp_servers.lock().await.clone())
}

async fn update_mcp_servers(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Vec<McpServerConfig>>,
) -> Result<Json<Vec<McpServerConfig>>, ApiError> {
    let servers = body
        .into_iter()
        .map(validate_mcp_server)
        .collect::<Result<Vec<_>, _>>()?;
    let value =
        serde_json::to_value(&servers).map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::save_setting(&state.pool, "mcp_servers", &value)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    *state.mcp_servers.lock().await = servers.clone();
    Ok(Json(servers))
}

async fn update_permission(
    State(state): State<Arc<AppState>>,
    Path(permission): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let permission = match permission.as_str() {
        "filesystem_read" => Permission::FilesystemRead,
        "filesystem_write" => Permission::FilesystemWrite,
        "shell_execute" => Permission::ShellExecute,
        "git_read" => Permission::GitRead,
        "git_write" => Permission::GitWrite,
        "browser_access" => Permission::BrowserAccess,
        "mcp_call" => Permission::McpCall,
        _ => return Err(ApiError::BadRequest("unknown permission".into())),
    };
    let mode: PermissionMode = serde_json::from_value(
        body.get("mode")
            .cloned()
            .ok_or_else(|| ApiError::BadRequest("mode is required".into()))?,
    )
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    state.permissions.set_mode(permission, mode).await;
    let settings = permission_settings_value(&state).await;
    evohime_storage::save_setting(&state.pool, "permissions", &settings)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(
        json!({"permission": permission_name(permission), "mode": mode}),
    ))
}

fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::FilesystemRead => "filesystem_read",
        Permission::FilesystemWrite => "filesystem_write",
        Permission::ShellExecute => "shell_execute",
        Permission::GitRead => "git_read",
        Permission::GitWrite => "git_write",
        Permission::BrowserAccess => "browser_access",
        Permission::McpCall => "mcp_call",
    }
}

fn parse_permission_name(name: &str) -> Option<Permission> {
    match name {
        "filesystem_read" => Some(Permission::FilesystemRead),
        "filesystem_write" => Some(Permission::FilesystemWrite),
        "shell_execute" => Some(Permission::ShellExecute),
        "git_read" => Some(Permission::GitRead),
        "git_write" => Some(Permission::GitWrite),
        "browser_access" => Some(Permission::BrowserAccess),
        "mcp_call" => Some(Permission::McpCall),
        _ => None,
    }
}

async fn permission_settings_value(state: &AppState) -> Value {
    let mut settings = serde_json::Map::new();
    for permission in [
        Permission::FilesystemRead,
        Permission::FilesystemWrite,
        Permission::ShellExecute,
        Permission::GitRead,
        Permission::GitWrite,
        Permission::BrowserAccess,
        Permission::McpCall,
    ] {
        settings.insert(
            permission_name(permission).to_string(),
            json!(state.permissions.mode(permission).await),
        );
    }
    Value::Object(settings)
}

async fn persist_permission_scopes(state: &AppState) -> Result<(), ApiError> {
    let snapshot = state.permissions.export_scopes().await;
    let value = serde_json::to_value(&snapshot)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::save_setting(&state.pool, "permission_scopes", &value)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(())
}

fn duration_to_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn validate_mcp_server(mut server: McpServerConfig) -> Result<McpServerConfig, ApiError> {
    server.name = server.name.trim().to_string();
    server.url = server.url.trim().to_string();
    if server.name.is_empty() {
        return Err(ApiError::BadRequest("mcp server name is required".into()));
    }

    let parsed = url::Url::parse(&server.url)
        .map_err(|error| ApiError::BadRequest(format!("invalid MCP server url: {error}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(ApiError::BadRequest(
                "mcp server url must use http or https".into(),
            ))
        }
    }

    if let Some(description) = server.description.as_mut() {
        let trimmed = description.trim().to_string();
        if trimmed.is_empty() {
            server.description = None;
        } else {
            *description = trimmed;
        }
    }

    Ok(server)
}

async fn create_session(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionBootstrap>, ApiError> {
    state
        .rate_limiter
        .allow_session_create()
        .map_err(|error| ApiError::TooManyRequests(error.message))?;

    let session = evohime_storage::create_session(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let event = ServerEvent::SessionCreated {
        session_id: session.id,
        created_at: session.created_at,
    };
    let event_json = to_value(&event).map_err(|error| ApiError::Internal(error.to_string()))?;
    let (sequence, created_at) =
        evohime_storage::insert_event(&state.pool, session.id, &event_json, None)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(SessionBootstrap {
        session_id: session.id,
        created_at: session.created_at,
        events: vec![HistoryItem {
            sequence,
            created_at,
            event,
        }],
    }))
}

async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = evohime_storage::delete_session(&state.pool, session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    if !deleted {
        return Err(ApiError::BadRequest("Чат не найден".to_string()));
    }

    state.session_buses.lock().await.remove(&session_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn archive_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let archived = evohime_storage::archive_session(&state.pool, session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    if !archived {
        return Err(ApiError::BadRequest(
            "Чат не найден или уже архивирован".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Serialize)]
struct SessionSummary {
    session_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    title: Option<String>,
    workspace_path: Option<String>,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message: Option<String>,
    last_role: Option<String>,
}

fn session_summary(row: evohime_storage::SessionSummaryRow) -> SessionSummary {
    SessionSummary {
        session_id: row.id,
        created_at: row.created_at,
        title: row.title,
        workspace_path: row.workspace_path,
        last_message_at: row.last_message_at,
        last_message: row.last_message,
        last_role: row.last_role,
    }
}

fn summarize_session_title(message: &str) -> String {
    let normalized = message
        .split("\n\nВложения:")
        .next()
        .unwrap_or(message)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = normalized.to_lowercase();
    let title = if lower.contains("разберись") && lower.contains("код") {
        "Разбор кода проекта".to_string()
    } else if lower.contains("запусти") && lower.contains("провер") {
        "Проверка проекта".to_string()
    } else if lower.contains("исправ") || lower.contains("почини") {
        "Исправление проекта".to_string()
    } else {
        normalized.chars().take(56).collect()
    };
    title
        .trim_end_matches([' ', '.', ',', ':', ';', '!', '?'])
        .to_string()
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let rows = evohime_storage::list_sessions(&state.pool, 20)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let sessions = rows.into_iter().map(session_summary).collect();

    Ok(Json(sessions))
}

async fn list_archived_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let rows = evohime_storage::list_archived_sessions(&state.pool, 100)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(rows.into_iter().map(session_summary).collect()))
}

#[derive(Debug, serde::Serialize)]
struct GithubAuthResponse {
    authenticated: bool,
    login: Option<String>,
    source: &'static str,
}

async fn github_auth() -> Json<GithubAuthResponse> {
    let login = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Json(GithubAuthResponse {
        authenticated: login.is_some(),
        login,
        source: "gh",
    })
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum PullRequestScope {
    All,
    Created,
    ReviewRequested,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct GithubPullRequestUser {
    login: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubPullRequestSummary {
    number: u64,
    title: String,
    url: String,
    state: String,
    author: Option<GithubPullRequestUser>,
    head_ref_name: String,
    base_ref_name: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, serde::Deserialize)]
struct GithubPullRequestQuery {
    #[serde(default)]
    scope: Option<PullRequestScope>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GithubPullRequestComment {
    author: Option<GithubPullRequestUser>,
    body: String,
    created_at: Option<String>,
    url: Option<String>,
    state: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GithubCheck {
    name: String,
    status: Option<String>,
    conclusion: Option<String>,
    details_url: Option<String>,
    workflow_name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GithubPullRequestDetail {
    #[serde(flatten)]
    summary: GithubPullRequestSummary,
    body: String,
    is_draft: bool,
    merge_state_status: Option<String>,
    diff: String,
    comments: Vec<GithubPullRequestComment>,
    reviews: Vec<GithubPullRequestComment>,
    checks: Vec<GithubCheck>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GithubCreatePullRequestRequest {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    head: Option<String>,
}

async fn list_pull_requests(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GithubPullRequestQuery>,
) -> Result<Json<Vec<GithubPullRequestSummary>>, ApiError> {
    let workspace_root = state.workspace_root.clone();
    let scope = query.scope.unwrap_or(PullRequestScope::All);
    let result = tokio::task::spawn_blocking(move || {
        let mut command = std::process::Command::new("gh");
        command.current_dir(&workspace_root).args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "40",
            "--json",
            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt",
        ]);

        match scope {
            PullRequestScope::All => {}
            PullRequestScope::Created => {
                command.args(["--search", "author:@me"]);
            }
            PullRequestScope::ReviewRequested => {
                command.args(["--search", "review-requested:@me"]);
            }
        }

        let output = command.output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        let prs = serde_json::from_slice::<Vec<GithubPullRequestSummary>>(&output.stdout)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(prs)
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    let prs = result.map_err(ApiError::Internal)?;
    Ok(Json(prs))
}

async fn get_pull_request(
    State(state): State<Arc<AppState>>,
    Path(number): Path<u64>,
) -> Result<Json<GithubPullRequestDetail>, ApiError> {
    let workspace_root = state.workspace_root.clone();
    let detail =
        tokio::task::spawn_blocking(move || load_pull_request_detail(&workspace_root, number))
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .map_err(ApiError::Internal)?;
    Ok(Json(detail))
}

async fn create_pull_request(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GithubCreatePullRequestRequest>,
) -> Result<Json<GithubPullRequestDetail>, ApiError> {
    let title = request.title.trim().to_string();
    if title.is_empty() {
        return Err(ApiError::BadRequest(
            "pull request title is required".to_string(),
        ));
    }

    let workspace_root = state.workspace_root.clone();
    let detail = tokio::task::spawn_blocking(move || {
        let mut args = vec![
            "pr".to_string(),
            "create".to_string(),
            "--title".to_string(),
            title,
            "--body".to_string(),
            request.body,
        ];
        if let Some(base) = request.base.filter(|value| !value.trim().is_empty()) {
            args.extend(["--base".to_string(), base]);
        }
        if let Some(head) = request.head.filter(|value| !value.trim().is_empty()) {
            args.extend(["--head".to_string(), head]);
        }

        let output = run_gh_command(&workspace_root, &args)?;
        let url = output
            .lines()
            .rev()
            .find(|line| line.trim().starts_with("http"))
            .map(str::trim)
            .ok_or_else(|| "gh pr create did not return a pull request URL".to_string())?;
        let created_number = run_gh_command(
            &workspace_root,
            &[
                "pr".to_string(),
                "view".to_string(),
                url.to_string(),
                "--json".to_string(),
                "number".to_string(),
            ],
        )?;
        let number = serde_json::from_str::<Value>(&created_number)
            .ok()
            .and_then(|value| value.get("number").and_then(Value::as_u64))
            .ok_or_else(|| {
                "gh pr view did not return the created pull request number".to_string()
            })?;
        load_pull_request_detail(&workspace_root, number)
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?
    .map_err(ApiError::Internal)?;

    Ok(Json(detail))
}

fn load_pull_request_detail(
    workspace_root: &std::path::Path,
    number: u64,
) -> Result<GithubPullRequestDetail, String> {
    let json_output = run_gh_command(
        workspace_root,
        &[
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--json".to_string(),
            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt,body,isDraft,mergeStateStatus,comments,reviews,statusCheckRollup".to_string(),
        ],
    )?;
    let value = serde_json::from_str::<Value>(&json_output).map_err(|error| error.to_string())?;
    let summary = serde_json::from_value::<GithubPullRequestSummary>(value.clone())
        .map_err(|error| error.to_string())?;
    let diff = run_gh_command(
        workspace_root,
        &["pr".to_string(), "diff".to_string(), number.to_string()],
    )?;

    Ok(GithubPullRequestDetail {
        summary,
        body: value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        is_draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        merge_state_status: value
            .get("mergeStateStatus")
            .and_then(Value::as_str)
            .map(str::to_string),
        diff,
        comments: parse_pull_request_comments(value.get("comments")),
        reviews: parse_pull_request_comments(value.get("reviews")),
        checks: parse_checks(value.get("statusCheckRollup")),
    })
}

fn run_gh_command(workspace_root: &std::path::Path, args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("gh")
        .current_dir(workspace_root)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_pull_request_comments(value: Option<&Value>) -> Vec<GithubPullRequestComment> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| GithubPullRequestComment {
            author: item
                .get("author")
                .and_then(|author| serde_json::from_value(author.clone()).ok()),
            body: item
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            created_at: item
                .get("createdAt")
                .and_then(Value::as_str)
                .or_else(|| item.get("submittedAt").and_then(Value::as_str))
                .map(str::to_string),
            url: item.get("url").and_then(Value::as_str).map(str::to_string),
            state: item
                .get("state")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect()
}

fn parse_checks(value: Option<&Value>) -> Vec<GithubCheck> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| GithubCheck {
            name: item
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| item.get("context").and_then(Value::as_str))
                .unwrap_or("check")
                .to_string(),
            status: item
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
            conclusion: item
                .get("conclusion")
                .and_then(Value::as_str)
                .map(str::to_string),
            details_url: item
                .get("detailsUrl")
                .and_then(Value::as_str)
                .or_else(|| item.get("details_url").and_then(Value::as_str))
                .map(str::to_string),
            workflow_name: item
                .get("workflowName")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect()
}

async fn session_history(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryItem>>, ApiError> {
    let after = query.after.unwrap_or(0).max(0);
    let rows = evohime_storage::list_session_events_after(&state.pool, session_id, after)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let mut history = Vec::with_capacity(rows.len());
    for row in rows {
        let event: ServerEvent = serde_json::from_value(row.event_json)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        history.push(HistoryItem {
            sequence: row.sequence,
            created_at: row.created_at,
            event,
        });
    }

    Ok(Json(history))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    after: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct WsConnectQuery {
    after_sequence: Option<i64>,
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Query(query): Query<WsConnectQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let after_sequence = query.after_sequence.unwrap_or(0).max(0);
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_socket(state, session_id, after_sequence, socket).await {
            error!("websocket session failed: {error}");
        }
    })
}

async fn handle_socket(
    state: Arc<AppState>,
    session_id: Uuid,
    after_sequence: i64,
    socket: WebSocket,
) -> Result<(), ApiError> {
    if evohime_storage::load_session(&state.pool, session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .is_none()
    {
        return Err(ApiError::BadRequest("unknown session".to_string()));
    }

    let bus_sender = state.session_bus(session_id).await;
    let mut bus_receiver = bus_sender.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Replay durable events first, then forward live bus items (client dedupes by sequence).
    let backlog = evohime_storage::list_session_events_after(&state.pool, session_id, after_sequence)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    for row in backlog {
        let event: ServerEvent = serde_json::from_value(row.event_json)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        let item = HistoryItem {
            sequence: row.sequence,
            created_at: row.created_at,
            event,
        };
        let serialized =
            serde_json::to_string(&item).map_err(|error| ApiError::Internal(error.to_string()))?;
        if sender.send(Message::Text(serialized)).await.is_err() {
            return Ok(());
        }
    }

    let forward_handle = tokio::spawn(async move {
        while let Ok(item) = bus_receiver.recv().await {
            if item.sequence <= after_sequence {
                continue;
            }
            let serialized = match serde_json::to_string(&item) {
                Ok(serialized) => serialized,
                Err(error) => {
                    error!("failed to serialize event: {error}");
                    break;
                }
            };
            if sender.send(Message::Text(serialized)).await.is_err() {
                break;
            }
        }
    });
    let result = async {
        while let Some(message) = receiver.next().await {
            let message = match message {
                Ok(message) => message,
                Err(error) => return Err(ApiError::Internal(error.to_string())),
            };

            match message {
                Message::Text(text) => {
                    let command: ClientCommand = serde_json::from_str(&text)
                        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
                    match command {
                        ClientCommand::UserMessage {
                            content,
                            model_route,
                            model,
                            workspace_path,
                        } => {
                            let concurrent = state.task_cancellations.lock().await.len();
                            if let Err(error) = state.rate_limiter.allow_task_start(concurrent) {
                                warn!(error = %error.message, "task start rate limited");
                                let _ = emit_event(
                                    &state,
                                    session_id,
                                    None,
                                    ServerEvent::ActionLogged {
                                        task_id: Uuid::nil(),
                                        action: "rate.limited".to_string(),
                                        detail: error.message.clone(),
                                        created_at: chrono::Utc::now(),
                                    },
                                )
                                .await;
                                continue;
                            }
                            let workspace_path = resolve_workspace_path(&state, workspace_path)?;
                            let workspace_path = workspace_path.to_string_lossy().to_string();
                            let task = match start_task(
                                &state.pool,
                                session_id,
                                &content,
                                model_route.as_deref(),
                                model.as_deref(),
                                Some(&workspace_path),
                            )
                            .await
                            {
                                Ok(task) => task,
                                Err(error) => {
                                    error!("failed to create task: {error}");
                                    continue;
                                }
                            };

                            let task_id = task.id;
                            let token = CancellationToken::new();
                            state
                                .task_cancellations
                                .lock()
                                .await
                                .insert(task_id, token.clone());
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
                                if let Err((task_id, error)) =
                                    process_user_message(&state_for_task, session_id, task, token)
                                        .await
                                {
                                    let _ = emit_event(
                                        &state_for_task,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: error.to_string(),
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                state_for_task
                                    .task_cancellations
                                    .lock()
                                    .await
                                    .remove(&task_id);
                            });
                        }
                        ClientCommand::TaskCancel { task_id } => {
                            let cancellation =
                                state.task_cancellations.lock().await.get(&task_id).cloned();
                            if let Some(token) = cancellation {
                                token.cancel();
                            }
                            let _ = evohime_task_engine::cancel_task(&state.pool, task_id).await;
                            let _ = finalize_open_task_steps(&state, task_id, "cancelled").await;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::TaskStatusChanged {
                                    task_id,
                                    status: "cancelled".to_string(),
                                },
                            )
                            .await?;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::ActionLogged {
                                    task_id,
                                    action: "task.cancel".to_string(),
                                    detail: "Task cancellation requested".to_string(),
                                    created_at: chrono::Utc::now(),
                                },
                            )
                            .await?;
                        }
                        ClientCommand::TaskResume { task_id } => {
                            let task = match evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                            {
                                Some(task) => task,
                                None => continue,
                            };
                            let _ = resume_task(&state.pool, task_id).await;
                            let _ = evohime_storage::merge_checkpoint(
                                &state.pool,
                                task_id,
                                None,
                                &json!({
                                    "pause_reason": Value::Null,
                                    "approval_wait": Value::Null,
                                }),
                            )
                            .await;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::TaskStatusChanged {
                                    task_id,
                                    status: "running".to_string(),
                                },
                            )
                            .await?;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::ActionLogged {
                                    task_id,
                                    action: "task.resume".to_string(),
                                    detail: "Task resumed from checkpoint".to_string(),
                                    created_at: chrono::Utc::now(),
                                },
                            )
                            .await?;

                            let token = CancellationToken::new();
                            state
                                .task_cancellations
                                .lock()
                                .await
                                .insert(task_id, token.clone());
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
                                if let Err((task_id, error)) =
                                    resume_task_run(&state_for_task, task, token, false).await
                                {
                                    let _ = emit_event(
                                        &state_for_task,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: error.to_string(),
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                state_for_task
                                    .task_cancellations
                                    .lock()
                                    .await
                                    .remove(&task_id);
                            });
                        }
                        ClientCommand::TaskRetry { task_id } => {
                            let task = match evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                            {
                                Some(task) => task,
                                None => continue,
                            };
                            let _ = retry_task(&state.pool, task_id).await;
                            state.metrics.task_retry(session_id, task_id);
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::TaskStatusChanged {
                                    task_id,
                                    status: "running".to_string(),
                                },
                            )
                            .await?;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::ActionLogged {
                                    task_id,
                                    action: "task.retry".to_string(),
                                    detail: "Failed task scheduled for retry".to_string(),
                                    created_at: chrono::Utc::now(),
                                },
                            )
                            .await?;

                            let token = CancellationToken::new();
                            state
                                .task_cancellations
                                .lock()
                                .await
                                .insert(task_id, token.clone());
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
                                if let Err((task_id, error)) =
                                    resume_task_run(&state_for_task, task, token, false).await
                                {
                                    let _ = emit_event(
                                        &state_for_task,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: error.to_string(),
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                state_for_task
                                    .task_cancellations
                                    .lock()
                                    .await
                                    .remove(&task_id);
                            });
                        }
                        ClientCommand::ApprovalGranted { approval_id } => {
                            let task_id = state
                                .permissions
                                .approval(approval_id)
                                .await
                                .map(|(request, _)| request.task_id)
                                .unwrap_or(Uuid::nil());
                            let status = state.permissions.resolve(approval_id, true).await;
                            if status.is_some() {
                                if let Err(error) = persist_permission_scopes(&state).await {
                                    warn!(error = %error, "failed to persist permission scopes after grant");
                                }
                            }
                            state.metrics.approval_resolved(
                                session_id,
                                task_id,
                                approval_id,
                                true,
                            );
                            let detail = if status.is_some() {
                                "Approval granted"
                            } else {
                                "Approval was already resolved or unknown"
                            };
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::ActionLogged {
                                    task_id,
                                    action: "approval.granted".into(),
                                    detail: detail.into(),
                                    created_at: chrono::Utc::now(),
                                },
                            )
                            .await?;
                        }
                        ClientCommand::ApprovalDenied { approval_id } => {
                            let task_id = state
                                .permissions
                                .approval(approval_id)
                                .await
                                .map(|(request, _)| request.task_id)
                                .unwrap_or(Uuid::nil());
                            let status = state.permissions.resolve(approval_id, false).await;
                            state.metrics.approval_resolved(
                                session_id,
                                task_id,
                                approval_id,
                                false,
                            );
                            let detail = if status.is_some() {
                                "Approval denied"
                            } else {
                                "Approval was already resolved or unknown"
                            };
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::ActionLogged {
                                    task_id,
                                    action: "approval.denied".into(),
                                    detail: detail.into(),
                                    created_at: chrono::Utc::now(),
                                },
                            )
                            .await?;
                        }
                        ClientCommand::MemoryAccept { memory_id } => {
                            handle_memory_decision(&state, session_id, memory_id, true).await;
                        }
                        ClientCommand::MemoryReject { memory_id } => {
                            handle_memory_decision(&state, session_id, memory_id, false).await;
                        }
                    }
                }
                Message::Close(_) => break,
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
            }
        }

        Ok(())
    }
    .await;

    forward_handle.abort();
    result
}

async fn process_user_message(
    state: &Arc<AppState>,
    session_id: Uuid,
    task: evohime_storage::TaskRow,
    cancellation: CancellationToken,
) -> Result<(), (Uuid, ApiError)> {
    run_task_pipeline(state, session_id, task, cancellation, true).await
}

async fn resume_task_run(
    state: &Arc<AppState>,
    task: evohime_storage::TaskRow,
    cancellation: CancellationToken,
    emit_started: bool,
) -> Result<(), (Uuid, ApiError)> {
    run_task_pipeline(state, task.session_id, task, cancellation, emit_started).await
}

async fn run_task_pipeline(
    state: &Arc<AppState>,
    session_id: Uuid,
    task: evohime_storage::TaskRow,
    cancellation: CancellationToken,
    emit_started: bool,
) -> Result<(), (Uuid, ApiError)> {
    if emit_started {
        state.metrics.task_started(session_id, task.id);
    } else {
        state.metrics.task_resumed(session_id, task.id);
    }

    let gateway = state.model_gateway.read().await.clone().ok_or_else(|| {
        state.metrics.task_finished(session_id, task.id, false);
        (
            task.id,
            ApiError::Internal("LITEROUTER_API_KEY is not configured — set it in .env".to_string()),
        )
    })?;

    let prior_messages = load_chat_history(&state.pool, session_id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    let memory_notes = load_memory_notes(&state.pool, session_id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    let workspace_scope = task
        .workspace_path
        .clone()
        .unwrap_or_else(|| state.workspace_root.to_string_lossy().into_owned());
    let global_memory = evohime_storage::list_global_memory(&state.pool, &workspace_scope, 20)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    let mut memory_notes = memory_notes;
    memory_notes.extend(
        global_memory
            .into_iter()
            .map(|row| format!("[global workspace memory] {}", row.note)),
    );

    let structured = evohime_memory::retrieve_for_prompt(
        &state.pool,
        evohime_memory::RetrieveRequest {
            session_id: Some(session_id),
            workspace_key: &workspace_scope,
            query: &task.user_message,
            max_chars: 4_000,
            max_items: 24,
        },
    )
    .await
    .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    let used_memory_ids = structured.used_memory_ids.clone();
    if !used_memory_ids.is_empty() {
        tracing::info!(
            session_id = %session_id,
            task_id = %task.id,
            used_memory_ids = ?used_memory_ids,
            "retrieved structured memory for prompt"
        );
    }
    let mut structured_notes = structured
        .entries
        .into_iter()
        .map(|entry| {
            match (&entry.scope, &entry.status) {
                (Some(scope), Some(status)) => format!("[{scope}/{status}] {}", entry.content),
                _ => entry.content,
            }
        })
        .collect::<Vec<_>>();
    structured_notes.extend(memory_notes);
    let memory_notes = structured_notes;

    if emit_started {
        let title = summarize_session_title(&task.user_message);
        if !title.is_empty() {
            evohime_storage::set_session_title_if_empty(&state.pool, session_id, &title)
                .await
                .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
        }
        evohime_storage::insert_message(
            &state.pool,
            session_id,
            Some(task.id),
            "user",
            &task.user_message,
        )
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    }

    let checkpoint = if emit_started {
        None
    } else {
        evohime_storage::load_checkpoint(&state.pool, task.id)
            .await
            .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?
    };

    let task_steps = if emit_started {
        Vec::new()
    } else {
        evohime_storage::list_task_steps(&state.pool, task.id)
            .await
            .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?
    };

    let resume_context = if emit_started {
        None
    } else {
        Some(build_agent_resume_context(checkpoint.as_ref(), &task_steps))
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let default_route = state.model_config.read().await.default_route.clone();
    let model_route = resolve_model_route(task.model_route.as_deref(), &default_route);
    let (planning_model_route, planning_model) = if task
        .model
        .as_deref()
        .is_some_and(|model| !model.trim().is_empty())
    {
        (model_route.clone(), task.model.clone())
    } else {
        let config = state.model_config.read().await;
        let route_name = if config.routes.contains_key("orchestrator") {
            "orchestrator".to_string()
        } else {
            default_route.clone()
        };
        let model = config
            .routes
            .get(&route_name)
            .map(|route| route.literouter.model.clone());
        (route_name, model)
    };
    let agent_config = AgentConfig {
        task_id: task.id,
        session_id,
        user_message: task.user_message.clone(),
        created_at: task.created_at,
        demo_file_path: task
            .workspace_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| state.workspace_root.clone())
            .join("docs/sample-context.md"),
        workspace_root: task
            .workspace_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| state.workspace_root.clone()),
        model_route,
        model: task.model.clone(),
        planning_model_route,
        planning_model,
        memory_pool: Some(state.pool.clone()),
        workspace_key: workspace_scope.clone(),
    };

    let tools = state.tools.clone();
    let gateway_for_agent = gateway.clone();
    let mut agent_handle = tokio::spawn(async move {
        match resume_context {
            Some(resume) => {
                run_agent_loop_resumed(
                    agent_config,
                    &gateway_for_agent,
                    &tools,
                    prior_messages,
                    memory_notes.clone(),
                    event_tx,
                    resume,
                )
                .await
            }
            None if emit_started => {
                run_agent_loop(
                    agent_config,
                    &gateway_for_agent,
                    &tools,
                    prior_messages,
                    memory_notes.clone(),
                    event_tx,
                )
                .await
            }
            None => {
                run_agent_loop_resumed(
                    agent_config,
                    &gateway_for_agent,
                    &tools,
                    prior_messages,
                    memory_notes,
                    event_tx,
                    AgentResumeContext::default(),
                )
                .await
            }
        }
    });

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                agent_handle.abort();
                let _ = finalize_open_task_steps(state, task.id, "cancelled").await;
                state.metrics.task_finished(session_id, task.id, false);
                return Err((task.id, ApiError::BadRequest("task cancelled".to_string())));
            }
            event = event_rx.recv() => match event {
                Some(event) => {
                    match &event {
                        ServerEvent::AgentPlanUpdated { plan, .. } => {
                            state.metrics.plan_updated(session_id, task.id, plan.len());
                            persist_task_plan(state, task.id, plan)
                                .await
                                .map_err(|error| (task.id, error))?;
                        }
                        ServerEvent::ToolStarted { tool_name, .. } => {
                            state.metrics.tool_started(session_id, task.id, tool_name);
                            let _ = update_task_step_status(
                                state,
                                task.id,
                                tool_name,
                                "running",
                                None,
                                None,
                            )
                            .await;
                        }
                        ServerEvent::ToolOutput {
                            tool_name, output, ..
                        } => {
                            let mut patch = json!({});
                            if tool_name == "filesystem.read" {
                                patch["workspace_context"] = Value::String(output.clone());
                            }
                            if !patch.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                                let _ = evohime_storage::merge_checkpoint(
                                    &state.pool,
                                    task.id,
                                    Some(1),
                                    &patch,
                                )
                                .await;
                            }
                            let _ = update_task_step_status(
                                state,
                                task.id,
                                tool_name,
                                "running",
                                Some(output.as_str()),
                                None,
                            )
                            .await;
                        }
                        ServerEvent::ToolCompleted {
                            tool_name, success, ..
                        } => {
                            state
                                .metrics
                                .tool_completed(session_id, task.id, tool_name, *success);
                            let status = if *success { "completed" } else { "failed" };
                            let _ = update_task_step_status(
                                state,
                                task.id,
                                tool_name,
                                status,
                                None,
                                None,
                            )
                            .await;
                        }
                        ServerEvent::TaskCompleted { .. } => {
                            let _ = finalize_open_task_steps(state, task.id, "completed").await;
                        }
                        ServerEvent::TaskFailed { .. } => {
                            let _ = finalize_open_task_steps(state, task.id, "failed").await;
                        }
                        _ => {}
                    }
                    emit_event(state, session_id, Some(task.id), event).await?;
                }
                None => break,
            }
        }
    }

    let agent_result = tokio::select! {
        _ = cancellation.cancelled() => {
            agent_handle.abort();
            return Err((task.id, ApiError::BadRequest("task cancelled".to_string())));
        }
        result = &mut agent_handle => result
    };

    let agent_result = match agent_result {
        Ok(result) => match result {
            Ok(result) => result,
            Err(AgentError::Tool(ToolError::NeedsApproval {
                tool,
                permission,
                scope,
                approval_id,
            })) => {
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::ApprovalRequired {
                        approval_id,
                        task_id: task.id,
                        tool_name: tool.clone(),
                        permission: permission_name(permission).to_string(),
                        scope: scope.clone(),
                        created_at: chrono::Utc::now(),
                    },
                )
                .await?;
                state
                    .metrics
                    .approval_requested(session_id, task.id, approval_id, &tool);
                let _ = pause_task(&state.pool, task.id).await;
                let _ = evohime_storage::merge_checkpoint(
                    &state.pool,
                    task.id,
                    None,
                    &json!({
                        "pause_reason": "approval_required",
                        "approval_wait": {
                            "approval_id": approval_id,
                            "tool_name": tool,
                            "permission": permission_name(permission),
                            "scope": scope,
                        }
                    }),
                )
                .await;
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::TaskStatusChanged {
                        task_id: task.id,
                        status: "paused".to_string(),
                    },
                )
                .await?;
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::ActionLogged {
                        task_id: task.id,
                        action: "approval.required".into(),
                        detail: format!(
                            "Waiting for approval on {} ({}) in scope {}",
                            tool,
                            permission_name(permission),
                            scope
                        ),
                        created_at: chrono::Utc::now(),
                    },
                )
                .await?;
                return Ok(());
            }
            Err(error) => {
                let err_msg = error.to_string();
                state.metrics.task_finished(session_id, task.id, false);
                apply_task_memory_feedback(state, session_id, task.id, &used_memory_ids, false).await;
                persist_structured_memory(
                    state,
                    &gateway,
                    session_id,
                    &task,
                    &workspace_scope,
                    &err_msg,
                    false,
                )
                .await;
                return Err((task.id, map_agent_error(error)));
            }
        },
        Err(error) => {
            state.metrics.task_finished(session_id, task.id, false);
            return Err((task.id, ApiError::Internal(error.to_string())));
        }
    };

    evohime_storage::insert_message(
        &state.pool,
        session_id,
        Some(task.id),
        "assistant",
        &agent_result.final_message,
    )
    .await
    .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

    let memory_note = summarize_task_memory(&task.user_message, &agent_result.final_message);
    evohime_storage::insert_session_memory(&state.pool, session_id, Some(task.id), &memory_note)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    evohime_storage::insert_global_memory(
        &state.pool,
        &workspace_scope,
        Some(task.id),
        &memory_note,
    )
    .await
    .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

    apply_task_memory_feedback(state, session_id, task.id, &used_memory_ids, true).await;

    persist_structured_memory(
        state,
        &gateway,
        session_id,
        &task,
        &workspace_scope,
        &agent_result.final_message,
        true,
    )
    .await;

    complete_task(&state.pool, task.id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

    state.metrics.task_finished(session_id, task.id, true);

    Ok(())
}

async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<evohime_storage::TaskRow>>, ApiError> {
    evohime_storage::list_tasks(&state.pool, None)
        .await
        .map(Json)
        .map_err(|error| ApiError::Internal(error.to_string()))
}

async fn load_chat_history(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<ChatMessage>, evohime_storage::StorageError> {
    let rows = evohime_storage::list_session_messages(pool, session_id).await?;
    let mut messages = Vec::with_capacity(rows.len());

    for row in rows {
        let role = match row.role.as_str() {
            "system" => ChatRole::System,
            "assistant" => ChatRole::Assistant,
            _ => ChatRole::User,
        };
        messages.push(ChatMessage {
            role,
            content: row.content,
        });
    }

    Ok(messages)
}

fn map_agent_error(error: AgentError) -> ApiError {
    ApiError::Internal(error.to_string())
}

async fn load_memory_notes(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<String>, evohime_storage::StorageError> {
    Ok(evohime_storage::list_session_memory(pool, session_id)
        .await?
        .into_iter()
        .map(|row| row.note)
        .collect())
}

fn summarize_task_memory(user_message: &str, final_message: &str) -> String {
    const LIMIT: usize = 400;
    let summary = format!(
        "User asked: {}; assistant replied: {}",
        user_message.trim(),
        final_message.trim()
    );
    summary.chars().take(LIMIT).collect()
}

const MEMORY_EXTRACT_PROMPT: &str = r#"Extract durable memory candidates from this completed task.
Return ONLY a JSON array (no markdown, no prose). Each object:
{"scope":"session|workspace|project|global|experience","kind":"fact|preference|constraint|failure_pattern|success_pattern|verification_rule|playbook","content":"...","confidence":0.0-1.0,"importance":0.0-1.0,"pinned":false,"playbook":{"trigger":"...","steps":["..."],"verify":"...","rollback_hint":"..."}}
Rules:
- Prefer workspace/session facts and preferences for ordinary notes.
- For reusable how-to / avoid / verify knowledge use scope=experience with success_pattern, failure_pattern, verification_rule, or playbook.
- Playbooks MUST include playbook{trigger,steps,verify?,rollback_hint?} (content may be empty; it will be derived).
- Use global/pinned/constraint only when clearly standing operator policy.
- Never include secrets, tokens, passwords, or private keys.
- Max 5 items. Empty array [] if nothing worth remembering."#;

fn scope_key_for(
    scope: evohime_storage::MemoryScope,
    session_id: Uuid,
    workspace_scope: &str,
) -> String {
    match scope {
        evohime_storage::MemoryScope::Session => session_id.to_string(),
        evohime_storage::MemoryScope::Workspace | evohime_storage::MemoryScope::Project => {
            workspace_scope.to_string()
        }
        evohime_storage::MemoryScope::Global | evohime_storage::MemoryScope::Experience => {
            evohime_storage::LOCAL_OPERATOR_SCOPE_KEY.to_string()
        }
    }
}

async fn collect_gateway_text(
    gateway: &ModelGateway,
    messages: &[ChatMessage],
    timeout: std::time::Duration,
) -> Option<String> {
    use futures_util::StreamExt;
    let stream = gateway.stream_chat(messages);
    let collect = async {
        let mut output = String::new();
        let mut stream = stream;
        while let Some(chunk) = stream.next().await {
            output.push_str(&chunk.ok()?);
        }
        Some(output)
    };
    tokio::time::timeout(timeout, collect)
        .await
        .ok()
        .flatten()
}

async fn llm_extract_memory_json(
    gateway: &ModelGateway,
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Option<String> {
    let status = if task_ok { "completed" } else { "failed" };
    let user = format!(
        "Task status: {status}\nUser message:\n{user_message}\n\nAssistant reply:\n{final_message}"
    );
    let messages = [
        ChatMessage {
            role: ChatRole::System,
            content: MEMORY_EXTRACT_PROMPT.to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: user,
        },
    ];
    collect_gateway_text(gateway, &messages, std::time::Duration::from_secs(20)).await
}

async fn apply_task_memory_feedback(
    state: &Arc<AppState>,
    session_id: Uuid,
    task_id: Uuid,
    used_memory_ids: &[Uuid],
    task_ok: bool,
) {
    if !used_memory_ids.is_empty() {
        let results = if task_ok {
            evohime_memory::record_memory_helpful(&state.pool, used_memory_ids, Some(task_id)).await
        } else {
            evohime_memory::record_memory_harmful(&state.pool, used_memory_ids, Some(task_id)).await
        };
        match results {
            Ok(applied) => {
                for item in applied {
                    let _ = emit_event(
                        state,
                        session_id,
                        Some(task_id),
                        ServerEvent::MemoryUsed {
                            memory_id: item.memory_id,
                            task_id,
                            signal: item.signal.as_str().to_string(),
                            confidence: item.row.confidence,
                        },
                    )
                    .await;
                }
            }
            Err(error) => {
                tracing::warn!(%task_id, %error, "memory feedback apply failed");
            }
        }
    }

    match evohime_memory::decay_unused_memory(
        &state.pool,
        evohime_memory::DEFAULT_IDLE_DAYS,
        evohime_memory::DEFAULT_IDLE_BATCH,
    )
    .await
    {
        Ok(decayed) if !decayed.is_empty() => {
            tracing::info!(
                %task_id,
                decayed = decayed.len(),
                "applied idle memory decay"
            );
        }
        Err(error) => {
            tracing::warn!(%task_id, %error, "idle memory decay failed");
        }
        _ => {}
    }
}

async fn persist_structured_memory(
    state: &Arc<AppState>,
    gateway: &ModelGateway,
    session_id: Uuid,
    task: &evohime_storage::TaskRow,
    workspace_scope: &str,
    final_message: &str,
    task_ok: bool,
) {
    let llm_raw =
        llm_extract_memory_json(gateway, &task.user_message, final_message, task_ok).await;
    let candidates = evohime_memory::extract_candidates(
        llm_raw.as_deref(),
        &task.user_message,
        final_message,
        task_ok,
    );

    for (index, candidate) in candidates.into_iter().enumerate() {
        let scope = candidate.scope;
        let scope_key = scope_key_for(scope, session_id, workspace_scope);
        let item = candidate.into_new_item(
            scope_key,
            Some(session_id),
            Some(task.id),
            format!("extract:{}:{}", task.id, index),
        );

        let outcome = match evohime_memory::admit_memory_item(&state.pool, item).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!(task_id = %task.id, %error, "memory admit failed");
                continue;
            }
        };

        let (decision, row) = evohime_memory::gate_after_admit(&outcome);
        let Some(row) = row else {
            continue;
        };

        let _ = emit_event(
            state,
            session_id,
            Some(task.id),
            ServerEvent::MemoryProposed {
                memory_id: row.id,
                task_id: task.id,
                scope: row.scope.clone(),
                kind: row.kind.clone(),
                content: row.content.clone(),
                confidence: row.confidence,
                status: row.status.clone(),
            },
        )
        .await;

        match decision {
            evohime_memory::GateDecision::AutoPromote => {
                match evohime_memory::promote_memory_item(&state.pool, row.id).await {
                    Ok(Some(_)) => {
                        let _ = emit_event(
                            state,
                            session_id,
                            Some(task.id),
                            ServerEvent::MemoryAccepted {
                                memory_id: row.id,
                                task_id: task.id,
                            },
                        )
                        .await;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(memory_id = %row.id, %error, "memory promote failed");
                    }
                }
            }
            evohime_memory::GateDecision::Ask { reason } => {
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::MemoryAsk {
                        memory_id: row.id,
                        task_id: task.id,
                        scope: row.scope.clone(),
                        kind: row.kind.clone(),
                        content: row.content.clone(),
                        confidence: row.confidence,
                        status: row.status.clone(),
                        reason,
                    },
                )
                .await;
            }
            evohime_memory::GateDecision::Drop { reason } => {
                tracing::debug!(memory_id = %row.id, %reason, "memory gate drop");
                let _ = evohime_memory::reject_memory_item(&state.pool, row.id).await;
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::MemoryRejected {
                        memory_id: row.id,
                        task_id: task.id,
                    },
                )
                .await;
            }
        }
    }
}

async fn handle_memory_decision(
    state: &Arc<AppState>,
    session_id: Uuid,
    memory_id: Uuid,
    accept: bool,
) {
    let existing = match evohime_storage::get_memory_item(&state.pool, memory_id).await {
        Ok(row) => row,
        Err(error) => {
            tracing::warn!(%memory_id, %error, "failed to load memory for decision");
            return;
        }
    };
    let Some(existing) = existing else {
        return;
    };
    let task_id = existing.source_task_id.unwrap_or(Uuid::nil());

    if accept {
        match evohime_memory::accept_memory_item(&state.pool, memory_id).await {
            Ok(Some(_)) => {
                let _ = evohime_memory::record_memory_corrected(
                    &state.pool,
                    memory_id,
                    existing.source_task_id,
                )
                .await;
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task_id),
                    ServerEvent::MemoryAccepted {
                        memory_id,
                        task_id,
                    },
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%memory_id, %error, "memory accept failed"),
        }
    } else {
        match evohime_memory::record_memory_rejected(
            &state.pool,
            memory_id,
            existing.source_task_id,
        )
        .await
        {
            Ok(Some(_)) => {
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task_id),
                    ServerEvent::MemoryRejected {
                        memory_id,
                        task_id,
                    },
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%memory_id, %error, "memory reject failed"),
        }
    }
}

fn resolve_model_route(model_route: Option<&str>, default_route: &str) -> String {
    model_route
        .map(|route| route.to_string())
        .unwrap_or_else(|| default_route.to_string())
}

fn resolve_workspace_path(
    state: &Arc<AppState>,
    requested_path: Option<String>,
) -> Result<PathBuf, ApiError> {
    let root = state.workspace_root.canonicalize().map_err(|error| {
        ApiError::Internal(format!("не удалось определить корень workspace: {error}"))
    })?;
    let projects_root = root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let requested = requested_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let candidate = if requested.is_absolute() {
        requested
    } else if requested.as_os_str() == "." {
        root.clone()
    } else {
        projects_root.join(requested)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|error| ApiError::BadRequest(format!("проект не найден: {error}")))?;
    if !resolved.starts_with(&projects_root) {
        return Err(ApiError::BadRequest(
            "проект должен находиться внутри workspace".to_string(),
        ));
    }
    if !resolved.is_dir() {
        return Err(ApiError::BadRequest(
            "путь проекта должен быть папкой".to_string(),
        ));
    }
    Ok(resolved)
}

async fn emit_event(
    state: &Arc<AppState>,
    session_id: Uuid,
    task_id: Option<Uuid>,
    event: ServerEvent,
) -> Result<(), (Uuid, ApiError)> {
    state
        .publish_event(session_id, task_id, event)
        .await
        .map_err(|error| {
            (
                task_id.unwrap_or(Uuid::nil()),
                ApiError::Internal(error.to_string()),
            )
        })?;
    Ok(())
}

fn build_agent_resume_context(
    checkpoint: Option<&evohime_storage::TaskCheckpointRow>,
    task_steps: &[evohime_storage::TaskStepRow],
) -> AgentResumeContext {
    let state = checkpoint.map(|row| &row.state_json);
    let workspace_context = state
        .and_then(|value| value.get("workspace_context"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let plan = state
        .and_then(|value| value.get("plan"))
        .and_then(|value| serde_json::from_value::<Vec<PlanStep>>(value.clone()).ok());
    let pause_reason = state
        .and_then(|value| value.get("pause_reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut completed_step_ids = Vec::new();
    let mut tool_results = Vec::new();
    for step in task_steps {
        let plan_step_id = step
            .input_json
            .get("plan_step_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if step.status == "completed" {
            if let Some(id) = &plan_step_id {
                completed_step_ids.push(id.clone());
            }
        }
        if let (Some(id), Some(output)) = (plan_step_id, step.output.as_ref()) {
            if !output.trim().is_empty() {
                tool_results.push(format!("{id} ({}):\n{output}", step.tool_name));
            }
        }
    }
    AgentResumeContext {
        workspace_context,
        plan,
        completed_step_ids,
        tool_results,
        pause_reason,
    }
}

async fn persist_task_plan(
    state: &Arc<AppState>,
    task_id: Uuid,
    plan: &[PlanStep],
) -> Result<(), ApiError> {
    if !evohime_storage::list_task_steps(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .is_empty()
    {
        return Ok(());
    }

    let existing_checkpoint = evohime_storage::load_checkpoint(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let workspace_context = existing_checkpoint
        .as_ref()
        .and_then(|checkpoint| {
            checkpoint
                .state_json
                .get("workspace_context")
                .and_then(|value| value.as_str())
        })
        .map(|value| value.to_string());

    let mut step_ids = HashMap::new();
    for (index, step) in plan.iter().enumerate() {
        let depends_on = step
            .depends_on
            .iter()
            .filter_map(|dependency| step_ids.get(dependency).copied())
            .collect::<Vec<_>>();
        let input = json!({
            "plan_step_id": step.id,
            "description": step.description,
            "tool_name": step.tool_name,
            "depends_on": step.depends_on,
        });
        let row = evohime_storage::create_task_step(
            &state.pool,
            task_id,
            index as i32,
            &step.tool_name,
            &input,
            &depends_on,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
        step_ids.insert(step.id.clone(), row.id);
        emit_task_step_changed(state, task_id, row.id, "pending", step.tool_name.as_str()).await?;
    }

    let checkpoint_state = match workspace_context {
        Some(workspace_context) => json!({
            "plan": plan,
            "workspace_context": workspace_context,
            "pause_reason": Value::Null,
            "approval_wait": Value::Null,
        }),
        None => json!({
            "plan": plan,
            "pause_reason": Value::Null,
            "approval_wait": Value::Null,
        }),
    };
    evohime_storage::merge_checkpoint(&state.pool, task_id, Some(0), &checkpoint_state)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    if let Some(first_step) = plan.first() {
        if first_step.tool_name == "filesystem.read" {
            if let Some(output) = checkpoint_state
                .get("workspace_context")
                .and_then(|value| value.as_str())
            {
                if let Some(step_id) = step_ids.get(&first_step.id).copied() {
                    evohime_storage::set_step_status(
                        &state.pool,
                        step_id,
                        "completed",
                        Some(output),
                        None,
                    )
                    .await
                    .map_err(|error| ApiError::Internal(error.to_string()))?;
                    emit_task_step_changed(
                        state,
                        task_id,
                        step_id,
                        "completed",
                        first_step.tool_name.as_str(),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

async fn update_task_step_status(
    state: &Arc<AppState>,
    task_id: Uuid,
    tool_name: &str,
    status: &str,
    output: Option<&str>,
    error: Option<&str>,
) -> Result<(), ApiError> {
    let steps = evohime_storage::list_task_steps(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let step = steps
        .iter()
        .find(|step| {
            step.tool_name == tool_name
                && match status {
                    "running" => step.status == "pending",
                    "completed" | "failed" | "cancelled" => {
                        step.status == "running" || step.status == "pending"
                    }
                    _ => true,
                }
        })
        .or_else(|| {
            steps.iter().find(|step| {
                step.tool_name == tool_name
                    && match status {
                        "running" => step.status == "running",
                        "completed" | "failed" | "cancelled" => true,
                        _ => true,
                    }
            })
        });

    if let Some(step) = step {
        evohime_storage::set_step_status(&state.pool, step.id, status, output, error)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        emit_task_step_changed(state, task_id, step.id, status, tool_name).await?;
    }

    Ok(())
}

async fn finalize_open_task_steps(
    state: &Arc<AppState>,
    task_id: Uuid,
    status: &str,
) -> Result<(), ApiError> {
    let steps = evohime_storage::list_task_steps(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    for step in steps
        .into_iter()
        .filter(|step| step.status == "pending" || step.status == "running")
    {
        evohime_storage::set_step_status(&state.pool, step.id, status, None, None)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        emit_task_step_changed(state, task_id, step.id, status, step.tool_name.as_str()).await?;
    }

    Ok(())
}

async fn emit_task_step_changed(
    state: &Arc<AppState>,
    task_id: Uuid,
    step_id: Uuid,
    status: &str,
    tool_name: &str,
) -> Result<(), ApiError> {
    let session_id = find_session_for_task(state, task_id).await?;
    emit_event(
        state,
        session_id,
        Some(task_id),
        ServerEvent::TaskStepChanged {
            task_id,
            step_id,
            status: status.to_string(),
            tool_name: tool_name.to_string(),
        },
    )
    .await
    .map_err(|(_, error)| error)
}

async fn find_session_for_task(state: &Arc<AppState>, task_id: Uuid) -> Result<Uuid, ApiError> {
    let task = evohime_storage::load_task(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("unknown task".to_string()))?;
    Ok(task.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn summarizes_task_memory() {
        let note = summarize_task_memory(
            "Find the project index slice and make it work",
            "Done. Project index and MCP bridge are in place.",
        );

        assert!(note.contains("User asked: Find the project index slice and make it work"));
        assert!(
            note.contains("assistant replied: Done. Project index and MCP bridge are in place.")
        );
    }

    #[test]
    fn merge_checkpoint_preserves_plan_when_patching_workspace() {
        let existing = json!({
            "plan": [{"id":"step-1","tool_name":"assistant.reply","description":"hi","depends_on":[]}],
            "workspace_context": "old",
        });
        let merged = evohime_storage::merge_checkpoint_state(
            &existing,
            &json!({ "workspace_context": "new" }),
        );
        assert_eq!(merged["workspace_context"], "new");
        assert!(merged.get("plan").is_some());
    }

    #[test]
    fn builds_resume_context_from_checkpoint_and_steps() {
        let checkpoint = evohime_storage::TaskCheckpointRow {
            task_id: Uuid::nil(),
            next_step: 1,
            state_json: json!({
                "workspace_context": "ctx",
                "plan": [{"id":"step-1","tool_name":"filesystem.read","description":"read","depends_on":[]}],
                "pause_reason": "approval_required",
            }),
            updated_at: chrono::Utc::now(),
        };
        let steps = vec![evohime_storage::TaskStepRow {
            id: Uuid::nil(),
            task_id: Uuid::nil(),
            step_index: 0,
            tool_name: "filesystem.read".into(),
            input_json: json!({"plan_step_id":"step-1"}),
            depends_on: vec![],
            status: "completed".into(),
            output: Some("file body".into()),
            error: None,
        }];
        let resume = build_agent_resume_context(Some(&checkpoint), &steps);
        assert_eq!(resume.workspace_context.as_deref(), Some("ctx"));
        assert_eq!(resume.completed_step_ids, vec!["step-1".to_string()]);
        assert_eq!(resume.pause_reason.as_deref(), Some("approval_required"));
        assert_eq!(resume.plan.as_ref().map(|p| p.len()), Some(1));
        assert!(resume.tool_results[0].contains("file body"));
    }

    #[test]
    fn resolves_model_route_with_default_fallback() {
        assert_eq!(resolve_model_route(Some("planner"), "default"), "planner");
        assert_eq!(resolve_model_route(None, "default"), "default");
    }

    #[test]
    fn carries_default_api_key_to_new_orchestrator_route() {
        let current = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::from([(
                "default".to_string(),
                ModelRouteConfig::literouter("", "https://api.literouter.com/v1", "deepseek:free"),
            )]),
        };
        let config = build_model_config(
            ModelSettingsRequest {
                default_route: "default".to_string(),
                routes: vec![
                    ModelRouteRequest {
                        name: "default".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: Some("lr_test_key".to_string()),
                        billing_mode: "free".to_string(),
                    },
                    ModelRouteRequest {
                        name: "orchestrator".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: Some("old_orchestrator_key".to_string()),
                        billing_mode: "free".to_string(),
                    },
                ],
            },
            &current,
        )
        .expect("model config is valid");

        assert_eq!(config.routes["default"].literouter.api_key, "lr_test_key");
        assert_eq!(
            config.routes["orchestrator"].literouter.api_key,
            "lr_test_key"
        );
    }

    #[test]
    fn preserves_saved_api_key_when_autosave_sends_empty_fields() {
        let current = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::from([(
                "default".to_string(),
                ModelRouteConfig::literouter(
                    "lr_saved_key",
                    "https://api.literouter.com/v1",
                    "deepseek:free",
                ),
            )]),
        };
        let config = build_model_config(
            ModelSettingsRequest {
                default_route: "default".to_string(),
                routes: vec![ModelRouteRequest {
                    name: "default".to_string(),
                    provider: "literouter".to_string(),
                    model: "deepseek:free".to_string(),
                    base_url: "https://api.literouter.com/v1".to_string(),
                    api_key: Some(String::new()),
                    billing_mode: "free".to_string(),
                }],
            },
            &current,
        )
        .expect("model config is valid");

        assert_eq!(config.routes["default"].literouter.api_key, "lr_saved_key");
    }
}
