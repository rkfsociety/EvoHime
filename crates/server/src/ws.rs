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
        Extension, Path, Query, State, WebSocketUpgrade,
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
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(session_id): Path<Uuid>,
    Query(query): Query<WsConnectQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let after_sequence = query.after_sequence.unwrap_or(0).max(0);
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_socket(state, identity, session_id, after_sequence, socket).await
        {
            error!("websocket session failed: {error}");
        }
    })
}

pub(crate) async fn handle_socket(
    state: Arc<AppState>,
    identity: crate::auth::OperatorIdentity,
    session_id: Uuid,
    after_sequence: i64,
    socket: WebSocket,
) -> Result<(), ApiError> {
    if evohime_storage::load_session_for_operator(&state.pool, identity.id, session_id)
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
    let backlog = evohime_storage::list_session_events_after_for_operator(
        &state.pool,
        identity.id,
        session_id,
        after_sequence,
    )
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
                                        correlation_id: None,
                                        duration_ms: None,
                                    },
                                )
                                .await;
                                continue;
                            }
                            let workspace_path_buf = resolve_workspace_path(&state, workspace_path)?;
                            // Persist a stable public path so UI project matching works on Windows
                            // (canonicalize() otherwise yields `\\?\F:\...`).
                            let workspace_path =
                                crate::task::helpers::public_fs_path(&workspace_path_buf);
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
                            // Single lock acquisition: the "is another task already
                            // running" check and this task's own registration must not
                            // be split into two separate lock/unlock pairs, or two tasks
                            // starting in the same instant could both observe an empty
                            // map and both skip isolation (7.107).
                            let is_concurrent = {
                                let mut guard = state.task_cancellations.lock().await;
                                let is_concurrent = !guard.is_empty();
                                guard.insert(task_id, token.clone());
                                is_concurrent
                            };
                            // `is_concurrent` is a snapshot at this exact instant, not
                            // re-checked after the lock above is released. If the other
                            // task finishes between here and `provision_worktree` below,
                            // this task still isolates unnecessarily — extra overhead,
                            // never a correctness issue (the reverse — starting unisolated
                            // when isolation was actually needed — is what must never
                            // happen, and this ordering guarantees that direction is safe).
                            if is_concurrent {
                                if let Err(error) = crate::task::worktree::provision_worktree(
                                    &state,
                                    task_id,
                                    &workspace_path_buf,
                                )
                                .await
                                {
                                    error!(%task_id, %error, "failed to allocate isolated worktree for concurrent task");
                                    state.task_cancellations.lock().await.remove(&task_id);
                                    let _ = fail_task(&state.pool, task_id).await;
                                    let _ = emit_event(
                                        &state,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: format!(
                                                "failed to allocate isolated worktree: {error}"
                                            ),
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    continue;
                                }
                            }
                            let mut cancellation_guard =
                                crate::task::helpers::TaskCancellationGuard::new(
                                    state.clone(),
                                    task_id,
                                );
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
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                cancellation_guard.disarm();
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
                            });
                        }
                        ClientCommand::TaskCancel { task_id } => {
                            let was_paused = evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                                .map(|task| task.status == "paused")
                                .unwrap_or(false);
                            let cancellation =
                                state.task_cancellations.lock().await.get(&task_id).cloned();
                            if let Some(token) = cancellation {
                                token.cancel();
                            }
                            let cancel_result =
                                evohime_task_engine::cancel_task(&state.pool, task_id).await;
                            // Force-remove immediately only for a task that was
                            // `paused` (no live spawned future left to ever run
                            // Step 3's post-await cleanup on its own) — and only
                            // once the FSM transition actually landed it in
                            // `Cancelled`. A task that was `running` must NOT be
                            // force-removed here even on a successful cancel:
                            // `cancel_task`'s DB transition is immediate, but the
                            // spawned `process_user_message` future may still be
                            // actively writing to `primary_workspace_root` for
                            // some time after this call returns — removing the
                            // entry now, before that future actually stops, would
                            // let a new task start unisolated while the
                            // cancelled-but-not-yet-stopped one is still live in
                            // the same directory. For a `running` task, leave the
                            // entry in place and let that future's own eventual
                            // `release_task_cancellation_if_terminal` call (once
                            // `process_user_message` genuinely returns) remove it
                            // — that's the only moment the workspace is actually
                            // free again. `release_task_cancellation_if_terminal`
                            // still runs unconditionally below for every other
                            // case (task was already terminal, the FSM transition
                            // itself failed) via its own authoritative DB check.
                            if was_paused && cancel_result.is_ok() {
                                state.task_cancellations.lock().await.remove(&task_id);
                            } else {
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state, task_id,
                                )
                                .await;
                            }
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
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
                                        correlation_id: Some(task_id),
                                        duration_ms: None,
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
                                        correlation_id: Some(task_id),
                                        duration_ms: None,
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
                                },
                            )
                            .await?;
                            let token = CancellationToken::new();
                            state
                                .task_cancellations
                                .lock()
                                .await
                                .insert(task_id, token.clone());
                            let mut cancellation_guard =
                                crate::task::helpers::TaskCancellationGuard::new(
                                    state.clone(),
                                    task_id,
                                );
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
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                cancellation_guard.disarm();
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
                                },
                            )
                            .await?;

                            let token = CancellationToken::new();
                            state
                                .task_cancellations
                                .lock()
                                .await
                                .insert(task_id, token.clone());
                            let mut cancellation_guard =
                                crate::task::helpers::TaskCancellationGuard::new(
                                    state.clone(),
                                    task_id,
                                );
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
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                cancellation_guard.disarm();
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
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
                            // Precondition gate for the worktree teardown below
                            // (new in 7.107): only a task that is actually
                            // `failed` right now may have its worktree torn
                            // down. Without this check, a stale/invalid
                            // `TaskRetry` for a task that's genuinely still
                            // `running` or `paused` would destroy a worktree
                            // still in active use — `retry_task`'s own FSM
                            // check happens too late (after teardown) and its
                            // failure is silently ignored (`let _ = ...`)
                            // exactly as it already is below, so it cannot be
                            // relied on to prevent this.
                            if task.status != "failed" {
                                continue;
                            }
                            // Discard any worktree left behind by a prior failed
                            // attempt (whether it failed mid-merge or mid-agent-run)
                            // rather than resuming into potentially stale/dirty
                            // state — see this step's design note above. Best-effort:
                            // a failure here just means the row/directory are left
                            // for the next startup cleanup pass, same as any other
                            // `remove_worktree` failure elsewhere in this feature.
                            if let Ok(Some(row)) =
                                evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
                                    .await
                            {
                                let worktree_path = std::path::PathBuf::from(&row.worktree_path);
                                let primary_root = std::path::PathBuf::from(&row.primary_workspace_root);
                                let lock = state.merge_lock_for(&primary_root).await;
                                let _guard = lock.lock().await;
                                if let Err(error) =
                                    crate::task::worktree::remove_worktree(&primary_root, &worktree_path)
                                        .await
                                {
                                    tracing::warn!(%task_id, %error, "failed to remove stale worktree before retry; leaving it for startup cleanup");
                                } else if let Err(error) = evohime_storage::task_worktrees::delete_task_worktree(
                                    &state.pool, task_id,
                                )
                                .await
                                {
                                    tracing::warn!(%task_id, %error, "failed to delete stale task_worktrees row before retry");
                                }
                            }
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
                                },
                            )
                            .await?;

                            let token = CancellationToken::new();
                            // Same atomic insert-and-check as the `UserMessage`
                            // handler (Step 2) — teardown above may have just
                            // deleted this task's own row, so whether it needs a
                            // *fresh* one now depends on the current state of
                            // `task_cancellations` at this exact instant, not on
                            // whatever was true when the task originally started.
                            let is_concurrent = {
                                let mut guard = state.task_cancellations.lock().await;
                                let is_concurrent = !guard.is_empty();
                                guard.insert(task_id, token.clone());
                                is_concurrent
                            };
                            let primary_root = crate::task::helpers::resolve_workspace_path(
                                &state,
                                task.workspace_path.clone(),
                            )?;
                            if is_concurrent {
                                if let Err(error) = crate::task::worktree::provision_worktree(
                                    &state, task_id, &primary_root,
                                )
                                .await
                                {
                                    error!(%task_id, %error, "failed to allocate isolated worktree for retried concurrent task");
                                    state.task_cancellations.lock().await.remove(&task_id);
                                    let _ = fail_task(&state.pool, task_id).await;
                                    let _ = emit_event(
                                        &state,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: format!("failed to allocate isolated worktree: {error}"),
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    continue;
                                }
                            }
                            let mut cancellation_guard =
                                crate::task::helpers::TaskCancellationGuard::new(state.clone(), task_id);
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
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                cancellation_guard.disarm();
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
                            });
                        }
                        ClientCommand::ApprovalGranted {
                            approval_id,
                            remember_path,
                        } => {
                            let task_id = state
                                .permissions
                                .approval(approval_id)
                                .await
                                .map(|(request, _)| request.task_id)
                                .unwrap_or(Uuid::nil());
                            let status = state
                                .permissions
                                .resolve_with_options(approval_id, true, remember_path)
                                .await;
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
                                if remember_path {
                                    "Approval granted; path remembered for 1h in this session"
                                } else {
                                    "Approval granted once"
                                }
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
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
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
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
