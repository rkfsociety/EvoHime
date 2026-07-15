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
use evohime_agent_runtime::{run_agent_loop, AgentConfig, AgentError};
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_model_gateway::ModelGateway;
use evohime_protocol::{ClientCommand, HistoryItem, ServerEvent, SessionBootstrap};
use evohime_task_engine::{complete_task, fail_task, start_task};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde_json::{json, Value};
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
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
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

    let state = Arc::new(AppState {
        pool,
        demo_file_path: config.demo_file_path.clone(),
        workspace_root: config.workspace_root.clone(),
        tools: evohime_tool_runtime::ToolRegistry::bootstrap(),
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
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/models/config", get(model_config))
        .route("/api/sessions", post(create_session))
        .route("/api/sessions/:session_id/history", get(session_history))
        .route("/api/files", get(workspace::list_files))
        .route("/api/files/content", get(workspace::read_file).put(workspace::save_file))
        .route("/api/git/status", get(workspace::git_status))
        .route("/api/git/diff", get(workspace::git_diff))
        .route("/api/tasks", get(list_tasks))
        .route("/ws/:session_id", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

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

async fn model_config(State(state): State<Arc<AppState>>) -> Json<evohime_model_gateway::ModelConfigResponse> {
    Json(state.model_config_response())
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
            if sender.send(Message::Text(serialized.into())).await.is_err() {
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
                            state.task_cancellations.lock().await.insert(task_id, token.clone());
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
                                if let Err((task_id, error)) = process_user_message(&state_for_task, session_id, task, token).await {
                                    let _ = emit_event(&state_for_task, session_id, Some(task_id), ServerEvent::TaskFailed { task_id, error: error.to_string() }).await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                state_for_task.task_cancellations.lock().await.remove(&task_id);
                            });
                        }
                        ClientCommand::TaskCancel { task_id } => {
                            let cancellation = state.task_cancellations.lock().await.get(&task_id).cloned();
                            if let Some(token) = cancellation { token.cancel(); }
                            let _ = evohime_task_engine::cancel_task(&state.pool, task_id).await;
                            emit_event(&state, session_id, Some(task_id), ServerEvent::TaskStatusChanged { task_id, status: "cancelled".to_string() }).await?;
                            emit_event(&state, session_id, Some(task_id), ServerEvent::ActionLogged { task_id, action: "task.cancel".to_string(), detail: "Task cancellation requested".to_string(), created_at: chrono::Utc::now() }).await?;
                        }
                        ClientCommand::TaskResume { task_id } => {
                            let _ = evohime_task_engine::resume_task(&state.pool, task_id).await;
                            emit_event(&state, session_id, Some(task_id), ServerEvent::TaskStatusChanged { task_id, status: "running".to_string() }).await?;
                            emit_event(&state, session_id, Some(task_id), ServerEvent::ActionLogged { task_id, action: "task.resume".to_string(), detail: "Task resumed from checkpoint".to_string(), created_at: chrono::Utc::now() }).await?;
                        }
                        ClientCommand::TaskRetry { task_id } => {
                            let _ = evohime_task_engine::retry_task(&state.pool, task_id).await;
                            emit_event(&state, session_id, Some(task_id), ServerEvent::TaskStatusChanged { task_id, status: "running".to_string() }).await?;
                            emit_event(&state, session_id, Some(task_id), ServerEvent::ActionLogged { task_id, action: "task.retry".to_string(), detail: "Failed task scheduled for retry".to_string(), created_at: chrono::Utc::now() }).await?;
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
    let gateway = state.model_gateway.clone().ok_or_else(|| {
        (
            task.id,
            ApiError::Internal(
                "LITEROUTER_API_KEY is not configured — set it in .env".to_string(),
            ),
        )
    })?;

    let prior_messages = load_chat_history(&state.pool, session_id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

    evohime_storage::insert_message(
        &state.pool,
        session_id,
        Some(task.id),
        "user",
        &task.user_message,
    )
    .await
    .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

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
    let agent_handle = tokio::spawn(async move {
        run_agent_loop(agent_config, &gateway, &tools, prior_messages, event_tx).await
    });

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                agent_handle.abort();
                return Err((task.id, ApiError::BadRequest("task cancelled".to_string())));
            }
            event = event_rx.recv() => match event {
                Some(event) => emit_event(state, session_id, Some(task.id), event).await?,
                None => break,
            }
        }
    }

    let agent_result = tokio::select! {
        _ = cancellation.cancelled() => {
            agent_handle.abort();
            return Err((task.id, ApiError::BadRequest("task cancelled".to_string())));
        }
        result = agent_handle => result
    }
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?
        .map_err(|error| (task.id, map_agent_error(error)))?;

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
    evohime_storage::list_tasks(&state.pool, None).await
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
        .map_err(|error| (task_id.unwrap_or(Uuid::nil()), ApiError::Internal(error.to_string())))?;
    Ok(())
}
