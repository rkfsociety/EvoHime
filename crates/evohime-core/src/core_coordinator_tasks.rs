use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::CreateProject {
            client_id,
            request_id,
            command_hash,
            project_id,
            title,
            workspace_path,
            source_ref,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if let Some(replay) = journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                    .await
                    .map_err(|error| error.to_string())?
                {
                    return Ok(replay);
                }
                let project = journal
                    .create_project(&project_id, &title, &workspace_path, source_ref.as_deref())
                    .await
                    .map_err(|error| error.to_string())?;
                let result = serde_json::to_vec(&serde_json::json!({
                    "project_id": project.id,
                    "title": project.title,
                    "workspace_path": project.workspace_path,
                    "version": project.version,
                }))
                .map_err(|error| error.to_string())?;
                journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(result)
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::CreateTask {
            client_id,
            request_id,
            command_hash,
            item,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if let Some(replay) = journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                    .await
                    .map_err(|error| error.to_string())?
                {
                    return Ok(replay);
                }
                let created = journal
                    .create_work_item(&item)
                    .await
                    .map_err(|error| error.to_string())?;
                let result = serde_json::to_vec(&serde_json::json!({
                    "task_id": created.id,
                    "project_id": created.project_id,
                    "status": created.status,
                    "version": created.version,
                }))
                .map_err(|error| error.to_string())?;
                journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(result)
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::UpdateTaskStatus {
            client_id,
            request_id,
            command_hash,
            task_id,
            expected_version,
            status,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if let Some(replay) = journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                    .await
                    .map_err(|error| error.to_string())?
                {
                    return Ok(replay);
                }
                let updated = journal
                    .update_work_item_status(&task_id, expected_version, &status)
                    .await
                    .map_err(|error| error.to_string())?;
                let result = serde_json::to_vec(&serde_json::json!({
                    "task_id": updated.id,
                    "status": updated.status,
                    "version": updated.version,
                }))
                .map_err(|error| error.to_string())?;
                journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(result)
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::AddTaskEdge {
            client_id,
            request_id,
            command_hash,
            from_task_id,
            to_task_id,
            kind,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if let Some(replay) = journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                    .await
                    .map_err(|error| error.to_string())?
                {
                    return Ok(replay);
                }
                journal
                    .add_dependency(&from_task_id, &to_task_id, &kind)
                    .await
                    .map_err(|error| error.to_string())?;
                let result = br#"{"from_task_id":"ok"}"#.to_vec();
                journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(result)
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetTaskGraph { project_id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let (tasks, edges) = journal
                    .list_task_graph(&project_id)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "project_id": project_id,
                    "tasks": tasks,
                    "edges": edges.into_iter().map(|(from, to, kind)| serde_json::json!({
                        "from_task_id": from,
                        "to_task_id": to,
                        "kind": kind,
                    })).collect::<Vec<_>>(),
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::NextReadyTask { project_id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let task = journal
                    .next_ready_task(&project_id)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "project_id": project_id,
                    "task": task,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ImportPrd {
            client_id,
            request_id,
            command_hash,
            import_id,
            project_id,
            origin,
            version,
            source_text,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if let Some(replay) = journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                    .await
                    .map_err(|error| error.to_string())?
                {
                    return Ok(replay);
                }
                let parsed = crate::prd::parse_markdown_prd(&source_text, &origin, &version);
                if !parsed.diagnostics.is_empty() {
                    let diagnostics = serde_json::to_string(&parsed.diagnostics)
                        .map_err(|error| error.to_string())?;
                    return Err(format!("PRD contains diagnostics: {diagnostics}"));
                }
                let document = parsed.document.ok_or_else(|| "PRD is empty".to_string())?;
                let tasks = document
                    .tasks
                    .iter()
                    .enumerate()
                    .map(|(index, task)| ImportedTask {
                        id: format!("{project_id}:{import_id}:{index}"),
                        title: task.title.clone(),
                        description: task.description.clone(),
                        source_ref: task.source_ref.clone(),
                        acceptance_criteria: task.acceptance_criteria.join("\n"),
                    })
                    .collect::<Vec<_>>();
                let imported = journal
                    .import_prd(
                        &import_id,
                        &project_id,
                        &origin,
                        &version,
                        &source_text,
                        &tasks,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let result = serde_json::to_vec(&serde_json::json!({
                    "import_id": import_id,
                    "project_id": project_id,
                    "task_ids": imported.into_iter().map(|task| task.id).collect::<Vec<_>>(),
                }))
                .map_err(|error| error.to_string())?;
                journal
                    .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(result)
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetTaskHistory {
            task_id,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let events = journal
                        .task_history(&task_id, limit.min(100))
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "events": events.into_iter().map(|event| serde_json::json!({
                            "sequence_id": event.sequence_id,
                            "event_type": event.event_type,
                            "created_at": event.created_at,
                            "payload": match serde_json::from_slice::<serde_json::Value>(&event.payload) {
                                Ok(value) => value,
                                Err(error) => {
                                    tracing::debug!(%error, "event payload is not JSON; exposing raw bytes");
                                    serde_json::json!({"raw_bytes": event.payload})
                                }
                            },
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetTaskContext {
            project_id,
            task_id,
            max_chars,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let project = journal
                    .get_project(&project_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "project not found".to_string())?;
                let task = journal
                    .get_work_item(&task_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "task not found".to_string())?;
                if task.project_id != project_id {
                    return Err("task does not belong to project".to_string());
                }
                let manifest =
                    crate::workspace::build_manifest(&project.workspace_path, 500, 2 * 1024 * 1024)
                        .map_err(|error| error.to_string())?;
                let references = manifest
                    .entries
                    .iter()
                    .map(|entry| entry.relative_path.clone())
                    .collect::<Vec<_>>();
                let context = crate::workspace::assemble_context(
                    crate::workspace::ContextInput {
                        title: &task.title,
                        description: &task.description,
                        acceptance_criteria: &task.acceptance_criteria,
                        non_goals: &task.non_goals,
                        references: &references,
                        skill_context: &[],
                    },
                    max_chars.min(32 * 1024),
                );
                serde_json::to_vec(&serde_json::json!({
                    "project_id": project_id,
                    "task_id": task_id,
                    "workspace_hash": manifest.workspace_hash,
                    "manifest": manifest,
                    "context": context,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetTaskPlanSpec {
            project_id,
            task_id,
            max_chars,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let task = journal
                    .get_work_item(&task_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "task not found".to_string())?;
                if task.project_id != project_id {
                    return Err("task does not belong to project".to_string());
                }
                let plan = crate::plan::build_task_plan_spec(
                    &task.title,
                    &task.description,
                    &task.acceptance_criteria,
                    &task.non_goals,
                    "offline context; research не выполняется",
                    max_chars.min(32 * 1024),
                );
                serde_json::to_vec(&plan).map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::PlanArtifact {
            operation,
            artifact_json,
            artifact_id,
            expected_version,
            status,
            policy_snapshot_hash,
            task_id,
            workflow_run_id,
            correlation_id,
            idempotency_key,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let runtime = crate::plan_artifact::PlanArtifactRuntime::new(journal);
                let now = crate::task_memory::now_millis() as i64;
                match operation.as_str() {
                    "create" => {
                        let artifact: crate::plan_artifact::PlanArtifactV1 =
                            serde_json::from_slice(&artifact_json).map_err(|e| e.to_string())?;
                        serde_json::to_vec(
                            &runtime
                                .create(&artifact, &idempotency_key, now)
                                .await
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())
                    }
                    "read" => serde_json::to_vec(
                        &runtime.get(&artifact_id).await.map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string()),
                    "execute" => serde_json::to_vec(
                        &runtime
                            .execute(crate::plan_artifact::ExecutePlanArtifact {
                                artifact_id: &artifact_id,
                                expected_version,
                                policy_snapshot_hash: &policy_snapshot_hash,
                                task_id: task_id.as_deref(),
                                workflow_run_id: workflow_run_id.as_deref(),
                                correlation_id: &correlation_id,
                                idempotency_key: &idempotency_key,
                                now_ms: now,
                            })
                            .await
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string()),
                    "transition" => {
                        let next = crate::plan_artifact::PlanArtifactStatus::parse(&status)
                            .ok_or_else(|| "invalid plan artifact status".to_string())?;
                        serde_json::to_vec(
                            &runtime
                                .transition(
                                    &artifact_id,
                                    expected_version,
                                    next,
                                    &idempotency_key,
                                    now,
                                )
                                .await
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())
                    }
                    _ => Err("invalid plan artifact operation".into()),
                }
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
