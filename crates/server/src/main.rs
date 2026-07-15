mod app;
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
use serde_json::{json, to_value, Value};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
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
}

#[derive(Debug, Deserialize, Serialize)]
struct ModelSettingsRequest {
    default_route: String,
    routes: Vec<ModelRouteRequest>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ModelRouteRequest {
    name: String,
    provider: String,
    model: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
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
        demo_file_path: config.demo_file_path.clone(),
        workspace_root: config.workspace_root.clone(),
        tools: evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions.clone()),
        permissions,
        model_gateway: Arc::new(RwLock::new(model_gateway)),
        model_config: Arc::new(RwLock::new(active_model_config)),
        mcp_servers: Arc::new(Mutex::new(config.mcp_servers.clone())),
        session_buses: Arc::new(Mutex::new(HashMap::new())),
        task_cancellations: Arc::new(Mutex::new(HashMap::new())),
    });

    let recovered = evohime_task_engine::recover_after_restart(&state.pool)
        .await
        .context("recover tasks after restart")?;
    if !recovered.is_empty() {
        info!(count = recovered.len(), "tasks marked paused for recovery");
        for task in recovered {
            let task_id = task.id;
            let session_id = task.session_id;
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
        .route("/api/models/config", get(model_config).put(update_model_config))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/archived", get(list_archived_sessions))
        .route("/api/sessions/:session_id", delete(delete_session))
        .route("/api/sessions/:session_id/archive", post(archive_session))
        .route("/api/sessions/:session_id/history", get(session_history))
        .route("/api/auth/github", get(github_auth))
        .route("/api/github/pull-requests", get(list_pull_requests))
        .route("/api/files", get(workspace::list_files))
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

async fn model_config(
    State(state): State<Arc<AppState>>,
) -> Json<evohime_model_gateway::ModelConfigResponse> {
    Json(state.model_config_response().await)
}

async fn update_model_config(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ModelSettingsRequest>,
) -> Result<Json<evohime_model_gateway::ModelConfigResponse>, ApiError> {
    let current = state.model_config.read().await;
    let request_value = serde_json::to_value(&request).map_err(|error| ApiError::Internal(error.to_string()))?;
    let config = build_model_config(request, &current)?;
    drop(current);
    let gateway = if config.routes.values().all(ModelRouteConfig::configured) {
        Some(Arc::new(
            ModelGateway::from_config(&config)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        ))
    } else {
        None
    };
    evohime_storage::save_setting(&state.pool, "model_config", &request_value)
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
        let existing_key = current
            .routes
            .get(&name)
            .map(|item| item.literouter.api_key.clone())
            .unwrap_or_default();
        let api_key = route
            .api_key
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(existing_key);
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
        return Err(ApiError::BadRequest("Чат не найден или уже архивирован".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Serialize)]
struct SessionSummary {
    session_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message: Option<String>,
    last_role: Option<String>,
}

fn session_summary(row: evohime_storage::SessionSummaryRow) -> SessionSummary {
    SessionSummary {
        session_id: row.id,
        created_at: row.created_at,
        last_message_at: row.last_message_at,
        last_message: row.last_message,
        last_role: row.last_role,
    }
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

    let prs = result.map_err(|error| ApiError::Internal(error))?;
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
                        } => {
                            let task = match start_task(
                                &state.pool,
                                session_id,
                                &content,
                                model_route.as_deref(),
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

    if emit_started {
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

    let resume_context = checkpoint.and_then(|row| {
        row.state_json
            .get("workspace_context")
            .and_then(Value::as_str)
            .map(|value| AgentResumeContext {
                workspace_context: Some(value.to_string()),
            })
    });

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let default_route = state.model_config.read().await.default_route.clone();
    let model_route = resolve_model_route(task.model_route.as_deref(), &default_route);
    let agent_config = AgentConfig {
        task_id: task.id,
        session_id,
        user_message: task.user_message.clone(),
        created_at: task.created_at,
        demo_file_path: state.demo_file_path.clone(),
        workspace_root: state.workspace_root.clone(),
        model_route,
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
                    AgentResumeContext {
                        workspace_context: None,
                    },
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
                            if tool_name == "filesystem.read" {
                                let checkpoint_state = json!({"workspace_context": output});
                                let _ = evohime_storage::upsert_checkpoint(
                                    &state.pool,
                                    task.id,
                                    1,
                                    &checkpoint_state,
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
        }),
        None => json!({
            "plan": plan,
        }),
    };
    evohime_storage::upsert_checkpoint(&state.pool, task_id, 0, &checkpoint_state)
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
    fn resolves_model_route_with_default_fallback() {
        assert_eq!(resolve_model_route(Some("planner"), "default"), "planner");
        assert_eq!(resolve_model_route(None, "default"), "default");
    }
}
