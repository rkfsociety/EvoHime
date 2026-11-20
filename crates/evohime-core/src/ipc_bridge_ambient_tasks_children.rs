use super::*;

/// Только подтверждённый shell-клиент Electron владеет чатом Улья.
fn is_trusted_hive_shell(client: Option<&AuthenticatedIpcClient>) -> bool {
    client.is_some_and(|identity| identity.client_role == "shell")
}

fn start_task_route_hint(start: &generated::StartTask) -> Option<String> {
    if start.conversation_id == crate::conversation_event_log::HIVE_SYSTEM_CHAT_ID {
        return Some("hive".into());
    }
    match start.preferred_route_hint.as_str() {
        "local" | "cloud" => Some(start.preferred_route_hint.clone()),
        "codex_cli" if start.execution_kind == "coding" => Some("codex_cli".into()),
        // Route hints are user-selectable except for the isolated Hive route.
        _ => None,
    }
}

impl IpcBridge {
    pub(super) async fn dispatch_tasks_children<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        request_id: String,
        client_id: String,
        authenticated_client: Option<&AuthenticatedIpcClient>,
        command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
            Some(generated::command_envelope::Command::PermissionMode(request)) => {
                if let Some(tools) = &self.tools {
                    let mode = match request.mode.as_str() {
                        "full" => PermissionMode::Allow,
                        "read_only" => PermissionMode::Deny,
                        _ => PermissionMode::Ask,
                    };
                    tools.permissions().set_all_modes(mode).await;
                    if request.mode == "read_only" {
                        tools
                            .permissions()
                            .set_mode(Permission::FilesystemRead, PermissionMode::Allow)
                            .await;
                        tools
                            .permissions()
                            .set_mode(Permission::GitRead, PermissionMode::Allow)
                            .await;
                    }
                }
            }
            Some(generated::command_envelope::Command::CreateProject(request)) => {
                let result = self
                    .dispatch_create_project(client_id, request_id, command_hash, request)
                    .await?;
                self.write_response(writer, "project.created", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateTask(request)) => {
                let item = WorkItemRecord {
                    id: request.task_id,
                    project_id: request.project_id,
                    parent_id: (!request.parent_id.is_empty()).then_some(request.parent_id),
                    title: request.title,
                    description: request.description,
                    source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                    acceptance_criteria: request.acceptance_criteria,
                    non_goals: request.non_goals,
                    status: if request.status.is_empty() {
                        "backlog".into()
                    } else {
                        request.status
                    },
                    priority: request.priority,
                    estimate: (request.estimate != 0).then_some(request.estimate),
                    complexity: (!request.complexity.is_empty()).then_some(request.complexity),
                    attempt_count: 0,
                    version: 1,
                };
                let result = self
                    .dispatch_create_task(client_id, request_id, command_hash, item)
                    .await?;
                self.write_response(writer, "task.created", result).await?;
            }
            Some(generated::command_envelope::Command::UpdateTaskStatus(request)) => {
                let result = self
                    .dispatch_update_status(
                        client_id,
                        request_id,
                        command_hash,
                        request.task_id,
                        request.expected_version,
                        request.status,
                    )
                    .await?;
                self.write_response(writer, "task.status_updated", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::AddTaskEdge(request)) => {
                let result = self
                    .dispatch_add_edge(
                        client_id,
                        request_id,
                        command_hash,
                        request.from_task_id,
                        request.to_task_id,
                        request.kind,
                    )
                    .await?;
                self.write_response(writer, "task.edge_added", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetTaskGraph(request)) => {
                let result = self.dispatch_get_task_graph(request.project_id).await?;
                self.write_response(writer, "task.graph", result).await?;
            }
            Some(generated::command_envelope::Command::NextReadyTask(request)) => {
                let result = self.dispatch_next_ready_task(request.project_id).await?;
                self.write_response(writer, "task.next_ready", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ImportPrd(request)) => {
                let result = self
                    .dispatch_import_prd(client_id, request_id, command_hash, request)
                    .await?;
                self.write_response(writer, "prd.imported", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskHistory(request)) => {
                let result = self
                    .dispatch_get_task_history(request.task_id, request.limit as usize)
                    .await?;
                self.write_response(writer, "task.history", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskContext(request)) => {
                let result = self
                    .dispatch_get_task_context(
                        request.project_id,
                        request.task_id,
                        request.max_chars as usize,
                    )
                    .await?;
                self.write_response(writer, "task.context", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskPlanSpec(request)) => {
                let result = self
                    .dispatch_get_task_plan_spec(
                        request.project_id,
                        request.task_id,
                        request.max_chars as usize,
                    )
                    .await?;
                self.write_response(writer, "task.plan_spec", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ApplyApprovedBuild(request)) => {
                let result = self
                    .dispatch_apply_approved_build(
                        request.project_id,
                        request.run_id,
                        request.task_id,
                        request.approved_build_json,
                    )
                    .await?;
                self.write_response(writer, "build.applied", result).await?;
            }
            Some(generated::command_envelope::Command::PrepareBuild(request)) => {
                let result = self
                    .dispatch_prepare_build(request.project_id, request.proposal_json)
                    .await?;
                self.write_response(writer, "build.prepared", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetTaskSnapshot(request)) => {
                let task_id = request.task_id;
                let result = self
                    .dispatch_get_task_snapshot(request.project_id, task_id.clone())
                    .await?;
                self.write_task_response(writer, "task.snapshot", &task_id, result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RestoreTaskSnapshot(request)) => {
                let result = self
                    .dispatch_restore_task_snapshot(
                        request.project_id,
                        request.task_id,
                        request.snapshot_id,
                    )
                    .await?;
                self.write_response(writer, "snapshot.restored", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetBuildPolicy(request)) => {
                let result = self.dispatch_get_build_policy(request.project_id).await?;
                self.write_response(writer, "build.policy", result).await?;
            }
            Some(generated::command_envelope::Command::SaveBuildPolicy(request)) => {
                let result = self
                    .dispatch_save_build_policy(
                        request.project_id,
                        request.policy_json,
                        request.expected_version,
                    )
                    .await?;
                self.write_response(writer, "build.policy.saved", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::StartTask(start)) => {
                let trusted_hive_shell = is_trusted_hive_shell(authenticated_client);
                let hive_conversation =
                    start.conversation_id == crate::conversation_event_log::HIVE_SYSTEM_CHAT_ID;
                if hive_conversation && !trusted_hive_shell {
                    self.write_conversation_event_log_response(
                        writer,
                        conversation_event_log_error("accept", &start.conversation_id, "forbidden"),
                    )
                    .await?;
                    return Ok(());
                }
                let has_conversation = !start.conversation_id.is_empty();
                let has_client_message = !start.client_message_id.is_empty();
                if has_conversation != has_client_message {
                    self.write_conversation_event_log_response(
                        writer,
                        conversation_event_log_error(
                            "accept",
                            &start.conversation_id,
                            "invalid_argument",
                        ),
                    )
                    .await?;
                    return Ok(());
                }
                let mut should_dispatch = true;
                let mut conversation_context = None;
                if has_conversation {
                    let workspace_id = crate::task_memory::project_scope_id(&start.workspace_path);
                    let accepted = self
                        .journal
                        .accept_conversation_message(
                            &start.conversation_id,
                            &workspace_id,
                            &start.task_id,
                            &start.client_message_id,
                            &start.prompt,
                        )
                        .await;
                    let (acceptance, sequence) = match accepted {
                            Ok(value) => value,
                            Err(StorageError::ConversationEventLog(
                                evohime_local_storage::domains::audit::ConversationStoreError::IdempotencyConflict,
                            )) => {
                                self.write_conversation_event_log_response(
                                    writer,
                                    conversation_event_log_error(
                                        "accept",
                                        &start.conversation_id,
                                        "idempotency_conflict",
                                    ),
                                )
                                .await?;
                                return Ok(());
                            }
                            Err(error) => {
                                self.write_conversation_event_log_response(
                                    writer,
                                    conversation_event_log_error(
                                        "accept",
                                        &start.conversation_id,
                                        &conversation_accept_error_code(&error),
                                    ),
                                )
                                .await?;
                                return Ok(());
                            }
                        };
                    conversation_context = Some(crate::ConversationTaskContext {
                        conversation_id: start.conversation_id.clone(),
                        before_sequence: acceptance.event.sequence,
                    });
                    if let Some(coordinator) = &self.coordinator {
                        coordinator.notify_journalled(sequence.max(0) as u64);
                    }
                    should_dispatch = self
                        .journal
                        .claim_conversation_dispatch(
                            &start.conversation_id,
                            &start.client_message_id,
                        )
                        .await
                        .unwrap_or(false);
                    if !should_dispatch && acceptance.dispatch_state == "dispatching" {
                        self.write_conversation_event_log_response(
                            writer,
                            conversation_event_log_error(
                                "accept",
                                &start.conversation_id,
                                "dispatch_unknown",
                            ),
                        )
                        .await?;
                        return Ok(());
                    }
                }
                if should_dispatch {
                    let session_binding = build_session_binding_from_start_task(&start);
                    let preferred_route_hint = if trusted_hive_shell && hive_conversation {
                        Some("hive".into())
                    } else {
                        start_task_route_hint(&start)
                    };
                    if let Some(coordinator) = &self.coordinator {
                        let dispatched = coordinator
                            .dispatch(CoreCommand::StartTask {
                                task_id: start.task_id,
                                prompt: start.prompt,
                                workspace_root: (!start.workspace_path.is_empty())
                                    .then(|| std::path::PathBuf::from(start.workspace_path)),
                                preferred_route_hint,
                                conversation: conversation_context,
                                goal_id: (!start.goal_id.is_empty()).then_some(start.goal_id),
                                session_binding,
                            })
                            .await;
                        if has_conversation {
                            self.journal
                                .finish_conversation_dispatch(
                                    &start.conversation_id,
                                    &start.client_message_id,
                                    dispatched.is_ok(),
                                )
                                .await
                                .map_err(|error| FrameError::Io(error.to_string()))?;
                        }
                        dispatched.map_err(|error| FrameError::Io(error.to_string()))?;
                    } else if has_conversation {
                        self.journal
                            .finish_conversation_dispatch(
                                &start.conversation_id,
                                &start.client_message_id,
                                false,
                            )
                            .await
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                }
            }
            Some(generated::command_envelope::Command::GetTaskCheckpoint(request)) => {
                let projection = self.dispatch_get_task_checkpoint(request).await;
                self.write_task_checkpoint_projection(writer, projection)
                    .await?;
            }
            Some(generated::command_envelope::Command::ResolveTaskCheckpoint(request)) => {
                let result = self.dispatch_resolve_task_checkpoint(request).await?;
                self.write_task_checkpoint_action_result(writer, result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListSkills(request)) => {
                self.dispatch_list_skills(request, writer).await?;
            }
            Some(generated::command_envelope::Command::LoadSkill(request)) => {
                self.dispatch_load_skill(request, writer).await?;
            }
            Some(generated::command_envelope::Command::LoadSkillReference(request)) => {
                self.dispatch_load_skill_reference(request, writer).await?;
            }
            Some(generated::command_envelope::Command::CreateGoal(request)) => {
                let result = self.dispatch_create_goal(request, &command_hash).await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::GetGoal(request)) => {
                let projection = self.dispatch_get_goal(request).await;
                self.write_goal_projection(writer, projection).await?;
            }
            Some(generated::command_envelope::Command::ListGoals(request)) => {
                let projection = self.dispatch_list_goals(request).await;
                self.write_goal_list_projection(writer, projection).await?;
            }
            Some(generated::command_envelope::Command::PauseGoal(request)) => {
                let result = self
                    .dispatch_goal_transition(
                        request,
                        crate::goal::GoalStatus::Paused,
                        &command_hash,
                    )
                    .await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::ResumeGoal(request)) => {
                let result = self
                    .dispatch_goal_transition(
                        request,
                        crate::goal::GoalStatus::Active,
                        &command_hash,
                    )
                    .await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::CancelGoal(request)) => {
                let result = self
                    .dispatch_goal_transition(
                        request,
                        crate::goal::GoalStatus::Cancelled,
                        &command_hash,
                    )
                    .await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::UpdateGoal(request)) => {
                let result = self.dispatch_update_goal(request, &command_hash).await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::VerifyGoalCriterion(request)) => {
                let result = self
                    .dispatch_verify_goal_criterion(request, &command_hash)
                    .await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::LinkGoalReference(request)) => {
                let result = self
                    .dispatch_link_goal_reference(request, &command_hash)
                    .await;
                self.write_goal_action_result(writer, result).await?;
            }
            Some(generated::command_envelope::Command::SaveContinuationPolicy(request)) => {
                let result = self
                    .dispatch_save_continuation_policy(
                        request,
                        &client_id,
                        &request_id,
                        &command_hash,
                    )
                    .await;
                self.write_response(
                    writer,
                    "continuation.policy",
                    result.unwrap_or_else(error_response_payload),
                )
                .await?;
            }
            Some(generated::command_envelope::Command::StartContinuationRun(request)) => {
                let result = self.dispatch_start_continuation(request).await;
                let payload = result.unwrap_or_else(error_response_payload);
                if serde_json::from_slice::<serde_json::Value>(&payload)
                    .ok()
                    .and_then(|v| v.get("run_id").cloned())
                    .is_some()
                {
                    self.write_continuation_projection(writer, payload).await?;
                } else {
                    self.write_response(writer, "continuation.run", payload)
                        .await?;
                }
            }
            Some(generated::command_envelope::Command::GetContinuationRun(request)) => {
                let result = self.dispatch_get_continuation(request).await;
                let payload = result.unwrap_or_else(error_response_payload);
                if serde_json::from_slice::<serde_json::Value>(&payload)
                    .ok()
                    .and_then(|v| v.get("run_id").cloned())
                    .is_some()
                {
                    self.write_continuation_projection(writer, payload).await?;
                } else {
                    self.write_response(writer, "continuation.run", payload)
                        .await?;
                }
            }
            Some(generated::command_envelope::Command::StopContinuation(request)) => {
                let result = self.dispatch_stop_continuation(request).await;
                let payload = result.unwrap_or_else(error_response_payload);
                if serde_json::from_slice::<serde_json::Value>(&payload)
                    .ok()
                    .and_then(|v| v.get("run_id").cloned())
                    .is_some()
                {
                    self.write_continuation_action(writer, payload).await?;
                } else {
                    self.write_response(writer, "continuation.action", payload)
                        .await?;
                }
            }
            Some(generated::command_envelope::Command::ListRetainedChildren(request)) => {
                let (reply, response) = oneshot::channel();
                let parent_id = client_id.clone();
                let coordinator = self
                    .coordinator
                    .as_ref()
                    .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                coordinator
                    .dispatch(CoreCommand::ListRetainedChildren {
                        parent_id,
                        now_ms: crate::task_memory::now_millis(),
                        limit: request.limit,
                        reply,
                    })
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?;
                let payload = response
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?
                    .map_err(FrameError::Io)?;
                self.write_response(writer, "retained_child.list", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::HiveState(_)) => {
                let snapshot = self.hive_state_snapshot().await;
                self.write_response(
                    writer,
                    "hive_state.snapshot",
                    serde_json::to_vec(&snapshot)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::TogglePulse(request)) => {
                let payload = match self.toggle_pulse(request.enabled) {
                    Ok(payload) => payload,
                    Err(error) => error_response_payload(error),
                };
                self.write_response(writer, "pulse.status", payload).await?;
            }
            Some(generated::command_envelope::Command::EvaPulseStatus(_)) => {
                let payload = match self.pulse.as_ref() {
                    Some(pulse) => serde_json::to_vec(&pulse.snapshot())?,
                    None => serde_json::to_vec(&serde_json::json!({
                        "schema_version": 1,
                        "owner": "eva_pulse",
                        "status": "unavailable",
                        "enabled": false,
                        "next_wake_at_ms": null,
                    }))?,
                };
                self.write_response(writer, "eva_pulse.status", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::SetEvaWorkspacePath(request)) => {
                let payload = self
                    .save_eva_workspace_path(&request.path)
                    .await
                    .unwrap_or_else(error_response_payload);
                self.write_response(writer, "hive_settings.saved", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::ValidateEvaWorkspacePath(request)) => {
                let payload = self
                    .validate_eva_workspace_path_payload(&request.path)
                    .unwrap_or_else(error_response_payload);
                self.write_response(writer, "hive_settings.validated", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::SetHiveSetting(request)) => {
                let payload = self
                    .set_hive_setting(&request.key, &request.value)
                    .await
                    .unwrap_or_else(error_response_payload);
                self.write_response(writer, "hive_settings.saved", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetRetainedChild(request)) => {
                let (reply, response) = oneshot::channel();
                let coordinator = self
                    .coordinator
                    .as_ref()
                    .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                coordinator
                    .dispatch(CoreCommand::GetRetainedChild {
                        parent_id: client_id.clone(),
                        child_id: request.child_id,
                        now_ms: crate::task_memory::now_millis(),
                        reply,
                    })
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?;
                let payload = response
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?
                    .map_err(FrameError::Io)?;
                self.write_response(writer, "retained_child", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::RetainChild(request)) => {
                let (reply, response) = oneshot::channel();
                let now_ms = crate::task_memory::now_millis();
                let child = crate::retained_child::RetainedChildV1 {
                    version: 1,
                    child_id: request.child_id,
                    parent_id: client_id.clone(),
                    family_root_id: if request.family_root_id.is_empty() {
                        client_id.clone()
                    } else {
                        request.family_root_id
                    },
                    role: request.role,
                    stable_name: (!request.stable_name.is_empty()).then_some(request.stable_name),
                    lifecycle: crate::retained_child::RetainedLifecycle::Active,
                    revision: request.revision,
                    active_session_id: None,
                    grant_snapshot_hash: request.grant_snapshot_hash,
                    context_scope_hash: request.context_scope_hash,
                    workspace_state_ref: (!request.workspace_state_ref.is_empty())
                        .then_some(request.workspace_state_ref),
                    last_report_ref: (!request.last_report_ref.is_empty())
                        .then_some(request.last_report_ref),
                    retained_until_ms: if request.retained_until_ms == 0 {
                        now_ms.saturating_add(crate::retained_child::DEFAULT_TTL_MS)
                    } else {
                        request.retained_until_ms
                    },
                    created_at_ms: if request.created_at_ms == 0 {
                        now_ms
                    } else {
                        request.created_at_ms
                    },
                    last_active_at_ms: if request.last_active_at_ms == 0 {
                        now_ms
                    } else {
                        request.last_active_at_ms
                    },
                    registry_version: request.expected_registry_version.saturating_add(1),
                };
                let coordinator = self
                    .coordinator
                    .as_ref()
                    .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                coordinator
                    .dispatch(CoreCommand::RetainChild {
                        child,
                        now_ms,
                        reply,
                    })
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?;
                let payload = response
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?
                    .map_err(FrameError::Io)?;
                self.write_response(writer, "retained_child.retained", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::SendChildFollowUp(request)) => {
                let (reply, response) = oneshot::channel();
                let mode = match request.mode.as_str() {
                    "auto" => crate::retained_child::FollowUpMode::Auto,
                    "follow_up" | "" => crate::retained_child::FollowUpMode::FollowUp,
                    "steer" => crate::retained_child::FollowUpMode::Steer,
                    _ => {
                        self.write_response(
                            writer,
                            "retained_child.follow_up",
                            b"{\"error_code\":\"invalid_scope\"}".to_vec(),
                        )
                        .await?;
                        return Ok(());
                    }
                };
                let follow = crate::retained_child::ChildFollowUpRequestV1 {
                    version: 1,
                    idempotency_key: request.idempotency_key,
                    parent_id: client_id.clone(),
                    child_id: request.child_id,
                    family_root_id: client_id.clone(),
                    parent_sequence: 0,
                    expected_child_revision: request.expected_child_revision,
                    instruction: request.instruction,
                    context_refs: request.context_refs,
                    requested_grants: request.requested_grants,
                    budget_json: request.budget_json,
                    mode,
                    correlation_id: request.correlation_id,
                };
                let coordinator = self
                    .coordinator
                    .as_ref()
                    .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                coordinator
                    .dispatch(CoreCommand::SendChildFollowUp {
                        request: follow,
                        now_ms: crate::task_memory::now_millis(),
                        busy: false,
                        reply,
                    })
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?;
                let payload = response
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?
                    .map_err(FrameError::Io)?;
                self.write_response(writer, "retained_child.follow_up", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::DeleteRetainedChild(request)) => {
                let (reply, response) = oneshot::channel();
                let coordinator = self
                    .coordinator
                    .as_ref()
                    .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                coordinator
                    .dispatch(CoreCommand::DeleteRetainedChild {
                        parent_id: client_id.clone(),
                        child_id: request.child_id,
                        expected_registry_version: request.expected_registry_version,
                        reply,
                    })
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?;
                let payload = response
                    .await
                    .map_err(|e| FrameError::Io(e.to_string()))?
                    .map_err(FrameError::Io)?;
                self.write_response(writer, "retained_child.delete", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateAnalysisKernel(request)) => {
                let projection = self.dispatch_create_analysis_kernel(request).await;
                write_analysis_kernel_projection(
                    writer,
                    projection,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::GetAnalysisKernel(request)) => {
                let projection = self.dispatch_get_analysis_kernel(request).await;
                write_analysis_kernel_projection(
                    writer,
                    projection,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ListAnalysisKernels(request)) => {
                let payload = self.dispatch_list_analysis_kernels(request).await;
                self.write_response(writer, "analysis_kernel.list", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExecuteAnalysisKernel(request)) => {
                let result = self.dispatch_execute_analysis_kernel(request).await;
                write_analysis_kernel_result(
                    writer,
                    result,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ResetAnalysisKernel(request)) => {
                let result = self.dispatch_reset_analysis_kernel(request).await;
                write_analysis_kernel_result(
                    writer,
                    result,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ListRefinementCandidates(request)) => {
                let projection = self.dispatch_list_refinement_candidates(request).await;
                write_refinement_list_projection(
                    writer,
                    projection,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::GetRefinementCandidate(request)) => {
                let projection = self.dispatch_get_refinement_candidate(request).await;
                write_refinement_projection(
                    writer,
                    projection,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::RefinementAction(request)) => {
                let result = self.dispatch_refinement_action(request).await;
                write_refinement_action_result(
                    writer,
                    result,
                    &self.core_instance_id,
                    self.session_epoch,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::PreviewWorkflowPackage(request)) => {
                let result = crate::workflow_package::preview_from_json(
                    &request.graph_json,
                    request.name,
                    request.description,
                    request.portable_argument_keys,
                    &request.credential_slots_json,
                    request.created_at,
                )
                .map(|preview| {
                    serde_json::json!({
                        "status": "previewed",
                        "package_hash": preview.package_hash,
                        "stripped_fields": preview.stripped_fields,
                        "package": preview.package,
                    })
                })
                .map_err(
                    |error| serde_json::json!({"status":"rejected","error_code":error.to_string()}),
                );
                let payload = match result {
                    Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                };
                self.write_package_response(writer, "preview", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExportWorkflowPackage(request)) => {
                let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .and_then(|preview| {
                        crate::workflow_package::write_package(
                            std::path::Path::new(&request.destination_path),
                            &preview.package,
                        )?;
                        Ok(serde_json::json!({"status":"exported","package_hash":preview.package_hash,"stripped_fields":preview.stripped_fields}))
                    })
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                let payload = match result {
                    Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                };
                self.write_package_response(writer, "export", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::CommitWorkflowPackage(request)) => {
                let result = async {
                        let package = crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::commit_import(
                            &database,
                            std::path::Path::new(&request.source_path),
                            &package,
                            &request.idempotency_key,
                            now_ms(),
                        )
                    }
                    .await
                    .map(|record| serde_json::json!({"status":"committed","import_id":record.import_id,"local_workflow_id":record.local_workflow_id,"package_hash":record.package_hash}))
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                let payload = match result {
                    Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                };
                self.write_package_response(writer, "commit", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::RebindWorkflowPackage(request)) => {
                let result = async {
                    let package = crate::workflow_package::parse_bounded(&request.package_json)?;
                    let database = self.journal.database().lock().await;
                    crate::workflow_package::persist_rebind(
                        &database,
                        &package,
                        &request.slot_id,
                        &request.local_credential_reference,
                        now_ms(),
                    )
                }
                .await;
                let payload = match result {
                    Ok(value) => serde_json::to_vec(
                        &serde_json::json!({"status":"rebound","binding":value}),
                    )?,
                    Err(error) => serde_json::to_vec(
                        &serde_json::json!({"status":"rejected","error_code":error.to_string()}),
                    )?,
                };
                self.write_package_response(writer, "rebind", payload)
                    .await?;
            }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}

fn build_session_binding_from_start_task(
    start: &generated::StartTask,
) -> Option<crate::SessionBinding> {
    let provider = (!start.provider.trim().is_empty()).then(|| start.provider.trim().to_owned());
    let profile_id =
        (!start.profile_id.trim().is_empty()).then(|| start.profile_id.trim().to_owned());
    let model = (!start.model.trim().is_empty()).then(|| start.model.trim().to_owned());
    let permission_mode = match start.permission_mode.trim() {
        "full" => Some(PermissionMode::Allow),
        "read_only" => Some(PermissionMode::Deny),
        "ask" => Some(PermissionMode::Ask),
        _ => None,
    };
    let isolation_mode = match start.isolation_mode.trim() {
        "isolated_worktree" => crate::IsolationMode::IsolatedWorktree,
        _ => crate::IsolationMode::SharedWorkspace,
    };
    if provider.is_none()
        && profile_id.is_none()
        && model.is_none()
        && permission_mode.is_none()
        && isolation_mode == crate::IsolationMode::SharedWorkspace
    {
        return None;
    }
    Some(crate::SessionBinding {
        provider,
        profile_id,
        model,
        permission_mode,
        isolation_mode,
    })
}

#[cfg(test)]
mod start_task_route_tests {
    use super::start_task_route_hint;
    use evohime_desktop_ipc::generated;

    #[test]
    fn only_hive_system_conversation_can_select_the_hive_route() {
        let ordinary = generated::StartTask {
            conversation_id: "ordinary-chat".into(),
            preferred_route_hint: "hive".into(),
            ..Default::default()
        };
        assert_eq!(start_task_route_hint(&ordinary), None);

        let unbound = generated::StartTask {
            preferred_route_hint: "hive".into(),
            ..Default::default()
        };
        assert_eq!(start_task_route_hint(&unbound), None);

        let hive = generated::StartTask {
            conversation_id: "hive-system-chat".into(),
            preferred_route_hint: "cloud".into(),
            ..Default::default()
        };
        assert_eq!(start_task_route_hint(&hive).as_deref(), Some("hive"));
    }

    #[test]
    fn hive_chat_requires_the_authenticated_shell_role() {
        let client = |client_role: &str| super::super::AuthenticatedIpcClient {
            client_id: "electron-shell".into(),
            client_role: client_role.into(),
        };
        assert!(super::is_trusted_hive_shell(Some(&client("shell"))));
        assert!(!super::is_trusted_hive_shell(Some(&client("cli"))));
        assert!(!super::is_trusted_hive_shell(Some(&client("listener"))));
        assert!(!super::is_trusted_hive_shell(None));
    }
}
