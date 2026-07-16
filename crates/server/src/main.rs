mod app;
mod worker;
mod workspace;

use anyhow::Context;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
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
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::app::{AppConfig, AppState, McpServerConfig};

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("approval required for {tool}: {approval_id}")]
    ApprovalRequired { tool: String, approval_id: Uuid },
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
            Self::ApprovalRequired { tool, approval_id } => (
                StatusCode::CONFLICT,
                format!("approval required for {tool}: {approval_id}"),
            ),
            Self::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
            Self::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,evohime_server=info".into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    let pool = PgPool::connect(&config.database_url)
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
        tools: evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions.clone()),
        permissions,
        model_gateway: Arc::new(RwLock::new(model_gateway)),
        model_config: Arc::new(RwLock::new(active_model_config)),
        mcp_servers: Arc::new(Mutex::new(config.mcp_servers.clone())),
        session_buses: Arc::new(Mutex::new(HashMap::new())),
        task_cancellations: Arc::new(Mutex::new(HashMap::new())),
        worker: worker::WorkerClient::new(config.worker_url.clone())?,
        worker_job_stall: config.worker_job_stall,
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
    if let Some(value) = evohime_storage::load_setting(&state.pool, "mcp_servers").await? {
        if let Ok(servers) = serde_json::from_value::<Vec<McpServerConfig>>(value) {
            *state.mcp_servers.lock().await = servers;
        }
    }

    let recovered = evohime_task_engine::recover_after_restart(&state.pool)
        .await
        .context("recover tasks after restart")?;
    if !recovered.is_empty() {
        info!(count = recovered.len(), "tasks marked paused for recovery");
        for task in recovered {
            let task_id = task.id;
            let session_id = task.session_id;
            let _ = evohime_storage::merge_checkpoint(
                &state.pool,
                task_id,
                None,
                &json!({ "pause_reason": "server_restart" }),
            )
            .await;
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
                    detail: "Task restored after server restart".to_string(),
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
        .route("/api/github/pull-requests", get(list_pull_requests))
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
        .route("/api/worker/jobs", post(create_worker_job))
        .route("/api/worker/jobs/:job_id", get(get_worker_job))
        .route("/api/worker/jobs/:job_id/retry", post(retry_worker_job))
        .route("/api/permissions", get(list_permissions))
        .route("/api/permissions/:permission", put(update_permission))
        .route("/api/tools", get(list_tools))
        .route(
            "/api/mcp/servers",
            get(list_mcp_servers).put(update_mcp_servers),
        )
        .route("/ws/:session_id", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let addr: SocketAddr = config.bind_addr.parse().context("parse bind address")?;
    info!(
        workspace_root = %config.workspace_root.display(),
        demo_file = %config.demo_file_path.display(),
        model = %default_model_name,
        provider = %default_provider_name,
        llm_configured = %state.model_gateway.read().await.is_some(),
        "listening on {}",
        addr
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[derive(Debug, Deserialize)]
struct WorkerJobRequest {
    task: String,
    #[serde(default)]
    payload: Value,
}

async fn create_worker_job(
    State(state): State<Arc<AppState>>,
    Json(request): Json<WorkerJobRequest>,
) -> Result<(StatusCode, Json<evohime_storage::WorkerJobRow>), ApiError> {
    if let Err(error) = worker::validate_task_payload(&request.task, &request.payload) {
        return Err(ApiError::BadRequest(error));
    }
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
        match retry_worker_job_after_error(&state, job.id, "server restart recovery".to_string())
            .await
        {
            Ok(Some(worker_job)) => {
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
            Ok(None) | Err(_) => {}
        }
    });
}

async fn run_worker_job(
    state: &AppState,
    id: Uuid,
    mut worker_job: worker::WorkerJob,
) -> Result<(), String> {
    loop {
        for _ in 0..120 {
            if worker::is_terminal_status(&worker_job.status) {
                evohime_storage::complete_worker_job(
                    &state.pool,
                    id,
                    &worker_job.status,
                    worker_job.result.as_ref(),
                    worker_job.error.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?;
                return Ok(());
            }
            if worker_job.status == "running"
                && worker::heartbeat_is_stale(
                    worker_job.heartbeat_at.as_deref(),
                    chrono::Utc::now(),
                    state.worker_job_stall,
                )
            {
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
                    None => return Ok(()),
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            match state.worker.get(&worker_job.id).await {
                Ok(job) => worker_job = job,
                Err(error) => {
                    match retry_worker_job_after_error(state, id, error.to_string()).await? {
                        Some(job) => worker_job = job,
                        None => return Ok(()),
                    }
                }
            }
        }
        match retry_worker_job_after_error(state, id, "worker polling timed out".to_string())
            .await?
        {
            Some(job) => worker_job = job,
            None => return Ok(()),
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
    let session = evohime_storage::create_session(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let event = ServerEvent::SessionCreated {
        session_id: session.id,
        created_at: session.created_at,
    };
    let event_json = to_value(&event).map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::insert_event(&state.pool, session.id, &event_json, None)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(SessionBootstrap {
        session_id: session.id,
        created_at: session.created_at,
        events: vec![HistoryItem {
            sequence: 1,
            created_at: session.created_at,
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

async fn session_history(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Vec<HistoryItem>>, ApiError> {
    let rows = evohime_storage::list_session_events(&state.pool, session_id)
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

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_socket(state, session_id, socket).await {
            error!("websocket session failed: {error}");
        }
    })
}

async fn handle_socket(
    state: Arc<AppState>,
    session_id: Uuid,
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
    let forward_handle = tokio::spawn(async move {
        while let Ok(event) = bus_receiver.recv().await {
            let serialized = match serde_json::to_string(&event) {
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
                            let granted = true;
                            let status = state.permissions.resolve(approval_id, granted).await;
                            let task_id = state
                                .permissions
                                .approval(approval_id)
                                .await
                                .map(|(request, _)| request.task_id)
                                .unwrap_or(Uuid::nil());
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
                            let status = state.permissions.resolve(approval_id, false).await;
                            let task_id = state
                                .permissions
                                .approval(approval_id)
                                .await
                                .map(|(request, _)| request.task_id)
                                .unwrap_or(Uuid::nil());
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
    let gateway = state.model_gateway.read().await.clone().ok_or_else(|| {
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
    };

    let tools = state.tools.clone();
    let mut agent_handle = tokio::spawn(async move {
        match resume_context {
            Some(resume) => {
                run_agent_loop_resumed(
                    agent_config,
                    &gateway,
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
                    &gateway,
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
                    &gateway,
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
                return Err((task.id, ApiError::BadRequest("task cancelled".to_string())));
            }
            event = event_rx.recv() => match event {
                Some(event) => {
                    match &event {
                        ServerEvent::AgentPlanUpdated { plan, .. } => {
                            persist_task_plan(state, task.id, plan)
                                .await
                                .map_err(|error| (task.id, error))?;
                        }
                        ServerEvent::ToolStarted { tool_name, .. } => {
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
            Err(error) => return Err((task.id, map_agent_error(error))),
        },
        Err(error) => return Err((task.id, ApiError::Internal(error.to_string()))),
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

    complete_task(&state.pool, task.id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

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
