use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::WorkspaceStateCheckpoint {
            operation,
            project_id,
            task_id,
            checkpoint_id,
            payload: _payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let root = journal.get_project(&project_id).await.map_err(|e| e.to_string())?
                        .or(journal.get_project_by_workspace_path(&project_id).await.map_err(|e| e.to_string())?)
                        .map(|project| std::path::PathBuf::from(project.workspace_path))
                        .unwrap_or_else(|| std::path::PathBuf::from(&project_id));
                    if !root.is_dir() {
                        return Err("project workspace not found".to_string());
                    }
                    let workspace_id = crate::task_memory::workspace_scope_id(&root);
                    let now = crate::task_memory::now_millis() as i64;
                    match operation.as_str() {
                        "list" => {
                            let summaries = {
                                let database = journal.database().lock().await;
                                evohime_local_storage::workspace_state_checkpoint::list_checkpoint_summaries(
                                    database.connection(), &workspace_id)
                                    .map_err(|e| e.to_string())?
                            };
                            serde_json::to_vec(&serde_json::json!({
                                "schema_version": 1,
                                "operation": "list",
                                "project_id": project_id,
                                "state": "listed",
                                "checkpoints": summaries,
                            })).map_err(|e| e.to_string())
                        }
                        "create" => {
                            let id = checkpoint_id.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
                            let checkpoint = match crate::workspace_state_checkpoints::capture(
                                &root, id.clone(), workspace_id.clone(), task_id.clone()) {
                                Ok(checkpoint) => checkpoint,
                                Err(error) => {
                                    let error_text = error.to_string();
                                    let error_code = if error_text.contains("file_bytes") {
                                        "workspace_checkpoint_file_too_large"
                                    } else if error_text.contains("snapshot_bytes") {
                                        "workspace_checkpoint_snapshot_too_large"
                                    } else if error_text.contains("files") {
                                        "workspace_checkpoint_too_many_files"
                                    } else {
                                        "workspace_checkpoint_capture_failed"
                                    };
                                    return serde_json::to_vec(&serde_json::json!({
                                        "schema_version": 1,
                                        "operation": "create",
                                        "checkpoint_id": id,
                                        "project_id": project_id,
                                        "task_id": task_id,
                                        "state": "failed",
                                        "error_code": error_code,
                                        "message": error_text,
                                    })).map_err(|e| e.to_string());
                                }
                            };
                            let json = serde_json::to_vec(&checkpoint).map_err(|e| e.to_string())?;
                            let record = evohime_local_storage::workspace_state_checkpoint::WorkspaceCheckpointRecord {
                                checkpoint_id: id.clone(), workspace_id: workspace_id.clone(), task_id: task_id.clone(),
                                snapshot_hash: checkpoint.baseline_hash.clone(), manifest_json: json, created_at_ms: now, pinned: false,
                            };
                            let database = journal.database().lock().await;
                            evohime_local_storage::workspace_state_checkpoint::insert_checkpoint(database.connection(), &record)
                                .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":"create","checkpoint_id":id,"project_id":project_id,"task_id":task_id,"state":"completed","file_count":checkpoint.files.len(),"snapshot_hash":checkpoint.baseline_hash})).map_err(|e| e.to_string())
                        }
                        "compare" | "restore" | "restore_both" | "restore_task" => {
                            let id = checkpoint_id.ok_or_else(|| "checkpoint_id is required".to_string())?;
                            let database = journal.database().lock().await;
                            let record = evohime_local_storage::workspace_state_checkpoint::get_checkpoint(database.connection(), &id)
                                .map_err(|e| e.to_string())?.ok_or_else(|| "checkpoint not found".to_string())?;
                            if record.workspace_id != workspace_id {
                                return Err("checkpoint does not belong to workspace".to_string());
                            }
                            if let Some(expected_task) = record.task_id.as_deref() {
                                if task_id.as_deref() != Some(expected_task) && operation != "restore" {
                                    return Err("checkpoint does not belong to task".to_string());
                                }
                            }
                            let checkpoint: crate::workspace_state_checkpoints::WorkspaceStateCheckpoint = serde_json::from_slice(&record.manifest_json).map_err(|e| e.to_string())?;
                            drop(database);
                            let conflicts = crate::workspace_state_checkpoints::compare(&root, &checkpoint).map_err(|e| e.to_string())?;
                            if operation == "compare" || operation == "restore_task" {
                                return serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":operation,"checkpoint_id":id,"project_id":project_id,"task_id":task_id,"state":if operation == "restore_task" { "task_projection_restored" } else { "compared" },"conflict_count":conflicts.len()})).map_err(|e| e.to_string());
                            }
                            if !conflicts.is_empty() {
                                let database = journal.database().lock().await;
                                let detail = serde_json::to_vec(&serde_json::json!({"conflict_count": conflicts.len()})).unwrap_or_default();
                                let operation_id = format!("{}:conflict", if idempotency_key.is_empty() { uuid::Uuid::now_v7().to_string() } else { idempotency_key.clone() });
                                let _ = evohime_local_storage::workspace_state_checkpoint::append_restore_journal(database.connection(), &evohime_local_storage::workspace_state_checkpoint::RestoreJournalRecord { operation_id, checkpoint_id: id.clone(), operation: operation.clone(), state: "conflict".into(), detail_json: detail, created_at_ms: now });
                                let response = match serde_json::to_string(&serde_json::json!({"error_code":"workspace_conflict","conflict_count":conflicts.len()})) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        tracing::warn!(%error, "failed to serialize workspace conflict response");
                                        "workspace conflict".into()
                                    }
                                };
                                return Err(response);
                            }
                            crate::workspace_state_checkpoints::restore(&root, &checkpoint).map_err(|e| e.to_string())?;
                            let database = journal.database().lock().await;
                            let detail = serde_json::to_vec(&serde_json::json!({"expected_version": expected_version})).unwrap_or_default();
                            let operation_id = format!("{}:completed", if idempotency_key.is_empty() { uuid::Uuid::now_v7().to_string() } else { idempotency_key.clone() });
                            evohime_local_storage::workspace_state_checkpoint::append_restore_journal(database.connection(), &evohime_local_storage::workspace_state_checkpoint::RestoreJournalRecord { operation_id, checkpoint_id: id.clone(), operation: operation.clone(), state: "completed".into(), detail_json: detail, created_at_ms: now }).map_err(|e| e.to_string())?;
                            let state = if operation == "restore_both" { "workspace_and_task_projection_restored" } else { "workspace_restored" };
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":operation,"checkpoint_id":id,"project_id":project_id,"task_id":task_id,"state":state,"conflict_count":0})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported workspace checkpoint operation".to_string()),
                    }
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::IncrementalChangeProtocol {
            operation,
            run_id,
            payload,
            expected_version,
            observed_fingerprint,
            idempotency_key,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let runtime = crate::incremental_change_protocol::Runtime::new(journal);
                    let now = crate::task_memory::now_millis() as i64;
                    let value = match operation.as_str() {
                        "create" => {
                            #[derive(Deserialize)]
                            struct Request { delta: crate::incremental_change_protocol::RequirementDelta, impact: crate::incremental_change_protocol::ImpactAnalysis, plan: crate::incremental_change_protocol::ChangePlan }
                            let request: Request = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            runtime.create(&run_id, &idempotency_key, &request.delta, &request.impact, &request.plan, now).await.map_err(|e| e.to_string())?
                        }
                        "apply" | "cancel" | "unknown" => {
                            let next = match operation.as_str() { "apply" => crate::incremental_change_protocol::State::Applied, "cancel" => crate::incremental_change_protocol::State::Cancelled, _ => crate::incremental_change_protocol::State::UnknownReconciliationRequired };
                            runtime.transition(&run_id, expected_version, next, &observed_fingerprint, now).await.map_err(|e| e.to_string())?
                        }
                        _ => return Err("unsupported incremental change operation".to_string()),
                    };
                    serde_json::to_vec(&value).map_err(|e| e.to_string())
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::RevisionSafeWorkspaceFiles {
            operation,
            project_id,
            logical_path,
            content: _,
            expected_hash: _,
            idempotency_key: _,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|e| e.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let ctx = evohime_tool_runtime::ToolContext {
                        workspace_root: project.workspace_path.into(),
                        task_id: uuid::Uuid::nil(),
                        session_id: None,
                        progress_tx: None,
                    };
                    let value = match operation.as_str() {
                        "read" => {
                            let (file_ref, text) = evohime_tool_runtime::revision_safe_workspace_files::read(&ctx, &logical_path).await.map_err(|e| e.to_string())?;
                            serde_json::json!({"status":"ok","ref":file_ref,"preview":text.chars().take(20000).collect::<String>()})
                        }
                        "write" => return Err("filesystem mutations must use the approved tool boundary".to_string()),
                        _ => return Err("unsupported revision-safe workspace files operation".to_string()),
                    };
                    serde_json::to_vec(&value).map_err(|e| e.to_string())
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::TaskWorktreeIsolation {
            operation,
            project_id,
            task_id,
            worktree_id,
            branch,
            base_commit,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|e| e.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    if branch.is_empty() || branch.len() > task_worktree_isolation::MAX_BRANCH_BYTES || branch.starts_with('-') || branch.contains("..") || branch.contains(' ') { return Err("invalid worktree branch".to_string()); }
                    if !matches!(operation.as_str(), "ready" | "integrating" | "cleanup_pending") { return Err("unsupported worktree transition".to_string()); }
                    let connection = journal.database().lock().await;
                    if operation == "create" {
                        let record = evohime_local_storage::task_worktree_isolation_store::TaskWorktreeRecord { worktree_id: worktree_id.clone(), task_id, repository_scope: project.id, branch, root_ref: format!(".evohime/worktrees/{worktree_id}"), base_commit, state: "planned".into(), version: 1, idempotency_key, updated_at_ms: crate::task_memory::now_millis() as i64 };
                        evohime_local_storage::task_worktree_isolation_store::create(connection.connection(), &record).map_err(|e| e.to_string())?;
                        return serde_json::to_vec(&record).map_err(|e| e.to_string());
                    }
                    let current = evohime_local_storage::task_worktree_isolation_store::get(connection.connection(), &worktree_id).map_err(|e| e.to_string())?.ok_or_else(|| "worktree not found".to_string())?;
                    if operation == "ready" && !std::path::PathBuf::from(&project.workspace_path).join(&current.root_ref).is_dir() {
                        return Err("worktree root is not present; create it through the approved git.worktree.create tool".to_string());
                    }
                    let ok = evohime_local_storage::task_worktree_isolation_store::transition(connection.connection(), &worktree_id, expected_version, &operation, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                    if !ok { return Err("stale or unknown worktree transition".to_string()); }
                    let record = evohime_local_storage::task_worktree_isolation_store::get(connection.connection(), &worktree_id).map_err(|e| e.to_string())?.ok_or_else(|| "worktree not found".to_string())?;
                    serde_json::to_vec(&record).map_err(|e| e.to_string())
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::TeamResourceBudget {
            operation,
            owner_scope,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    match operation.as_str() {
                        "validate_policy" | "save_policy" => {
                            let policy: team_resource_budget::TeamBudgetPolicy = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            team_resource_budget::validate_hash(&policy).map_err(|e| e.to_string())?;
                            if operation == "save_policy" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let json = serde_json::to_string(&policy).map_err(|e| e.to_string())?;
                                let inserted = evohime_local_storage::team_resource_budget_store::put_policy(connection.connection(), &owner_scope, policy.version, &json, &policy.content_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({"status":if inserted { "saved" } else { "duplicate" },"policy_id":policy.id,"policy_version":policy.version,"content_hash":policy.content_hash})).map_err(|e| e.to_string());
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"valid","policy_id":policy.id,"policy_version":policy.version,"content_hash":policy.content_hash})).map_err(|e| e.to_string())
                        }
                        "save_state" => {
                            let state_value: team_resource_budget::TeamBudgetState = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            if state_value.schema_version != team_resource_budget::SCHEMA_VERSION || state_value.team_session_id.is_empty() { return Err("invalid team budget state".to_string()); }
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let json = serde_json::to_string(&state_value).map_err(|e| e.to_string())?;
                            let saved = evohime_local_storage::team_resource_budget_store::put_state(connection.connection(), &state_value.team_session_id, state_value.policy_version, &json, if expected_version == 0 { None } else { Some(expected_version) }, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return Err("duplicate or stale team budget state".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","team_session_id":state_value.team_session_id,"version":state_value.version.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        "record_usage" => {
                            let event: team_resource_budget::ResourceUsageEvent = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            if event.schema_version != team_resource_budget::SCHEMA_VERSION || event.id.is_empty() || event.team_session_id.is_empty() || event.run_id.is_empty() || event.operation_kind.is_empty() { return Err("invalid team resource usage event".to_string()); }
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let json = serde_json::to_string(&event).map_err(|e| e.to_string())?;
                            let inserted = evohime_local_storage::team_resource_budget_store::append_usage(connection.connection(), &event.id, &event.team_session_id, &json, event.observed_at_ms).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if inserted { "recorded" } else { "duplicate" },"usage_id":event.id,"uncertain":event.uncertain,"idempotency_key":idempotency_key})).map_err(|e| e.to_string())
                        }
                        "preflight" => {
                            let request: TeamBudgetPreflightRequest = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            let decision = team_resource_budget::preflight_charge(&request.state, &request.policy, &request.estimate, request.reserve_access, request.unknown_cost).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":format!("{decision:?}").to_lowercase()})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported team resource budget operation".to_string()),
                    }
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::ComposableTerminationConditions {
            operation,
            owner_scope,
            payload,
            expected_version,
            idempotency_key: _,
            reply,
        } => {
            let result = async {
                    match operation.as_str() {
                        "validate_policy" | "save_policy" => {
                            let policy: composable_termination_conditions::TerminationPolicy = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            composable_termination_conditions::validate_hash(&policy).map_err(|e| e.to_string())?;
                            if operation == "save_policy" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let json = serde_json::to_string(&policy).map_err(|e| e.to_string())?;
                                let saved = evohime_local_storage::composable_termination_conditions_store::put_policy(connection.connection(), &owner_scope, policy.version, &json, &policy.content_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({"status":if saved { "saved" } else { "duplicate" },"policy_id":policy.id,"content_hash":policy.content_hash})).map_err(|e| e.to_string());
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"valid","policy_id":policy.id,"content_hash":policy.content_hash})).map_err(|e| e.to_string())
                        }
                        "evaluate" => {
                            let request: TerminationEvaluateRequest = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            let decision = composable_termination_conditions::evaluate_policy(&request.policy, &request.state, &request.event).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({
                                "status": "evaluated",
                                "decision": decision,
                                "hard_stop": request.policy.hard_stop,
                                "counters": {
                                    "messages": request.event.messages,
                                    "turns": request.event.turns,
                                    "tool_calls": request.event.tool_calls,
                                    "input_tokens": request.event.input_tokens,
                                    "output_tokens": request.event.output_tokens,
                                    "cost_micros": request.event.cost_micros,
                                    "elapsed_ms": request.event.elapsed_ms,
                                    "idle_ms": request.event.idle_ms,
                                },
                            })).map_err(|e| e.to_string())
                        }
                        "save_state" => {
                            let request: TerminationSaveStateRequest = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            let state_value = request.state;
                            let run_id = request.run_id.as_str();
                            let policy_id = request.policy_id.as_str();
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let json = serde_json::to_string(&state_value).map_err(|e| e.to_string())?;
                            let saved = evohime_local_storage::composable_termination_conditions_store::put_state(connection.connection(), run_id, policy_id, &json, expected_version, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return Err("duplicate or stale termination state".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","run_id":run_id,"version":state_value.version.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported termination operation".to_string()),
                    }
                }.await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
