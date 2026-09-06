use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::GetTaskSnapshot {
            project_id,
            task_id,
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
                let snapshot = journal
                    .latest_snapshot_for_task(&task_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "snapshot not found".to_string())?;
                let snapshot_json = serde_json::from_slice::<serde_json::Value>(&snapshot.payload)
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "id": snapshot.id,
                    "run_id": snapshot.run_id,
                    "workspace_hash": snapshot.workspace_hash,
                    "created_at": snapshot.created_at,
                    "snapshot": snapshot_json,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RestoreTaskSnapshot {
            project_id,
            task_id,
            snapshot_id,
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
                let snapshot = journal
                    .get_snapshot(&snapshot_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "snapshot not found".to_string())?;
                let run = journal
                    .get_run(&snapshot.run_id)
                    .await
                    .map_err(|error| error.to_string())?;
                if run.as_ref().map(|run| run.work_item_id.as_str()) != Some(task_id.as_str()) {
                    return Err("snapshot ownership could not be verified".to_string());
                }
                let run_id = snapshot.run_id.clone();
                let workspace_snapshot =
                    serde_json::from_slice::<crate::build::WorkspaceSnapshot>(&snapshot.payload)
                        .map_err(|error| format!("invalid snapshot: {error}"))?;
                crate::workspace_state_checkpoints::restore_build_snapshot_safe(
                    &project.workspace_path,
                    &workspace_snapshot,
                )
                .map_err(|error| error.to_string())?;
                let audit_payload = serde_json::to_vec(&serde_json::json!({
                    "task_id": task_id,
                    "snapshot_id": snapshot_id,
                    "run_id": run_id,
                    "operation": "workspace_restore",
                }))
                .map_err(|error| error.to_string())?;
                journal
                    .record_audit(&task_id, "snapshot.rollback.applied", &audit_payload)
                    .await
                    .map_err(|error| error.to_string())?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    task_id.clone(),
                    "snapshot.rollback.applied",
                    [
                        ("snapshot_id".to_owned(), snapshot_id.clone()),
                        ("run_id".to_owned(), run_id.clone()),
                        ("operation".to_owned(), "workspace_restore".to_owned()),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({
                    "snapshot_id": snapshot_id,
                    "restored": true,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetBuildPolicy { project_id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|error| error.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    let (policy, version) = journal.get_build_policy(&project.id, &default_build_policy()).await?;
                    serde_json::to_vec(&serde_json::json!({ "project_id": project_id, "version": version, "policy": policy })).map_err(|error| error.to_string())
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::SaveBuildPolicy {
            project_id,
            policy_json,
            expected_version,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    journal.get_project(&project_id).await.map_err(|error| error.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    let policy = harden_build_policy(serde_json::from_slice::<crate::scope::BuildScope>(&policy_json).map_err(|error| format!("invalid build policy: {error}"))?);
                    if let Some(violation) = crate::scope::validate_build_scope(&policy, &[]).first() { return Err(format!("invalid build policy: {}", violation.reason)); }
                    let saved = journal.save_build_policy(&project_id, &policy, Some(expected_version)).await?;
                    serde_json::to_vec(&serde_json::json!({ "project_id": project_id, "version": saved.version, "policy": policy })).map_err(|error| error.to_string())
                }.await;
            let _ = reply.send(result);
        }
        CoreCommand::ApplyApprovedBuild {
            project_id,
            run_id,
            task_id,
            approved_build_json,
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
                    let approved =
                        serde_json::from_slice::<crate::build::ApprovedBuild>(&approved_build_json)
                            .map_err(|error| format!("invalid approved build: {error}"))?;
                    let _effect = journal
                        .begin_build_effect(&run_id, &task_id, &approved.intent_hash)
                        .await
                        .map_err(|error| error.to_string())?;
                    let heartbeat_failure = Arc::new(StdMutex::new(None::<String>));
                    let heartbeat_cancel = CancellationToken::new();
                    let heartbeat_journal = journal.clone();
                    let heartbeat_run_id = run_id.clone();
                    let heartbeat_failure_slot = heartbeat_failure.clone();
                    let heartbeat_cancel_for_task = heartbeat_cancel.clone();
                    let heartbeat_task = tokio::spawn(async move {
                        let mut interval = tokio::time::interval(Duration::from_secs(10));
                        loop {
                            tokio::select! {
                                _ = heartbeat_cancel_for_task.cancelled() => break,
                                _ = interval.tick() => {
                                    if let Err(error) = heartbeat_journal.heartbeat_build_effect(&heartbeat_run_id).await {
                                        *heartbeat_failure_slot.lock().expect("heartbeat failure lock") = Some(error.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    });
                    let apply_result = tokio::task::spawn_blocking({
                        let workspace_path = project.workspace_path.clone();
                        let run_id = run_id.clone();
                        let approved = approved.clone();
                        move || crate::build::apply_approved_build(&workspace_path, &run_id, &approved)
                    })
                    .await;
                    heartbeat_cancel.cancel();
                    let _ = heartbeat_task.await;
                    let apply_result =
                        apply_result.map_err(|error| format!("build worker failed: {error}"))?;
                    let snapshot = match apply_result {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            let _ = journal.complete_build_effect(&run_id, false, None).await;
                            TaskCoordinator::record_audit(
                                &state,
                                crate::audit::AuditKind::Failure,
                                if task_id.is_empty() {
                                    run_id.clone()
                                } else {
                                    task_id.clone()
                                },
                                "build.apply_failed",
                                [
                                    ("run_id".to_owned(), run_id.clone()),
                                    ("task_id".to_owned(), task_id.clone()),
                                    ("intent_hash".to_owned(), approved.intent_hash.clone()),
                                    ("error".to_owned(), error.to_string()),
                                ],
                            )
                            .await;
                            return Err(error.to_string());
                        }
                    };
                    let payload =
                        serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
                    journal
                        .save_snapshot(
                            &snapshot.id,
                            &run_id,
                            &snapshot.baseline_workspace_hash,
                            &payload,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    if let Some(error) = heartbeat_failure
                        .lock()
                        .expect("heartbeat failure lock")
                        .clone()
                    {
                        return Err(format!(
                            "build lease heartbeat failed; outcome requires reconciliation: {error}"
                        ));
                    }
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "run_id": run_id,
                        "snapshot_id": snapshot.id,
                        "intent_hash": approved.intent_hash,
                        "effective_permissions_hash": approved.effective_permissions_hash,
                        "workspace_hash": snapshot.baseline_workspace_hash,
                        "diff_count": snapshot.diff.len(),
                        "diff": &snapshot.diff,
                    }))
                    .map_err(|error| error.to_string())?;
                    let audit_subject = if task_id.is_empty() {
                        &run_id
                    } else {
                        &task_id
                    };
                    journal
                        .record_audit(audit_subject, "build.applied", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    journal
                        .complete_build_effect(&run_id, true, Some(&snapshot.id))
                        .await
                        .map_err(|error| error.to_string())?;
                    TaskCoordinator::record_audit(
                        &state,
                        crate::audit::AuditKind::Diff,
                        audit_subject.to_string(),
                        "build.applied",
                        [
                            ("run_id".to_owned(), run_id.clone()),
                            ("task_id".to_owned(), task_id.clone()),
                            ("snapshot_id".to_owned(), snapshot.id.clone()),
                            ("intent_hash".to_owned(), approved.intent_hash.clone()),
                            ("diff_count".to_owned(), snapshot.diff.len().to_string()),
                        ],
                    )
                    .await;
                    Ok(payload)
                }
                .await;
            let _ = reply.send(result);
        }
        CoreCommand::PrepareBuild {
            project_id,
            proposal_json,
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
                let proposal =
                    serde_json::from_slice::<crate::build::BuildProposal>(&proposal_json)
                        .map_err(|error| format!("invalid build proposal: {error}"))?;
                let policy = journal
                    .get_or_create_build_policy(&project_id, &default_build_policy())
                    .await?;
                let effective_scope = crate::scope::restrict_to_policy(&policy, &proposal.scope)
                    .map_err(|violations| match serde_json::to_string(&violations) {
                        Ok(value) => value,
                        Err(error) => {
                            tracing::warn!(%error, "failed to serialize build policy violations");
                            "build policy violation".into()
                        }
                    })?;
                let effective_proposal = crate::build::BuildProposal {
                    scope: effective_scope,
                    changes: proposal.changes,
                };
                let approved =
                    crate::build::prepare_build(&project.workspace_path, &effective_proposal)
                        .map_err(|error| error.to_string())?;
                let payload = serde_json::to_vec(&approved).map_err(|error| error.to_string())?;
                let audit_subject = format!("proposal-{}", approved.intent_hash);
                let audit_payload = serde_json::to_vec(&serde_json::json!({
                    "intent_hash": approved.intent_hash,
                    "effective_permissions_hash": approved.effective_permissions_hash,
                    "expected_workspace_hash": approved.expected_workspace_hash,
                    "change_count": approved.changes.len(),
                }))
                .map_err(|error| error.to_string())?;
                journal
                    .record_audit(&audit_subject, "build.approval_prepared", &audit_payload)
                    .await
                    .map_err(|error| error.to_string())?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Budget,
                    project_id.clone(),
                    "build.approval_prepared",
                    [
                        ("intent_hash".to_owned(), approved.intent_hash.clone()),
                        (
                            "change_count".to_owned(),
                            approved.changes.len().to_string(),
                        ),
                        (
                            "max_files_changed".to_owned(),
                            policy.max_files_changed.to_string(),
                        ),
                        (
                            "max_bytes_changed".to_owned(),
                            policy.max_bytes_changed.to_string(),
                        ),
                    ],
                )
                .await;
                Ok(payload)
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RunDoctor {
            project_id,
            protocol_major,
            expected_protocol_major,
            provider,
            approval_required,
            registered_tools,
            expected_tools,
            unavailable_tools,
            detail_level,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let storage = match &journal {
                    Some(journal) => {
                        let (path, schema_version) = journal
                            .storage_snapshot()
                            .await
                            .map_err(|error| error.to_string())?;
                        let exists = path.exists();
                        let writable = exists
                            && std::fs::metadata(&path)
                                .map(|meta| !meta.permissions().readonly())
                                .unwrap_or(false);
                        crate::doctor::StorageProbe {
                            path_label: path.display().to_string(),
                            exists,
                            writable,
                            schema_version: Some(schema_version),
                            expected_schema_version: evohime_local_storage::SCHEMA_VERSION,
                        }
                    }
                    None => crate::doctor::StorageProbe {
                        path_label: "not-configured".into(),
                        exists: false,
                        writable: false,
                        schema_version: None,
                        expected_schema_version: evohime_local_storage::SCHEMA_VERSION,
                    },
                };

                let pipe = crate::doctor::PipeProbe {
                    pipe_label: "desktop-ipc".into(),
                    reachable: true,
                    protocol_major,
                    expected_protocol_major,
                };

                let recovery = match &journal {
                    Some(journal) => journal
                        .recovery_probe()
                        .await
                        .map_err(|error| error.to_string())?,
                    None => crate::doctor::RecoveryProbe {
                        state: "NOT_CONFIGURED".into(),
                        unknown_effects: 0,
                        lease_expired: false,
                        resumable_runs: 0,
                    },
                };

                let permissions = match (&journal, project_id.is_empty()) {
                    (Some(journal), false) => {
                        match journal
                            .get_project(&project_id)
                            .await
                            .map_err(|error| error.to_string())?
                        {
                            Some(project) => {
                                let workspace = std::path::Path::new(&project.workspace_path);
                                let workspace_readable = workspace.is_dir();
                                let workspace_writable = workspace_readable
                                    && std::fs::metadata(workspace)
                                        .map(|meta| !meta.permissions().readonly())
                                        .unwrap_or(false);
                                let protected_paths_intact = [".git", ".evohime"]
                                    .iter()
                                    .all(|segment| workspace.join(segment).exists());
                                crate::doctor::PermissionsProbe {
                                    workspace_readable,
                                    workspace_writable,
                                    protected_paths_intact,
                                    approval_required,
                                }
                            }
                            None => unresolved_permissions_probe(approval_required),
                        }
                    }
                    _ => unresolved_permissions_probe(approval_required),
                };

                let scheduler = crate::export::scheduler_probe();

                let snapshot = crate::doctor::DoctorSnapshot {
                    storage,
                    pipe,
                    provider,
                    recovery,
                    permissions,
                    tools: crate::doctor::ToolsProbe {
                        registered_tools,
                        expected_tools,
                        unavailable_tools,
                    },
                    scheduler,
                };
                let report =
                    crate::doctor::DoctorReport::from_snapshot_with_detail(&snapshot, detail_level)
                        .map_err(|error| format!("{error:?}"))?;
                Ok(report.to_bounded_json().into_bytes())
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
