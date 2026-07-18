//! WebSocket session handler.
use crate::app::AppState;
use crate::permissions_api::persist_permission_scopes;
use crate::task::{
    emit_event, finalize_open_task_steps, handle_memory_decision, process_user_message,
    replace_task_plan, resolve_workspace_path, resume_task_run,
};
use crate::ApiError;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use evohime_protocol::{ClientCommand, HistoryItem, ServerEvent};
use evohime_task_engine::validate_plan;
use evohime_task_engine::{fail_task, resume_task, retry_task, start_task};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(crate) struct WsConnectQuery {
    after_sequence: Option<i64>,
}

pub(crate) async fn ws_handler(
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

pub(crate) async fn handle_socket(
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
    let backlog =
        evohime_storage::list_session_events_after(&state.pool, session_id, after_sequence)
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
                        ClientCommand::TaskPlanApprove { task_id, plan } => {
                            let task = match evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                            {
                                Some(task) if task.session_id == session_id => task,
                                _ => continue,
                            };
                            let checkpoint = evohime_storage::load_checkpoint(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?;
                            let pending = task.status == "paused"
                                && checkpoint.as_ref().and_then(|row| {
                                    row.state_json.get("pause_reason").and_then(Value::as_str)
                                }) == Some("plan_approval_required");
                            if !pending {
                                emit_event(
                                    &state,
                                    session_id,
                                    Some(task_id),
                                    ServerEvent::ActionLogged {
                                        task_id,
                                        action: "plan.approval.invalid".to_string(),
                                        detail: "Plan approval ignored because the task is not awaiting approval".to_string(),
                                        created_at: chrono::Utc::now(),
                                    },
                                )
                                .await?;
                                continue;
                            }
                            if let Err(error) = validate_plan(&plan) {
                                emit_event(
                                    &state,
                                    session_id,
                                    Some(task_id),
                                    ServerEvent::ActionLogged {
                                        task_id,
                                        action: "plan.approval.invalid".to_string(),
                                        detail: format!("Invalid plan: {error}"),
                                        created_at: chrono::Utc::now(),
                                    },
                                )
                                .await?;
                                continue;
                            }
                            replace_task_plan(&state, task_id, &plan).await?;
                            resume_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?;
                            evohime_storage::merge_checkpoint(
                                &state.pool,
                                task_id,
                                None,
                                &json!({
                                    "pause_reason": Value::Null,
                                    "approval_wait": Value::Null,
                                }),
                            )
                            .await
                            .map_err(|error| ApiError::Internal(error.to_string()))?;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::AgentPlanUpdated { task_id, plan },
                            )
                            .await?;
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
                                    action: "plan.approval.granted".to_string(),
                                    detail: "Approved plan scheduled for execution".to_string(),
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
                        ClientCommand::TaskPlanReject { task_id } => {
                            let task = match evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                            {
                                Some(task) if task.session_id == session_id => task,
                                _ => continue,
                            };
                            let checkpoint = evohime_storage::load_checkpoint(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?;
                            let pending = task.status == "paused"
                                && checkpoint.as_ref().and_then(|row| {
                                    row.state_json.get("pause_reason").and_then(Value::as_str)
                                }) == Some("plan_approval_required");
                            if !pending {
                                continue;
                            }
                            evohime_task_engine::cancel_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?;
                            finalize_open_task_steps(&state, task_id, "cancelled").await?;
                            evohime_storage::merge_checkpoint(
                                &state.pool,
                                task_id,
                                None,
                                &json!({
                                    "pause_reason": "plan_rejected",
                                    "approval_wait": Value::Null,
                                }),
                            )
                            .await
                            .map_err(|error| ApiError::Internal(error.to_string()))?;
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
                                    action: "plan.approval.rejected".to_string(),
                                    detail: "Plan rejected by user; task cancelled".to_string(),
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
