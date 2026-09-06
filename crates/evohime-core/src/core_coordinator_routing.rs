use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::StartTask {
            task_id,
            prompt,
            workspace_root,
            preferred_route_hint,
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
            let _ = state_guard.events.send(CoreEvent::TaskStarted {
                task_id: task_id.clone(),
                prompt: prompt.clone(),
            });
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
            let Some(background_permit) = state.lock().await.background_tasks.try_acquire() else {
                let mut state_guard = state.lock().await;
                state_guard.tasks.remove(&task_id);
                let _ = state_guard.events.send(CoreEvent::TaskFailed {
                    task_id,
                    error: "background task capacity is exhausted".into(),
                });
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
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: warning,
                            });
                            return;
                        }
                        Err(error) => {
                            let mut state_guard = state.lock().await;
                            state_guard.tasks.remove(&task_id);
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!("task checkpoint recovery failed: {error}"),
                            });
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
                        let _ = state_guard.events.send(CoreEvent::TaskFailed {
                            task_id,
                            error: format!("task checkpoint could not be persisted: {error}"),
                        });
                        return;
                    }
                    if let Err(error) = journal
                        .begin_agent_run(&run_id, &task_id, &intent_hash)
                        .await
                    {
                        let mut state_guard = state.lock().await;
                        state_guard.tasks.remove(&task_id);
                        let _ = state_guard.events.send(CoreEvent::TaskFailed {
                            task_id,
                            error: format!("agent run could not acquire durable lease: {error}"),
                        });
                        return;
                    }
                }

                let heartbeat_cancel = CancellationToken::new();
                let heartbeat_failure = Arc::new(StdMutex::new(None::<String>));
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
                                            *failure.lock().expect("heartbeat failure lock") = Some(error.to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                        })
                    });
                // A task is a loop of model calls and tool runs, so its
                // budget must exceed one model call (120 s by default).
                // The old 60 s cut off agents that were working fine.
                let task_timeout_secs = std::env::var("EVOHIME_TASK_TIMEOUT_SECONDS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(DEFAULT_TASK_TIMEOUT_SECONDS);
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
                let mut result = Err(AgentRunError::Internal(
                    "continuation did not execute an attempt".into(),
                ));
                let mut continuation_index = continuation_context
                    .as_ref()
                    .map(|(run, _)| run.continuation_index)
                    .unwrap_or(0);
                loop {
                    let attempt = if let Some((run, policy)) = &continuation_context {
                        if policy.budget.max_wall_clock_ms.is_some_and(|limit| {
                            crate::task_memory::now_millis()
                                .saturating_sub(run.created_at_ms.max(0) as u64)
                                >= limit
                        }) {
                            if let Some(journal) = &journal {
                                if let Ok(database) = journal.database().try_lock() {
                                    let _ =
                                        evohime_local_storage::domains::runs::transition_run(
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
                        let mut database = journal
                            .as_ref()
                            .expect("continuation has a journal")
                            .database()
                            .lock()
                            .await;
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
                            Ok(false) => None,
                            Err(_) => {
                                let _ = state.lock().await.events.send(CoreEvent::TaskFailed {
                                    task_id: task_id.clone(),
                                    error: "continuation budget or state rejected the next attempt"
                                        .into(),
                                });
                                break;
                            }
                        }
                    } else {
                        None
                    };
                    result = match executor.as_ref() {
                        Some(executor) => match timeout(
                            Duration::from_secs(task_timeout_secs),
                            executor.execute_in_workspace_with_routing_hint(
                                task_id.clone(),
                                prompt.clone(),
                                workspace_root.clone(),
                                preferred_route_hint.clone(),
                                cancellation.clone(),
                                events.clone(),
                            ),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => Err(AgentRunError::Timeout(task_timeout_secs)),
                        },
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
                                    let database = journal
                                        .as_ref()
                                        .expect("continuation has a journal")
                                        .database()
                                        .lock()
                                        .await;
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
                        let database = journal
                            .as_ref()
                            .expect("continuation has a journal")
                            .database()
                            .lock()
                            .await;
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
                                unknown_outcome: result.is_err() || gate_unknown,
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
                            let _ = events.send(CoreEvent::ApprovalRequired {
                                task_id: task_id.clone(),
                                approval_id,
                                tool_name: "continuation_gate".into(),
                                permission: "continuation_gate".into(),
                                scope: workspace_root.to_string_lossy().chars().take(256).collect(),
                                preview: evohime_permissions::ApprovalPreview {
                                    kind: "continuation_gate".into(),
                                    summary: "Continuation gate requires user approval".into(),
                                    command: None,
                                    cwd: None,
                                    path: None,
                                    details: None,
                                    truncated: false,
                                },
                            });
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
                    if !matches!(decision, crate::continuation::Decision::Continue) {
                        break;
                    }
                }
                heartbeat_cancel.cancel();
                if let Some(heartbeat_task) = heartbeat_task {
                    let _ = heartbeat_task.await;
                }
                let heartbeat_error = heartbeat_failure
                    .lock()
                    .expect("heartbeat failure lock")
                    .clone();
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
                if let Err(error) = &result {
                    let _ = state_guard.events.send(CoreEvent::RoutingTrace {
                        task_id: task_id.clone(),
                        trace: routing_failure_trace(&run_id, error),
                    });
                }
                match (result, heartbeat_error) {
                    (Ok(_), Some(error)) => {
                        let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!(
                                    "agent run lease was lost; outcome requires reconciliation: {error}"
                                ),
                            });
                    }
                    (Ok(_), None) => {}
                    (Err(error), _) => {
                        let task_id = task_id;
                        if matches!(error, AgentRunError::Cancelled) {
                            let _ = state_guard.events.send(CoreEvent::TaskStopped { task_id });
                        } else {
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: error.to_string(),
                            });
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
            let executor = state.lock().await.executor.clone();
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
                executor.extract_ambient_memory(episode_id).await;
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
