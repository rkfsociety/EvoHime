mod app;
mod workspace;

use anyhow::Context;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use evohime_agent_runtime::{
    run_agent_loop, run_agent_loop_resumed, AgentConfig, AgentError, AgentResumeContext,
};
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_model_gateway::ModelGateway;
use evohime_permissions::{Permission, PermissionMode};
use evohime_protocol::{ClientCommand, HistoryItem, PlanStep, ServerEvent, SessionBootstrap};
use evohime_task_engine::{
    complete_task, fail_task, pause_task, resume_task, retry_task, start_task,
};
use evohime_tool_runtime::ToolError;
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde_json::{json, to_value, Value};
use sqlx::PgPool;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::app::{AppConfig, AppState};

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("approval required for {tool}: {approval_id}")]
    ApprovalRequired { tool: String, approval_id: Uuid },
    #[error("{0}")]
    Internal(String),
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

    let model_gateway = if config.model_config.literouter.api_key.is_empty() {
        warn!("LITEROUTER_API_KEY is not set — LLM requests will fail until configured");
        None
    } else {
        Some(Arc::new(
            ModelGateway::from_config(&config.model_config).context("init model gateway")?,
        ))
    };

    let permissions = evohime_permissions::PermissionEngine::new();
    let state = Arc::new(AppState {
        pool,
        demo_file_path: config.demo_file_path.clone(),
        workspace_root: config.workspace_root.clone(),
        tools: evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions.clone()),
        permissions,
        model_gateway,
        model_config: config.model_config.clone(),
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
        .route("/api/models/config", get(model_config))
        .route("/api/sessions", post(create_session))
        .route("/api/sessions/:session_id/history", get(session_history))
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
        .route("/ws/:session_id", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let addr: SocketAddr = config.bind_addr.parse().context("parse bind address")?;
    info!(
        workspace_root = %config.workspace_root.display(),
        demo_file = %config.demo_file_path.display(),
        model = %config.model_config.literouter.model,
        provider = %config.model_config.provider.as_str(),
        llm_configured = %state.model_gateway.is_some(),
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
    Json(state.model_config_response())
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
                        ClientCommand::UserMessage { content } => {
                            let task = match start_task(&state.pool, session_id, &content).await {
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
    let gateway = state.model_gateway.clone().ok_or_else(|| {
        (
            task.id,
            ApiError::Internal("LITEROUTER_API_KEY is not configured — set it in .env".to_string()),
        )
    })?;

    let prior_messages = load_chat_history(&state.pool, session_id)
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
    let agent_config = AgentConfig {
        task_id: task.id,
        session_id,
        user_message: task.user_message.clone(),
        created_at: task.created_at,
        demo_file_path: state.demo_file_path.clone(),
        workspace_root: state.workspace_root.clone(),
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
                    event_tx,
                    resume,
                )
                .await
            }
            None if emit_started => {
                run_agent_loop(agent_config, &gateway, &tools, prior_messages, event_tx).await
            }
            None => {
                run_agent_loop_resumed(
                    agent_config,
                    &gateway,
                    &tools,
                    prior_messages,
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
