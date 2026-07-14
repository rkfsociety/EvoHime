mod app;

use anyhow::Context;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use evohime_protocol::{ClientCommand, HistoryItem, ServerEvent, SessionBootstrap};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde_json::{json, to_value, Value};
use sqlx::PgPool;
use std::{net::SocketAddr, sync::Arc};
use tokio::fs;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
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

    let config = AppConfig::from_env();
    let pool = PgPool::connect(&config.database_url)
        .await
        .context("connect to postgres")?;

    evohime_storage::run_migrations(&pool)
        .await
        .context("run migrations")?;

    let state = Arc::new(AppState {
        pool,
        demo_file_path: config.demo_file_path.clone(),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/sessions", post(create_session))
        .route("/api/sessions/:session_id/history", get(session_history))
        .route("/ws/:session_id", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = config.bind_addr.parse().context("parse bind address")?;
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
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

    let (mut sender, mut receiver) = socket.split();
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
                        let task = match evohime_storage::create_task(&state.pool, session_id, &content).await {
                            Ok(task) => task,
                            Err(error) => {
                                error!("failed to create task: {error}");
                                continue;
                            }
                        };

                        if let Err((task_id, error)) =
                            process_user_message(&state, session_id, task, content, &mut sender).await
                        {
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                &ServerEvent::TaskFailed {
                                    task_id,
                                    error: error.to_string(),
                                },
                                &mut sender,
                            )
                            .await?;
                            let _ = evohime_storage::fail_task(&state.pool, task_id).await;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
        }
    }

    Ok(())
}

async fn process_user_message(
    state: &Arc<AppState>,
    session_id: Uuid,
    task: evohime_storage::TaskRow,
    content: String,
    sender: &mut (impl futures_util::sink::Sink<Message, Error = axum::Error> + Unpin),
) -> Result<(), (Uuid, ApiError)> {
    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::TaskStarted {
            task_id: task.id,
            session_id,
            user_message: content,
            created_at: task.created_at,
        },
        sender,
    )
    .await?;

    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::AgentPlanUpdated {
            task_id: task.id,
            plan: vec!["read demo file".to_string(), "build a short response".to_string()],
        },
        sender,
    )
    .await?;

    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::AgentMessageDelta {
            task_id: task.id,
            delta: "Received your message. ".to_string(),
        },
        sender,
    )
    .await?;

    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::ToolStarted {
            task_id: task.id,
            tool_name: "filesystem.read".to_string(),
        },
        sender,
    )
    .await?;

    let file_path = state.demo_file_path.clone();
    let file_content = fs::read_to_string(&file_path)
        .await
        .map_err(|error| {
            (
                task.id,
                ApiError::Internal(format!("filesystem.read failed: {error}")),
            )
        })?;

    let output = truncate_for_display(&file_content);
    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::ToolOutput {
            task_id: task.id,
            tool_name: "filesystem.read".to_string(),
            output: output.clone(),
        },
        sender,
    )
    .await?;

    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::ToolCompleted {
            task_id: task.id,
            tool_name: "filesystem.read".to_string(),
            success: true,
        },
        sender,
    )
    .await?;

    let final_message = format!(
        "I read `{}` and used it as context. Short summary: {}",
        file_path.display(),
        first_sentence(&output)
    );

    for chunk in chunk_text(&final_message) {
        emit_event(
            state,
            session_id,
            Some(task.id),
            &ServerEvent::AgentMessageDelta {
                task_id: task.id,
                delta: chunk,
            },
            sender,
        )
        .await?;
    }

    emit_event(
        state,
        session_id,
        Some(task.id),
        &ServerEvent::TaskCompleted {
            task_id: task.id,
            final_message: final_message.clone(),
            completed_at: Utc::now(),
        },
        sender,
    )
    .await?;

    evohime_storage::complete_task(&state.pool, task.id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;

    Ok(())
}

async fn emit_event(
    state: &Arc<AppState>,
    session_id: Uuid,
    task_id: Option<Uuid>,
    event: &ServerEvent,
    sender: &mut (impl futures_util::sink::Sink<Message, Error = axum::Error> + Unpin),
) -> Result<(), (Uuid, ApiError)> {
    let event_json = to_value(event).map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::insert_event(&state.pool, session_id, &event_json, task_id)
        .await
        .map_err(|error| (task_id.unwrap_or(Uuid::nil()), ApiError::Internal(error.to_string())))?;

    let serialized = serde_json::to_string(event).map_err(|error| ApiError::Internal(error.to_string()))?;
    sender
        .send(Message::Text(serialized.into()))
        .await
        .map_err(|error| (task_id.unwrap_or(Uuid::nil()), ApiError::Internal(error.to_string())))?;

    Ok(())
}

fn truncate_for_display(content: &str) -> String {
    let mut lines = content.lines().take(8).collect::<Vec<_>>().join("\n");
    if content.lines().count() > 8 {
        lines.push_str("\n...");
    }
    lines
}

fn first_sentence(content: &str) -> String {
    content
        .split(|ch| ch == '.' || ch == '\n')
        .find(|part| !part.trim().is_empty())
        .map(|part| part.trim().to_string())
        .unwrap_or_else(|| "context is empty".to_string())
}

fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();
    while start < chars.len() {
        let end = (start + 20).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        start = end;
    }
    chunks
}

