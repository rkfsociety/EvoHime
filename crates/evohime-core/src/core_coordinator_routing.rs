use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::StartTask {
            task_id,
            prompt,
            workspace_root,
            preferred_route_hint,
            conversation,
            goal_id,
        } => {
            let cancellation = CancellationToken::new();
            let run_id = format!("agent-{}", uuid::Uuid::new_v4());
            let mut state_guard = state.lock().await;
            if state_guard
                .tasks
                .insert(
                    task_id.clone(),
                    ActiveTask {
                        cancellation: cancellation.clone(),
                    },
                )
                .is_some()
            {
                return;
            }
            let events = state_guard.events.clone();
            let executor = state_guard.executor.clone();
            let journal = state_guard.journal.clone();
            let mut workspace_root =
                workspace_root.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            if let Some(journal) = &state_guard.journal {
                let database = journal.database().lock().await;
                if let Ok(Some(binding)) =
                    evohime_local_storage::domains::workflow::get_ready_for_task(
                        database.connection(),
                        &task_id,
                    )
                {
                    let candidate = workspace_root.join(&binding.root_ref);
                    if candidate.is_dir() {
                        workspace_root = candidate;
                    }
                }
            }
            if let Some(journal) = &state_guard.journal {
                let database = journal.database().lock().await;
                let _ = evohime_local_storage::domains::runs::attach_task_context(
                    database.connection(),
                    &task_id,
                    &prompt,
                    &workspace_root.to_string_lossy(),
                    crate::task_memory::now_millis() as i64,
                );
                if let Ok(Some(binding)) =
                    evohime_local_storage::workspace_sets_store::get_run_binding(
                        database.connection(),
                        &task_id,
                    )
                {
                    if let Ok(binding) = serde_json::from_slice::<serde_json::Value>(&binding) {
                        write_model_trace(
                            "workspace_sets.run_binding_pinned",
                            serde_json::json!({
                                "task_id": task_id,
                                "set_id": binding.get("set_id"),
                                "set_version": binding.get("set_version"),
                                "set_hash": binding.get("set_hash"),
                                "root_count": binding.get("roots").and_then(serde_json::Value::as_array).map_or(0, Vec::len),
                                "pinned": binding.get("pinned")
                            }),
                        );
                    }
                }
            }
            drop(state_guard);
            let conversation_history = if let Some(conversation) = conversation {
                let history = match journal.as_ref() {
                    Some(journal) => {
                        journal
                            .model_conversation_history_before(
                                &conversation.conversation_id,
                                conversation.before_sequence,
                            )
                            .await
                    }
                    None => Err(StorageError::InvalidInput(
                        "conversation journal is unavailable".into(),
                    )),
                };
                match history {
                    Ok(messages) => messages,
                    Err(_) => {
                        state.lock().await.tasks.remove(&task_id);
                        let _ = events
                            .send(CoreEvent::TaskFailed {
                                task_id,
                                error: "conversation history could not be loaded".into(),
                            })
                            .await;
                        return;
                    }
                }
            } else {
                Vec::new()
            };
            let _ = events
                .send(CoreEvent::TaskStarted {
                    task_id: task_id.clone(),
                    prompt: evohime_model_provenance::OMITTED_MESSAGE_MARKER.into(),
                })
                .await;
            let Some(background_permit) = state.lock().await.background_tasks.try_acquire() else {
                let mut state_guard = state.lock().await;
                state_guard.tasks.remove(&task_id);
                let events = state_guard.events.clone();
                drop(state_guard);
                let _ = events
                    .send(CoreEvent::TaskFailed {
                        task_id,
                        error: "background task capacity is exhausted".into(),
                    })
                    .await;
                return;
            };
            tokio::spawn(async move {
                let _background_permit = background_permit;
                let intent_hash = crate::research::sha256_hex(prompt.as_bytes());
                if let Some(journal) = &journal {
                    let checkpoint_runtime =
                        crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
                    match checkpoint_runtime.recover(&task_id, &workspace_root).await {
                        Ok(recovery)
                            if matches!(
                                recovery.disposition,
                                crate::task_checkpoint::RecoveryDisposition::Blocked
                                    | crate::task_checkpoint::RecoveryDisposition::Terminal
                            ) =>
                        {
                            let mut state_guard = state.lock().await;
                            state_guard.tasks.remove(&task_id);
                            let warning = recovery.warning.unwrap_or_else(|| {
                                "checkpoint recovery requires explicit reconciliation".into()
                            });
                            let events = state_guard.events.clone();
                            drop(state_guard);
                            let _ = events
                                .send(CoreEvent::TaskFailed {
                                    task_id,
                                    error: warning,
                                })
                                .await;
                            return;
                        }
                        Err(error) => {
                            let mut state_guard = state.lock().await;
                            state_guard.tasks.remove(&task_id);
                            let events = state_guard.events.clone();
                            drop(state_guard);
                            let _ = events
                                .send(CoreEvent::TaskFailed {
                                    task_id,
                                    error: format!("task checkpoint recovery failed: {error}"),
                                })
                                .await;
                            return;
                        }
                        Ok(_) => {}
                    }
                    if let Err(error) = checkpoint_runtime
                        .capture(
                            &task_id,
                            &workspace_root,
                            crate::task_checkpoint::CheckpointStatus::InProgress,
                            crate::task_checkpoint::CheckpointCaptureReason::RunStarted,
                            None,
                        )
                        .await
                    {
                        let mut state_guard = state.lock().await;
                        state_guard.tasks.remove(&task_id);
                        let events = state_guard.events.clone();
                        drop(state_guard);
                        let _ = events
                            .send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!("task checkpoint could not be persisted: {error}"),
                            })
                            .await;
                        return;
                    }
                    if let Err(error) = journal
                        .begin_agent_run(&run_id, &task_id, &intent_hash)
                        .await
                    {
                        let mut state_guard = state.lock().await;
                        state_guard.tasks.remove(&task_id);
                        let events = state_guard.events.clone();
                        drop(state_guard);
                        let _ = events
                            .send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!(
                                    "agent run could not acquire durable lease: {error}"
                                ),
                            })
                            .await;
                        return;
                    }
                }

                let heartbeat_cancel = CancellationToken::new();
                let heartbeat_failure = Arc::new(Mutex::new(None::<String>));
                let heartbeat_task = journal.as_ref().map(|journal| {
                    let journal = journal.clone();
                    let run_id = run_id.clone();
                    let failure = heartbeat_failure.clone();
                    let cancel = heartbeat_cancel.clone();
                    tokio::spawn(async move {
                        let mut interval = tokio::time::interval(Duration::from_secs(10));
                        loop {
                            tokio::select! {
                                _ = cancel.cancelled() => break,
                                _ = interval.tick() => {
                                    if let Err(error) = journal.heartbeat_agent_run(&run_id).await {
                                        *failure.lock().await = Some(error.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    })
                });
                // A task is a loop of model calls and tool runs, so its
                // budget must exceed one model call (120 s by default).
                // Set the environment override to zero to disable this deadline.
                let task_timeout_secs = std::env::var("EVOHIME_TASK_TIMEOUT_SECONDS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(DEFAULT_TASK_TIMEOUT_SECONDS);
                let task_timeout = task_timeout_duration(task_timeout_secs);
                if let (Some(journal), Some(gid)) = (&journal, goal_id.as_deref()) {
                    ensure_goal_continuation_run(journal, &task_id, &workspace_root, gid).await;
                }
                let continuation_context = if let Some(journal) = &journal {
                    let database = journal.database().lock().await;
                    let run = evohime_local_storage::domains::runs::get_run_by_task(
                        database.connection(),
                        &task_id,
                    )
                    .ok()
                    .flatten();
                    run.and_then(|run| {
                        serde_json::from_slice::<crate::continuation::ContinuationPolicyV1>(
                            &evohime_local_storage::domains::runs::get_policy(
                                database.connection(),
                                &run.policy_id,
                                run.policy_revision,
                                &run.owner_scope,
                            )
                            .ok()
                            .flatten()?
                            .canonical_json,
                        )
                        .ok()
                        .map(|policy| (run, policy))
                    })
                } else {
                    None
                };
                let is_continuation_run = continuation_context.is_some();
                let mut result = Err(AgentRunError::Internal(
                    "continuation did not execute an attempt".into(),
                ));
                let mut continuation_index = continuation_context
                    .as_ref()
                    .map(|(run, _)| run.continuation_index)
                    .unwrap_or(0);
                let mut stagnation_count: u32 = 0;
                let mut last_attempt_output: Option<String> = None;
                loop {
                    let attempt = if let Some((run, policy)) = &continuation_context {
                        if policy.budget.max_wall_clock_ms.is_some_and(|limit| {
                            crate::task_memory::now_millis()
                                .saturating_sub(run.created_at_ms.max(0) as u64)
                                >= limit
                        }) {
                            if let Some(journal) = &journal {
                                if let Ok(database) = journal.database().try_lock() {
                                    let _ = evohime_local_storage::domains::runs::transition_run(
                                        database.connection(),
                                        &run.run_id,
                                        "running",
                                        "budget_limited",
                                        Some("max_wall_clock_ms"),
                                        crate::task_memory::now_millis() as i64,
                                    );
                                }
                            }
                            break;
                        }
                        let fingerprint =
                            format!("{}:{}", task_id, continuation_index.saturating_add(1));
                        let Some(journal) = journal.as_ref() else {
                            break;
                        };
                        let mut database = journal.database().lock().await;
                        match evohime_local_storage::domains::runs::reserve_attempt(
                            database.connection_mut(),
                            &run.run_id,
                            "task",
                            &fingerprint,
                            0,
                            0,
                            crate::task_memory::now_millis() as i64,
                        ) {
                            Ok(true) => Some((run.run_id.clone(), continuation_index + 1)),
                            Ok(false) => break,
                            Err(_) => {
                                result = Err(AgentRunError::Internal(
                                    "continuation budget or state rejected the next attempt".into(),
                                ));
                                break;
                            }
                        }
                    } else {
                        None
                    };
                    let current_attempt = attempt
                        .as_ref()
                        .and_then(|(_, index)| u32::try_from(*index).ok());
                    let (iteration_prompt, completed_before) = if let (
                        Some(journal),
                        Some((run, policy)),
                    ) =
                        (&journal, &continuation_context)
                    {
                        if let Some(gid) = policy.linked_goal_id.as_deref() {
                            let database = journal.database().lock().await;
                            let current_goal =
                                evohime_local_storage::goal::GoalStore::new(database.connection())
                                    .get(gid)
                                    .ok()
                                    .flatten();
                            if let Some(goal) = current_goal {
                                match goal.status {
                                    evohime_local_storage::goal::GoalStatus::Paused
                                    | evohime_local_storage::goal::GoalStatus::Cancelled => {
                                        let _ = evohime_local_storage::domains::runs::transition_run(
                                            database.connection(),
                                            &run.run_id,
                                            "running",
                                            "stopped",
                                            Some(if goal.status == evohime_local_storage::goal::GoalStatus::Paused { "goal_paused" } else { "goal_cancelled" }),
                                            crate::task_memory::now_millis() as i64,
                                        );
                                        if result.is_err() {
                                            result = Err(AgentRunError::Cancelled);
                                        }
                                        break;
                                    }
                                    evohime_local_storage::goal::GoalStatus::Completed => {
                                        let _ =
                                            evohime_local_storage::domains::runs::transition_run(
                                                database.connection(),
                                                &run.run_id,
                                                "running",
                                                "completed",
                                                Some("goal_completed"),
                                                crate::task_memory::now_millis() as i64,
                                            );
                                        if result.is_err() {
                                            result = Ok(last_attempt_output
                                                .clone()
                                                .unwrap_or_else(|| "Цель уже выполнена.".into()));
                                        }
                                        break;
                                    }
                                    _ => {}
                                }
                                let verified = goal.success_criteria.iter().filter(|c| c.status == evohime_local_storage::goal::GoalCriterionStatus::Verified).count();
                                let formatted = format_goal_continuation_prompt(
                                    &prompt,
                                    &goal,
                                    continuation_index + 1,
                                    run.max_continuations,
                                    last_attempt_output.as_deref(),
                                );
                                (formatted, verified)
                            } else {
                                (prompt.clone(), 0)
                            }
                        } else {
                            (prompt.clone(), 0)
                        }
                    } else {
                        (prompt.clone(), 0)
                    };
                    result = match executor.as_ref() {
                        Some(executor) => {
                            let execution = executor.execute_in_conversation(
                                task_id.clone(),
                                iteration_prompt,
                                workspace_root.clone(),
                                ConversationExecutionContext {
                                    preferred_route_hint: preferred_route_hint.clone(),
                                    history: conversation_history.clone(),
                                    continuation_attempt: current_attempt,
                                    defer_completion_event: is_continuation_run,
                                },
                                cancellation.clone(),
                                events.clone(),
                            );
                            match await_with_optional_timeout(task_timeout, execution).await {
                                Ok(result) => result,
                                Err(()) => Err(AgentRunError::Timeout(task_timeout_secs)),
                            }
                        }
                        None => {
                            cancellation.cancelled().await;
                            Err(AgentRunError::Cancelled)
                        }
                    };
                    let Some((run_id, attempt_index)) = attempt else {
                        break;
                    };
                    continuation_index = continuation_index.saturating_add(1);
                    let success = result.is_ok();
                    let mut required_gates_passed = true;
                    let mut pending_approval = false;
                    let mut pending_approval_id: Option<String> = None;
                    let mut gate_unknown = false;
                    let mut gate_non_retryable = false;
                    if success {
                        if let Some((_, policy)) = &continuation_context {
                            for gate in &policy.gates {
                                let outcome = match executor.as_ref() {
                                    Some(executor) => {
                                        executor
                                            .execute_continuation_gate(
                                                gate.clone(),
                                                task_id.clone(),
                                                workspace_root.clone(),
                                                cancellation.clone(),
                                            )
                                            .await
                                    }
                                    None => crate::continuation::GateOutcome::Unavailable {
                                        code: "gate_executor_unavailable".into(),
                                    },
                                };
                                if let Some((run, _)) = &continuation_context {
                                    let (status, evidence_ref, error_code) = match &outcome {
                                        crate::continuation::GateOutcome::Passed {
                                            evidence_ref,
                                        } => ("passed", Some(evidence_ref.clone()), None),
                                        crate::continuation::GateOutcome::PendingApproval {
                                            ..
                                        } => (
                                            "pending_approval",
                                            None,
                                            Some("approval_required".into()),
                                        ),
                                        crate::continuation::GateOutcome::Failed {
                                            code, ..
                                        } => ("failed", None, Some(code.clone())),
                                        crate::continuation::GateOutcome::Unavailable { code } => {
                                            ("unavailable", None, Some(code.clone()))
                                        }
                                    };
                                    let Some(journal) = journal.as_ref() else {
                                        gate_unknown = true;
                                        break;
                                    };
                                    let database = journal.database().lock().await;
                                    let _ = evohime_local_storage::domains::runs::record_gate_result(
                                            database.connection(),
                                            &evohime_local_storage::domains::runs::GateResultRecord {
                                                run_id: run.run_id.clone(),
                                                gate_id: gate.id.clone(),
                                                attempt_index,
                                                status: status.into(),
                                                evidence_ref,
                                                error_code,
                                                created_at_ms: crate::task_memory::now_millis() as i64,
                                            },
                                        );
                                }
                                match outcome {
                                    crate::continuation::GateOutcome::Passed { .. } => {}
                                    crate::continuation::GateOutcome::PendingApproval {
                                        approval_id,
                                    } => {
                                        required_gates_passed = false;
                                        pending_approval = true;
                                        pending_approval_id = Some(approval_id);
                                        break;
                                    }
                                    crate::continuation::GateOutcome::Failed {
                                        retryable, ..
                                    } => {
                                        required_gates_passed = false;
                                        gate_non_retryable |= !retryable;
                                        gate_unknown |= retryable;
                                        break;
                                    }
                                    crate::continuation::GateOutcome::Unavailable { .. } => {
                                        required_gates_passed = false;
                                        gate_unknown = true;
                                        break;
                                    }
                                }
                            }
                        }
                    } else {
                        required_gates_passed = false;
                    }
                    let decision = if let Some((run, policy)) = &continuation_context {
                        let Some(journal) = journal.as_ref() else {
                            result = Err(AgentRunError::Internal(
                                "continuation journal disappeared".into(),
                            ));
                            break;
                        };
                        let database = journal.database().lock().await;
                        let result_json = serde_json::to_vec(&serde_json::json!({
                            "success": success,
                            "error": result.as_ref().err().map(ToString::to_string)
                        }))
                        .unwrap_or_default();
                        let _ = evohime_local_storage::domains::runs::finish_attempt(
                            database.connection(),
                            &run_id,
                            attempt_index,
                            if success { "completed" } else { "failed" },
                            &result_json,
                            crate::task_memory::now_millis() as i64,
                        );
                        let mut goal_stagnated = false;
                        if let Some(gid) = policy.linked_goal_id.as_deref() {
                            if let Ok(Some(goal_after)) =
                                evohime_local_storage::goal::GoalStore::new(database.connection())
                                    .get(gid)
                            {
                                let completed_after = goal_after.success_criteria.iter().filter(|c| c.status == evohime_local_storage::goal::GoalCriterionStatus::Verified).count();
                                if result.is_ok()
                                    && goal_after.status
                                        == evohime_local_storage::goal::GoalStatus::Active
                                {
                                    if completed_after > completed_before {
                                        stagnation_count = 0;
                                    } else {
                                        stagnation_count = stagnation_count.saturating_add(1);
                                        if stagnation_count >= 3 {
                                            goal_stagnated = true;
                                            let cmd_hash = crate::research::sha256_hex(
                                                format!(
                                                    "stagnation:{gid}:{}:{}",
                                                    goal_after.version, continuation_index
                                                )
                                                .as_bytes(),
                                            );
                                            let _ = evohime_local_storage::goal::GoalStore::new(database.connection()).transition(
                                                gid,
                                                goal_after.version,
                                                evohime_local_storage::goal::GoalStatus::Blocked,
                                                crate::goal::GoalCommand::new("core", &format!("goal-stagnation:{task_id}:{continuation_index}"), &cmd_hash),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        let goal_criteria_complete =
                            policy.linked_goal_id.as_ref().is_none_or(|goal_id| {
                                evohime_local_storage::goal::GoalStore::new(database.connection())
                                    .get(goal_id)
                                    .ok()
                                    .flatten()
                                    .is_some_and(|goal| {
                                        matches!(
                                            goal.status,
                                            evohime_local_storage::goal::GoalStatus::Completed
                                        ) && goal.remaining_criteria.is_empty()
                                    })
                            });
                        let decision =
                            crate::continuation::decide(&crate::continuation::DecisionEvidence {
                                required_gates_passed,
                                goal_criteria_complete,
                                pending_approval,
                                unknown_outcome: result.is_err() || gate_unknown || goal_stagnated,
                                non_retryable_failure: result.is_err() || gate_non_retryable,
                                continuation_index: continuation_index as u32,
                                max_continuations: run.max_continuations as u32,
                                model_turns: continuation_index as u32,
                                max_model_turns: run.max_model_turns as u32,
                                ..Default::default()
                            });
                        let next_state = match decision {
                            crate::continuation::Decision::Complete => "completed",
                            crate::continuation::Decision::BudgetLimited => "budget_limited",
                            crate::continuation::Decision::StopFailed => "failed",
                            crate::continuation::Decision::StopUser => "stopped",
                            crate::continuation::Decision::PauseForApproval => "waiting_approval",
                            crate::continuation::Decision::Blocked => "blocked",
                            crate::continuation::Decision::Continue => "running",
                        };
                        if let Some(approval_id) = pending_approval_id {
                            let _ = events
                                .send(CoreEvent::ApprovalRequired {
                                    task_id: task_id.clone(),
                                    approval_id,
                                    tool_name: "continuation_gate".into(),
                                    permission: "continuation_gate".into(),
                                    scope: workspace_root
                                        .to_string_lossy()
                                        .chars()
                                        .take(256)
                                        .collect(),
                                    preview: evohime_permissions::ApprovalPreview {
                                        kind: "continuation_gate".into(),
                                        summary: "Continuation gate requires user approval".into(),
                                        command: None,
                                        cwd: None,
                                        path: None,
                                        details: None,
                                        truncated: false,
                                    },
                                })
                                .await;
                        }
                        if next_state == "budget_limited" {
                            if let Some(gid) = policy.linked_goal_id.as_deref() {
                                if let Ok(Some(goal_now)) =
                                    evohime_local_storage::goal::GoalStore::new(
                                        database.connection(),
                                    )
                                    .get(gid)
                                {
                                    if goal_now.status
                                        == evohime_local_storage::goal::GoalStatus::Active
                                    {
                                        let cmd_hash = crate::research::sha256_hex(
                                            format!(
                                                "budget:{gid}:{}:{}",
                                                goal_now.version, continuation_index
                                            )
                                            .as_bytes(),
                                        );
                                        let _ = evohime_local_storage::goal::GoalStore::new(
                                            database.connection(),
                                        )
                                        .transition(
                                            gid,
                                            goal_now.version,
                                            evohime_local_storage::goal::GoalStatus::BudgetLimited,
                                            crate::goal::GoalCommand::new(
                                                "core",
                                                &format!(
                                                    "goal-budget:{task_id}:{continuation_index}"
                                                ),
                                                &cmd_hash,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        if next_state != "running" {
                            let _ = evohime_local_storage::domains::runs::transition_run(
                                database.connection(),
                                &run.run_id,
                                "running",
                                next_state,
                                Some(&format!("continuation_{next_state}")),
                                crate::task_memory::now_millis() as i64,
                            );
                        }
                        decision
                    } else {
                        crate::continuation::Decision::Complete
                    };
                    if matches!(decision, crate::continuation::Decision::Continue) {
                        if let Ok(output) = &result {
                            if !output.trim().is_empty() {
                                let _ = events
                                    .send(CoreEvent::AssistantDelta {
                                        task_id: task_id.clone(),
                                        content: format!("{output}\n"),
                                    })
                                    .await;
                                last_attempt_output = Some(output.clone());
                            }
                        }
                        continue;
                    }
                    break;
                }
                heartbeat_cancel.cancel();
                if let Some(heartbeat_task) = heartbeat_task {
                    let _ = heartbeat_task.await;
                }
                let heartbeat_error = heartbeat_failure.lock().await.clone();
                if let Some(journal) = &journal {
                    let checkpoint_status = if heartbeat_error.is_some() {
                        crate::task_checkpoint::CheckpointStatus::Conflicted
                    } else if result.is_ok() {
                        crate::task_checkpoint::CheckpointStatus::Completed
                    } else if matches!(&result, Err(AgentRunError::Cancelled)) {
                        crate::task_checkpoint::CheckpointStatus::Paused
                    } else {
                        crate::task_checkpoint::CheckpointStatus::Failed
                    };
                    let reason = match checkpoint_status {
                        crate::task_checkpoint::CheckpointStatus::Completed => {
                            crate::task_checkpoint::CheckpointCaptureReason::Completed
                        }
                        crate::task_checkpoint::CheckpointStatus::Paused => {
                            crate::task_checkpoint::CheckpointCaptureReason::Paused
                        }
                        _ => crate::task_checkpoint::CheckpointCaptureReason::Failed,
                    };
                    let checkpoint_runtime =
                        crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
                    if let Err(error) = checkpoint_runtime
                        .capture(&task_id, &workspace_root, checkpoint_status, reason, None)
                        .await
                    {
                        if result.is_ok() {
                            result = Err(AgentRunError::Internal(format!(
                                "task checkpoint could not be persisted: {error}"
                            )));
                        }
                    }
                    if heartbeat_error.is_none() || result.is_err() {
                        let _ = journal.complete_agent_run(&run_id, result.is_ok()).await;
                    }
                }
                let mut state_guard = state.lock().await;
                state_guard.tasks.remove(&task_id);
                let events = state_guard.events.clone();
                drop(state_guard);
                if let Err(error) = &result {
                    let _ = events
                        .send(CoreEvent::RoutingTrace {
                            task_id: task_id.clone(),
                            trace: routing_failure_trace(&run_id, error),
                        })
                        .await;
                }
                match (result, heartbeat_error) {
                    (Ok(_), Some(error)) => {
                        let _ = events
                            .send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!(
                                "agent run lease was lost; outcome requires reconciliation: {error}"
                            ),
                            })
                            .await;
                    }
                    (Ok(final_message), None) => {
                        if is_continuation_run {
                            let _ = events
                                .send(CoreEvent::TaskCompleted {
                                    task_id,
                                    final_message,
                                })
                                .await;
                        }
                    }
                    (Err(error), _) => {
                        let task_id = task_id;
                        if matches!(error, AgentRunError::Cancelled) {
                            let _ = events.send(CoreEvent::TaskStopped { task_id }).await;
                        } else {
                            let _ = events
                                .send(CoreEvent::TaskFailed {
                                    task_id,
                                    error: error.to_string(),
                                })
                                .await;
                        }
                    }
                }
            });
        }
        CoreCommand::ResolveRoutingDecision {
            trace_id,
            approve,
            reply,
        } => {
            let approvals = state.lock().await.routing_approvals.clone();
            match approvals.resolve(&trace_id, approve).await {
                Ok(_) => {
                    state
                        .lock()
                        .await
                        .routing_decisions
                        .insert(trace_id, approve);
                    let _ = reply.send(Ok(serde_json::json!({"accepted": true})
                        .to_string()
                        .into_bytes()));
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            }
        }
        CoreCommand::ExtractAmbientMemory { episode_id } => {
            let (executor, events) = {
                let state = state.lock().await;
                (state.executor.clone(), state.events.clone())
            };
            let Some(executor) = executor else {
                return;
            };
            // Извлечение не держит очередь команд: эпизод уже закрыт, и
            // ждать его разбора некому.
            let Some(background_permit) = state.lock().await.background_tasks.try_acquire() else {
                return;
            };
            tokio::spawn(async move {
                let _background_permit = background_permit;
                executor.extract_ambient_memory(episode_id, events).await;
            });
        }
        CoreCommand::StopTask { task_id } => {
            let mut state_guard = state.lock().await;
            if let Some(active) = state_guard.tasks.remove(&task_id) {
                active.cancellation.cancel();
            }
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

async fn await_with_optional_timeout<F>(
    timeout_duration: Option<Duration>,
    future: F,
) -> Result<F::Output, ()>
where
    F: std::future::Future,
{
    match timeout_duration {
        Some(duration) => timeout(duration, future).await.map_err(|_| ()),
        None => Ok(future.await),
    }
}

fn task_timeout_duration(timeout_secs: u64) -> Option<Duration> {
    (timeout_secs > 0).then(|| Duration::from_secs(timeout_secs))
}

async fn ensure_goal_continuation_run(
    journal: &crate::EventJournal,
    task_id: &str,
    workspace_root: &std::path::Path,
    goal_id: &str,
) {
    let database = journal.database().lock().await;
    if evohime_local_storage::domains::runs::get_run_by_task(database.connection(), task_id)
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }
    let store = evohime_local_storage::goal::GoalStore::new(database.connection());
    let Ok(Some(mut goal)) = store.get(goal_id) else {
        return;
    };
    if matches!(
        goal.status,
        evohime_local_storage::goal::GoalStatus::Completed
            | evohime_local_storage::goal::GoalStatus::Cancelled
    ) {
        return;
    }
    if matches!(
        goal.status,
        evohime_local_storage::goal::GoalStatus::Paused
            | evohime_local_storage::goal::GoalStatus::Blocked
            | evohime_local_storage::goal::GoalStatus::BudgetLimited
    ) {
        let cmd_hash =
            crate::research::sha256_hex(format!("resume:{goal_id}:{task_id}").as_bytes());
        if let Ok(resumed) = store.transition(
            goal_id,
            goal.version,
            evohime_local_storage::goal::GoalStatus::Active,
            crate::goal::GoalCommand::new(
                "core",
                &format!("goal-resume:{task_id}:{goal_id}"),
                &cmd_hash,
            ),
        ) {
            goal = resumed.goal;
        }
    }
    let now = crate::task_memory::now_millis() as i64;
    let max_continuations = goal.continuation_budget.unwrap_or(12).clamp(1, 50) as u32;
    let policy_id = format!("goal-policy:{goal_id}");
    let workspace_path = workspace_root.to_string_lossy();
    let owner_scope = crate::task_memory::project_scope_id(&workspace_path);
    let mut policy = crate::continuation::ContinuationPolicyV1 {
        schema_version: crate::continuation::POLICY_SCHEMA_VERSION,
        id: policy_id.clone(),
        revision: 1,
        scope: crate::continuation::ScopeRef {
            kind: crate::continuation::ScopeKind::Goal,
            owner_scope: owner_scope.clone(),
            workspace_id: owner_scope.clone(),
        },
        actor: "core".into(),
        enabled: true,
        linked_goal_id: Some(goal_id.to_owned()),
        budget: crate::continuation::BudgetV1 {
            max_continuations,
            max_model_turns: max_continuations.saturating_mul(4),
            max_tokens: goal.token_budget,
            max_cost_micros: goal.cost_budget_micros,
            max_wall_clock_ms: None,
        },
        require_workspace_change_before_retry: false,
        stop_on_user_interaction: true,
        stop_on_approval_required: true,
        stop_on_unknown_outcome: false,
        gates: Vec::new(),
        completion_mode: crate::continuation::CompletionMode::GoalCriteriaAndGates,
        created_at_ms: now,
        updated_at_ms: now,
        content_hash: String::new(),
    };
    if let Ok(hash) = policy.compute_hash() {
        policy.content_hash = hash;
    }
    let Ok(canonical_json) = serde_json::to_vec(&policy) else {
        return;
    };
    let _ = evohime_local_storage::domains::runs::save_policy(
        database.connection(),
        &evohime_local_storage::domains::runs::PolicyRecord {
            policy_id: policy_id.clone(),
            revision: 1,
            owner_scope: owner_scope.clone(),
            actor: "core".into(),
            enabled: true,
            canonical_json,
            content_hash: policy.content_hash.clone(),
            created_at_ms: now,
            updated_at_ms: now,
        },
    );
    let _ = evohime_local_storage::domains::runs::create_run(
        database.connection(),
        &evohime_local_storage::domains::runs::RunRecord {
            run_id: format!("goal-run:{task_id}"),
            idempotency_key: format!("goal-run:{task_id}:{goal_id}"),
            task_id: task_id.to_owned(),
            owner_scope,
            policy_id,
            policy_revision: 1,
            policy_hash: policy.content_hash,
            goal_id: Some(goal_id.to_owned()),
            goal_version: Some(goal.version as i64),
            state: "running".into(),
            continuation_index: 0,
            max_continuations: max_continuations as i64,
            max_model_turns: max_continuations.saturating_mul(4) as i64,
            used_model_turns: 0,
            token_budget: goal.token_budget.map(|v| v as i64),
            token_used: 0,
            cost_budget_micros: goal.cost_budget_micros.map(|v| v as i64),
            cost_used_micros: 0,
            stop_reason: None,
            prompt: None,
            workspace_path: Some(workspace_root.to_string_lossy().into_owned()),
            created_at_ms: now,
            updated_at_ms: now,
        },
    );
}

fn format_goal_iteration_prompt(
    base_prompt: &str,
    goal: &evohime_local_storage::goal::GoalV1,
    iteration: i64,
    max_iterations: i64,
) -> String {
    let mut criteria_lines = Vec::new();
    for criterion in &goal.success_criteria {
        let status = match criterion.status {
            evohime_local_storage::goal::GoalCriterionStatus::Verified => "verified",
            evohime_local_storage::goal::GoalCriterionStatus::Failed => "failed",
            evohime_local_storage::goal::GoalCriterionStatus::Blocked => "blocked",
            evohime_local_storage::goal::GoalCriterionStatus::Pending => "pending",
        };
        criteria_lines.push(format!(
            "- [{}] {} ({})",
            status, criterion.id, criterion.statement
        ));
    }
    format!(
        "<active_goal id=\"{}\" iteration=\"{}/{}\">\nЦель: {}\nКритерии выполнения:\n{}\nИнструкция: работай над целью до полного выполнения всех критериев. Проверяй результат реальными инструментами и вызывай инструмент `goal.verify` с `goal_id`, `criterion_id` и `evidence_summary` только после того, как критерий действительно выполнен и проверен.\n</active_goal>\n\n{}",
        goal.id,
        iteration.max(1),
        max_iterations.max(1),
        goal.objective,
        criteria_lines.join("\n"),
        base_prompt
    )
}

fn format_goal_continuation_prompt(
    base_prompt: &str,
    goal: &evohime_local_storage::goal::GoalV1,
    iteration: i64,
    max_iterations: i64,
    previous_attempt_output: Option<&str>,
) -> String {
    let mut prompt = format_goal_iteration_prompt(base_prompt, goal, iteration, max_iterations);
    if iteration > 1 {
        if let Some(previous) = previous_attempt_output
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let excerpt: String = previous.chars().take(1_200).collect();
            prompt.push_str("\n\n<previous_attempt_summary>\n");
            prompt.push_str(&excerpt);
            prompt.push_str("\n</previous_attempt_summary>\nПродолжай с того места, на котором остановился: не повторяй уже сделанную работу без необходимости, заверши оставшиеся критерии цели и подтверди каждый через `goal.verify`.");
        }
    }
    prompt
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[tokio::test]
    async fn zero_task_timeout_waits_for_execution_to_finish() {
        let result = await_with_optional_timeout(task_timeout_duration(0), async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            7
        })
        .await;

        assert_eq!(result, Ok(7));
    }

    #[tokio::test]
    async fn positive_task_timeout_still_applies() {
        assert_eq!(task_timeout_duration(1), Some(Duration::from_secs(1)));
        let result = await_with_optional_timeout(
            Some(Duration::from_millis(1)),
            tokio::time::sleep(Duration::from_millis(20)),
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn format_goal_iteration_prompt_renders_header_and_criteria() {
        let mut verified = evohime_local_storage::goal::GoalCriterionV1::new(
            "c1",
            evohime_local_storage::goal::GoalCriterionKind::Manual,
            "First step",
        );
        verified.status = evohime_local_storage::goal::GoalCriterionStatus::Verified;
        let pending = evohime_local_storage::goal::GoalCriterionV1::new(
            "c2",
            evohime_local_storage::goal::GoalCriterionKind::Manual,
            "Second step",
        );
        let goal = evohime_local_storage::goal::GoalV1 {
            id: "goal-1".into(),
            version: 1,
            workspace_id: "ws-1".into(),
            chat_id: None,
            objective: "Finish task".into(),
            success_criteria: vec![verified, pending],
            status: evohime_local_storage::goal::GoalStatus::Active,
            progress_summary: String::new(),
            completed_criteria: vec!["c1".into()],
            remaining_criteria: vec!["c2".into()],
            blockers: Vec::new(),
            next_action: None,
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: None,
            token_budget: None,
            cost_budget_micros: None,
            continuation_budget: None,
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: "core".into(),
            updated_by: "core".into(),
            content_hash: String::new(),
        };

        let rendered = format_goal_iteration_prompt("Base prompt", &goal, 0, 0);
        assert!(rendered.starts_with("<active_goal id=\"goal-1\" iteration=\"1/1\">\n"));
        assert!(rendered.contains("Цель: Finish task\nКритерии выполнения:\n- [verified] c1 (First step)\n- [pending] c2 (Second step)\n"));
        assert!(rendered.ends_with("</active_goal>\n\nBase prompt"));

        let first =
            format_goal_continuation_prompt("Base prompt", &goal, 1, 3, Some("Attempt 1 output"));
        assert!(!first.contains("<previous_attempt_summary>"));

        let second =
            format_goal_continuation_prompt("Base prompt", &goal, 2, 3, Some("Attempt 1 output"));
        assert!(second
            .contains("<previous_attempt_summary>\nAttempt 1 output\n</previous_attempt_summary>"));
    }

    #[test]
    fn goal_continuation_completes_second_attempt_without_ledger_or_request_collisions() {
        std::thread::Builder::new()
            .name("evohime-goal-continuation-test".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("test runtime builds")
                    .block_on(
                        goal_continuation_completes_second_attempt_without_ledger_or_request_collisions_inner(),
                    );
            })
            .expect("test thread starts")
            .join()
            .expect("test thread completes");
    }

    async fn goal_continuation_completes_second_attempt_without_ledger_or_request_collisions_inner()
    {
        use evohime_model_gateway::{
            providers::mock::MockProvider, ChatResult, ModelGateway, NativeToolCall,
        };
        use std::sync::Arc;

        let dir = tempfile::tempdir().expect("tempdir creates");
        let db_path = dir.path().join("events.db");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace creates");
        let journal = EventJournal::open(&db_path).expect("journal opens");

        let goal_id = "goal-cont-1";
        let owner_scope = crate::task_memory::project_scope_id(&workspace.to_string_lossy());
        let goal = evohime_local_storage::goal::GoalV1 {
            id: goal_id.into(),
            version: 1,
            workspace_id: owner_scope,
            chat_id: None,
            objective: "Complete criterion c1".into(),
            success_criteria: vec![evohime_local_storage::goal::GoalCriterionV1::new(
                "c1",
                evohime_local_storage::goal::GoalCriterionKind::Manual,
                "Verify c1",
            )],
            status: evohime_local_storage::goal::GoalStatus::Active,
            progress_summary: "Initial active goal".into(),
            completed_criteria: Vec::new(),
            remaining_criteria: vec!["c1".into()],
            blockers: Vec::new(),
            next_action: None,
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: None,
            token_budget: None,
            cost_budget_micros: None,
            continuation_budget: Some(3),
            created_at_ms: 1,
            updated_at_ms: 1,
            created_by: "core".into(),
            updated_by: "core".into(),
            content_hash: String::new(),
        };
        {
            let db = journal.database().lock().await;
            evohime_local_storage::goal::GoalStore::new(db.connection())
                .create(
                    &goal,
                    crate::goal::GoalCommand::new(
                        "core",
                        "create-goal-cont-1",
                        &crate::research::sha256_hex(b"create-goal-cont-1"),
                    ),
                )
                .expect("goal creates");
        }

        let provider = MockProvider::with_tool_call_sequence(
            "mock",
            vec![
                // Attempt 1, iteration 0: model finishes without goal.verify, triggering nudge.
                ChatResult {
                    content: "attempt 1 before nudge".into(),
                    ..ChatResult::default()
                },
                // Attempt 1, iteration 1: still finishes without calling goal.verify.
                ChatResult {
                    content: "attempt 1 after nudge".into(),
                    ..ChatResult::default()
                },
                // Attempt 2, iteration 0: model calls goal.verify.
                ChatResult {
                    content: String::new(),
                    tool_calls: vec![NativeToolCall {
                        id: "verify-call-1".into(),
                        name: "goal.verify".into(),
                        arguments: serde_json::json!({
                            "goal_id": goal_id,
                            "criterion_id": "c1",
                            "evidence_summary": "verified on second attempt"
                        })
                        .to_string(),
                    }],
                    ..ChatResult::default()
                },
                // Attempt 2, iteration 1: model completes the task.
                ChatResult {
                    content: "goal completed cleanly".into(),
                    ..ChatResult::default()
                },
            ],
        );
        let permissions = evohime_permissions::PermissionEngine::new();
        permissions
            .set_all_modes(evohime_permissions::PermissionMode::Allow)
            .await;
        let receipt_keys = Arc::new(evohime_receipts::key_lifecycle::ReceiptKeyManager::new(
            dir.path(),
        ));
        {
            let mut db = journal.database().lock().await;
            receipt_keys
                .startup_with_database(db.connection_mut())
                .expect("receipt keys initialize");
        }
        let agent = crate::ToolAgent::new(
            Arc::new(ModelGateway::from_provider(Arc::new(provider))),
            Arc::new(evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions)),
        )
        .with_journal(journal.clone())
        .with_receipt_keys(receipt_keys);

        let (coordinator, mut events) =
            TaskCoordinator::new_with_journal(8, Some(Arc::new(agent)), journal.clone());
        let task_id = "task-cont-success";
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: task_id.into(),
                prompt: "Finish the active goal".into(),
                workspace_root: Some(workspace),
                preferred_route_hint: None,
                conversation: None,
                goal_id: Some(goal_id.into()),
            })
            .await
            .expect("task dispatches");

        let mut observed = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(2), events.recv()).await {
                Ok(Ok(event)) => {
                    let is_terminal = matches!(
                        event,
                        CoreEvent::TaskCompleted { .. }
                            | CoreEvent::TaskFailed { .. }
                            | CoreEvent::TaskStopped { .. }
                    );
                    observed.push(event);
                    if is_terminal {
                        break;
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
            }
        }
        while let Ok(event) = events.try_recv() {
            observed.push(event);
        }

        let completed_count = observed
            .iter()
            .filter(|e| matches!(e, CoreEvent::TaskCompleted { .. }))
            .count();
        let failed_count = observed
            .iter()
            .filter(|e| matches!(e, CoreEvent::TaskFailed { .. }))
            .count();
        assert_eq!(completed_count, 1, "events={observed:?}");
        assert_eq!(failed_count, 0, "events={observed:?}");
        assert!(observed.iter().any(|e| matches!(
            e,
            CoreEvent::TaskCompleted { final_message, .. } if final_message == "goal completed cleanly"
        )));
        assert!(observed.iter().any(|e| matches!(
            e,
            CoreEvent::AssistantDelta { content, .. } if content.contains("attempt 1 after nudge")
        )));

        let db = journal.database().lock().await;
        let stored_goal = evohime_local_storage::goal::GoalStore::new(db.connection())
            .get(goal_id)
            .expect("goal queries")
            .expect("goal exists");
        assert_eq!(
            stored_goal.status,
            evohime_local_storage::goal::GoalStatus::Completed
        );

        let request_ids: Vec<String> = db
            .connection()
            .prepare("SELECT logical_request_id FROM model_requests WHERE logical_request_id LIKE ?1 ORDER BY rowid ASC")
            .expect("prepare model_requests")
            .query_map([format!("{task_id}:%")], |row| row.get(0))
            .expect("query model_requests")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect model_requests");
        assert!(
            request_ids
                .iter()
                .any(|id| id == &format!("{task_id}:{task_id}-0")),
            "request_ids={request_ids:?}"
        );
        assert!(
            request_ids
                .iter()
                .any(|id| id == &format!("{task_id}:{task_id}-c2-0")),
            "request_ids={request_ids:?}"
        );

        let ledger_count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM context_ledger WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .expect("count context_ledger");
        assert!(ledger_count >= 3, "ledger_count={ledger_count}");
    }

    #[test]
    fn goal_continuation_failure_on_second_attempt_emits_single_terminal_failure() {
        std::thread::Builder::new()
            .name("evohime-goal-continuation-fail-test".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("test runtime builds")
                    .block_on(
                        goal_continuation_failure_on_second_attempt_emits_single_terminal_failure_inner(),
                    );
            })
            .expect("test thread starts")
            .join()
            .expect("test thread completes");
    }

    async fn goal_continuation_failure_on_second_attempt_emits_single_terminal_failure_inner() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        struct FailOnSecondAttemptExecutor {
            attempts: AtomicU32,
        }

        impl crate::TaskExecutor for FailOnSecondAttemptExecutor {
            fn execute(
                &self,
                _task_id: String,
                _prompt: String,
                _cancellation: tokio_util::sync::CancellationToken,
                _events: crate::EventSink,
            ) -> futures_util::future::BoxFuture<'static, Result<String, crate::AgentRunError>>
            {
                Box::pin(async { Ok("unused".into()) })
            }

            fn execute_in_conversation(
                &self,
                _task_id: String,
                prompt: String,
                _workspace_root: std::path::PathBuf,
                context: crate::ConversationExecutionContext,
                _cancellation: tokio_util::sync::CancellationToken,
                _events: crate::EventSink,
            ) -> futures_util::future::BoxFuture<'static, Result<String, crate::AgentRunError>>
            {
                let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
                Box::pin(async move {
                    assert!(context.defer_completion_event);
                    assert_eq!(context.continuation_attempt, Some(attempt));
                    if attempt == 1 {
                        Ok("first attempt partial".into())
                    } else {
                        assert!(prompt.contains("first attempt partial"));
                        Err(crate::AgentRunError::Internal(
                            "repeated failure on attempt 2".into(),
                        ))
                    }
                })
            }
        }

        let dir = tempfile::tempdir().expect("tempdir creates");
        let db_path = dir.path().join("events.db");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace creates");
        let journal = EventJournal::open(&db_path).expect("journal opens");

        let goal_id = "goal-fail-2";
        let owner_scope = crate::task_memory::project_scope_id(&workspace.to_string_lossy());
        let goal = evohime_local_storage::goal::GoalV1 {
            id: goal_id.into(),
            version: 1,
            workspace_id: owner_scope,
            chat_id: None,
            objective: "Should fail cleanly on attempt 2".into(),
            success_criteria: vec![evohime_local_storage::goal::GoalCriterionV1::new(
                "c1",
                evohime_local_storage::goal::GoalCriterionKind::Manual,
                "Criterion",
            )],
            status: evohime_local_storage::goal::GoalStatus::Active,
            progress_summary: "Initial active goal".into(),
            completed_criteria: Vec::new(),
            remaining_criteria: vec!["c1".into()],
            blockers: Vec::new(),
            next_action: None,
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: None,
            token_budget: None,
            cost_budget_micros: None,
            continuation_budget: Some(3),
            created_at_ms: 1,
            updated_at_ms: 1,
            created_by: "core".into(),
            updated_by: "core".into(),
            content_hash: String::new(),
        };
        {
            let db = journal.database().lock().await;
            evohime_local_storage::goal::GoalStore::new(db.connection())
                .create(
                    &goal,
                    crate::goal::GoalCommand::new(
                        "core",
                        "create-goal-fail-2",
                        &crate::research::sha256_hex(b"create-goal-fail-2"),
                    ),
                )
                .expect("goal creates");
        }

        let executor = Arc::new(FailOnSecondAttemptExecutor {
            attempts: AtomicU32::new(0),
        });
        let (coordinator, mut events) =
            TaskCoordinator::new_with_journal(8, Some(executor), journal);
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-cont-fail".into(),
                prompt: "Run failing continuation".into(),
                workspace_root: Some(workspace),
                preferred_route_hint: None,
                conversation: None,
                goal_id: Some(goal_id.into()),
            })
            .await
            .expect("task dispatches");

        let mut observed = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(1), events.recv()).await {
                Ok(Ok(event)) => {
                    let is_terminal = matches!(
                        event,
                        CoreEvent::TaskCompleted { .. }
                            | CoreEvent::TaskFailed { .. }
                            | CoreEvent::TaskStopped { .. }
                    );
                    observed.push(event);
                    if is_terminal {
                        break;
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
            }
        }
        while let Ok(event) = events.try_recv() {
            observed.push(event);
        }

        assert_eq!(
            observed
                .iter()
                .filter(|e| matches!(e, CoreEvent::TaskCompleted { .. }))
                .count(),
            0,
            "events={observed:?}"
        );
        assert_eq!(
            observed
                .iter()
                .filter(|e| matches!(e, CoreEvent::TaskFailed { .. }))
                .count(),
            1,
            "events={observed:?}"
        );
    }
}
