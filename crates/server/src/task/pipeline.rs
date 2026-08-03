//! Task run pipeline: ReAct execution, pause/resume, list tasks.
use crate::app::AppState;
use crate::permissions_api::permission_name;
use crate::sessions_api::summarize_session_title;
use crate::task::approval_review::approval_review;
use crate::task::helpers::{
    claim_attachment_context, emit_event, load_chat_history, map_agent_error, resolve_model_route,
};
use crate::task::memory::{apply_task_memory_feedback, persist_structured_memory};
use crate::task::steps::{
    build_agent_resume_context, finalize_open_task_steps, update_task_step_status,
};
use crate::ApiError;
use axum::{
    extract::{Extension, State},
    Json,
};
use evohime_agent_runtime::{
    run_agent_loop, run_agent_loop_resumed, AgentConfig, AgentError, AgentResumeContext,
};
use evohime_protocol::ServerEvent;
use evohime_task_engine::{complete_task, pause_task};
use evohime_tool_runtime::ToolError;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(crate) async fn process_user_message(
    state: &Arc<AppState>,
    session_id: Uuid,
    task: evohime_storage::TaskRow,
    cancellation: CancellationToken,
) -> Result<(), (Uuid, ApiError)> {
    run_task_pipeline(state, session_id, task, cancellation, true).await
}

pub(crate) async fn resume_task_run(
    state: &Arc<AppState>,
    task: evohime_storage::TaskRow,
    cancellation: CancellationToken,
    emit_started: bool,
) -> Result<(), (Uuid, ApiError)> {
    run_task_pipeline(state, task.session_id, task, cancellation, emit_started).await
}

pub(crate) async fn run_task_pipeline(
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
    let workspace_scope = task
        .workspace_path
        .clone()
        .unwrap_or_else(|| state.workspace_root.to_string_lossy().into_owned());
    // `primary_workspace_root` is the task's own semantic project root — the
    // same value used for memory scoping/UI display, and what merge-back
    // must land its commit on. `workspace_root` may be overridden below to
    // point at an isolated worktree; memory/UI scoping never is.
    let primary_workspace_root = task
        .workspace_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.workspace_root.clone());
    let worktree_row = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task.id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    let workspace_root = worktree_row
        .as_ref()
        .map(|row| PathBuf::from(&row.worktree_path))
        .unwrap_or_else(|| primary_workspace_root.clone());
    let attachment_context = if emit_started {
        claim_attachment_context(state, session_id, task.id, &primary_workspace_root)
            .await
            .map_err(|error| (task.id, error))?
    } else {
        None
    };
    let agent_user_message = match attachment_context {
        Some(context) => format!("{}\n\n[Attachment Context]\n{}", task.user_message, context),
        None => task.user_message.clone(),
    };

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
    let memory_notes = structured
        .entries
        .into_iter()
        .map(|entry| match (&entry.scope, &entry.status) {
            (Some(scope), Some(status)) => format!("[{scope}/{status}] {}", entry.content),
            _ => entry.content,
        })
        .collect::<Vec<_>>();
    let planning_memory_context =
        evohime_memory::format_planner_suggestions(&structured.planner_suggestions);

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
    let session = evohime_storage::load_session(&state.pool, session_id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?
        .ok_or_else(|| {
            (
                task.id,
                ApiError::Internal(format!("Session {session_id} not found")),
            )
        })?;

    let effective_workspace_root = workspace_root.clone();
    let agent_config = AgentConfig {
        task_id: task.id,
        session_id,
        operator_id: session.operator_id,
        user_message: agent_user_message,
        created_at: task.created_at,
        demo_file_path: workspace_root.join("docs/sample-context.md"),
        workspace_root,
        model_route,
        model: task.model.clone(),
        planning_model_route,
        planning_model,
        planning_memory_context,
        memory_pool: Some(state.pool.clone()),
        workspace_key: workspace_scope.clone(),
        is_subagent: false,
        subagent_depth: 0,
        subagent_max_steps: None,
        telemetry: Some(crate::llm_telemetry::PipelineLlmTelemetry::shared(
            state.metrics.clone(),
        )),
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
                Some(mut event) => {
                    if let ServerEvent::TaskCompleted { duration_ms, .. } = &mut event {
                        *duration_ms = state.metrics.open_task_duration_ms(task.id);
                    }
                    match &event {
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
                input,
            })) => {
                let preview = approval_review(&tool, &input, &effective_workspace_root).await;
                emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::AgentStatus {
                        task_id: task.id,
                        phase: "waiting_approval".into(),
                    },
                )
                .await?;
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
                        risk_level: preview.risk_level,
                        review: Some(preview.review),
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
                            "input": input.clone(),
                        },
                        "react_pending_call": {
                            "id": approval_id.to_string(),
                            "name": tool.replace('.', "_"),
                            "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()),
                        },
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
                        correlation_id: Some(task.id),
                        duration_ms: None,
                    },
                )
                .await?;
                return Ok(());
            }
            Err(error) => {
                let err_msg = error.to_string();
                state.metrics.task_finished(session_id, task.id, false);
                apply_task_memory_feedback(state, session_id, task.id, &used_memory_ids, false)
                    .await;
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

    if let Some(row) = &worktree_row {
        crate::task::worktree::finalize_worktree(state, task.id, &primary_workspace_root, row)
            .await
            .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    }

    evohime_storage::insert_message(
        &state.pool,
        session_id,
        Some(task.id),
        "assistant",
        &agent_result.final_message,
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

pub(crate) async fn list_tasks(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
) -> Result<Json<Vec<evohime_storage::TaskRow>>, ApiError> {
    evohime_storage::list_tasks_for_operator(&state.pool, identity.id, None)
        .await
        .map(Json)
        .map_err(|error| ApiError::Internal(error.to_string()))
}
