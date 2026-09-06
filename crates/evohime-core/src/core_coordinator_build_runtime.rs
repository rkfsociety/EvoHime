use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::CreateDiagnosticsSnapshot {
            project_id,
            conversation_id,
            run_id,
            max_event_count,
            max_log_bytes,
            protocol_major,
            expected_protocol_major,
            provider,
            approval_required,
            registered_tools,
            expected_tools,
            unavailable_tools,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                    let started = std::time::Instant::now();
                    let (schema_version, recovery_state) = match &journal {
                        Some(journal) => {
                            let (_, version) = journal.storage_snapshot().await.map_err(|e| e.to_string())?;
                            let recovery = journal.recovery_probe().await.map_err(|e| e.to_string())?;
                            (version, recovery.state)
                        }
                        None => (0, "NOT_CONFIGURED".to_owned()),
                    };
                    let doctor = serde_json::json!({
                        "contract_version": 1,
                        "checks": [
                            {"id":"storage", "status": if schema_version > 0 { "OK" } else { "BLOCKED" }, "summary":"Хранилище и схема доступны", "action":"Действий не требуется"},
                            {"id":"pipe", "status": if protocol_major == Some(expected_protocol_major) { "OK" } else { "BLOCKED" }, "summary":"Core pipe доступен", "action":"Проверь версии UI и Core"},
                            {"id":"provider", "status": if provider.configured && provider.metadata_valid { "OK" } else { "WARN" }, "summary":"Состояние провайдера проверено", "action":"Проверь настройки провайдера"},
                            {"id":"recovery", "status": if recovery_state == "CLEAN" { "OK" } else { "WARN" }, "summary":"Состояние recovery: {recovery_state}", "action":"Проверь recovery state"},
                            {"id":"permissions", "status": if approval_required { "WARN" } else { "OK" }, "summary":"Политика разрешений проверена", "action":"Подтверди требуемое разрешение явно"},
                            {"id":"tools", "status": if registered_tools >= expected_tools && unavailable_tools.is_empty() { "OK" } else { "WARN" }, "summary":"Каталог tools проверен", "action":"Проверь регистрацию tools"}
                        ]
                    });
                    let doctor_json = serde_json::to_vec(&doctor).map_err(|e| e.to_string())?;
                    let run_status = if run_id.is_empty() {
                        String::new()
                    } else {
                        let run = journal
                            .as_ref()
                            .ok_or_else(|| "run_not_found".to_owned())?
                            .get_run(&run_id)
                            .await
                            .map_err(|e| e.to_string())?
                            .ok_or_else(|| "run_not_found".to_owned())?;
                        run.status
                    };
                    crate::support_bundle::build_snapshot(&doctor_json, conversation_id, run_id, run_status, max_event_count, max_log_bytes, started.elapsed().as_millis() as u64)
                }.await;
            let _ = reply.send(result);
            let _ = project_id;
        }
        CoreCommand::ExportDoctorLogs {
            destination_path,
            reply,
        } => {
            let result = crate::export::export_logs(std::path::Path::new(&destination_path))
                .map(|summary| summary.to_bounded_json().into_bytes())
                .map_err(|error| format!("{error:?}"));
            let _ = reply.send(result);
        }
        CoreCommand::CreateDatabaseBackup {
            operation_id,
            destination_path,
            progress,
            reply,
        } => {
            let cancellation = CancellationToken::new();
            let (journal, events) = {
                let guard = state.lock().await;
                (guard.journal.clone(), guard.events.clone())
            };
            state
                .lock()
                .await
                .backup_cancellations
                .insert(operation_id.clone(), cancellation.clone());
            let background_permit = state.lock().await.background_tasks.try_acquire();
            let Some(background_permit) = background_permit else {
                state
                    .lock()
                    .await
                    .backup_cancellations
                    .remove(&operation_id);
                let _ = reply.send(Err("background task capacity is exhausted".into()));
                return;
            };
            tokio::spawn(async move {
                let _background_permit = background_permit;
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let start_payload = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": "started",
                        "destination_name": safe_file_name(&destination_path),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.started", &start_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    let operation_for_events = operation_id.clone();
                    let progress = progress;
                    let operation_cancellation = cancellation.clone();
                    let result = journal
                        .create_database_backup_with_cancel(
                            std::path::Path::new(&destination_path),
                            env!("CARGO_PKG_VERSION"),
                            |item| {
                                let _ = progress.send(item.clone());
                                let _ = events.send(CoreEvent::StorageProgress {
                                    operation_id: operation_for_events.clone(),
                                    progress: item,
                                });
                            },
                            move || operation_cancellation.is_cancelled(),
                        )
                        .await
                        .map_err(|error| error.to_string());
                    let audit = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": if result.is_ok() { "created" } else if result.as_ref().err().is_some_and(|error| error.to_string().contains("cancelled")) { "cancelled" } else { "failed" },
                        "destination_name": safe_file_name(&destination_path),
                        "error_category": result.as_ref().err().map(|error| error_category(error)),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.completed", &audit)
                        .await
                        .map_err(|error| error.to_string())?;
                    result.and_then(|value| {
                        serde_json::to_vec(&value).map_err(|error| error.to_string())
                    })
                    }
                    .await;
                state
                    .lock()
                    .await
                    .backup_cancellations
                    .remove(&operation_id);
                let _ = reply.send(result);
            });
        }
        CoreCommand::PrepareDatabaseRestore {
            operation_id,
            backup_path,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let preview = LocalDatabase::preview_backup(&backup_path)
                    .map_err(|error| error.to_string())?;
                let approval_id = uuid::Uuid::new_v4().to_string();
                state
                    .lock()
                    .await
                    .backup_approvals
                    .insert(approval_id.clone(), backup_path.clone());
                if let Some(journal) = journal {
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": "previewed",
                        "backup_name": safe_file_name(&backup_path),
                        "schema_version": preview.schema_version,
                        "checksum_sha256": preview.checksum_sha256,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.previewed", &payload)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                serde_json::to_vec(&serde_json::json!({
                    "operation_id": operation_id,
                    "approval_id": approval_id,
                    "preview": preview,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RestoreDatabase {
            operation_id,
            backup_path,
            approval_id,
            progress,
            reply,
        } => {
            let cancellation = CancellationToken::new();
            let approved = {
                let mut guard = state.lock().await;
                guard
                    .backup_approvals
                    .get(&approval_id)
                    .is_some_and(|path| path == &backup_path)
                    .then(|| guard.backup_approvals.remove(&approval_id))
                    .flatten()
                    .is_some()
            };
            let (journal, events) = {
                let guard = state.lock().await;
                (guard.journal.clone(), guard.events.clone())
            };
            if approved {
                state
                    .lock()
                    .await
                    .backup_cancellations
                    .insert(operation_id.clone(), cancellation.clone());
            }
            let background_permit = state.lock().await.background_tasks.try_acquire();
            let Some(background_permit) = background_permit else {
                if approved {
                    state
                        .lock()
                        .await
                        .backup_cancellations
                        .remove(&operation_id);
                }
                let _ = reply.send(Err("background task capacity is exhausted".into()));
                return;
            };
            tokio::spawn(async move {
                let _background_permit = background_permit;
                let result = async {
                    if !approved {
                        if let Some(journal) = &journal {
                            let payload = serde_json::to_vec(&serde_json::json!({
                                "operation_id": operation_id,
                                "result": "rejected",
                                "backup_name": safe_file_name(&backup_path),
                                "error_category": "approval",
                            }))
                            .map_err(|error| error.to_string())?;
                            journal
                                .record_audit(&operation_id, "storage.restore.rejected", &payload)
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                        return Err("restore approval is missing or does not match the preview".into());
                    }
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let (database_path, _) = journal
                        .storage_snapshot()
                        .await
                        .map_err(|error| error.to_string())?;
                    let safety_path = database_path.with_file_name(format!(
                        "{}.pre-restore-{}.evohime",
                        safe_file_stem(&database_path),
                        uuid::Uuid::new_v4()
                    ));
                    let operation_for_events = operation_id.clone();
                    let progress = progress;
                    let operation_cancellation = cancellation.clone();
                    let restore = journal
                        .restore_database_with_cancel(
                            std::path::Path::new(&backup_path),
                            &safety_path,
                            env!("CARGO_PKG_VERSION"),
                            |item| {
                                let _ = progress.send(item.clone());
                                let _ = events.send(CoreEvent::StorageProgress {
                                    operation_id: operation_for_events.clone(),
                                    progress: item,
                                });
                            },
                            move || operation_cancellation.is_cancelled(),
                        )
                        .await;
                    let audit = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": if restore.is_ok() { "restored" } else if restore.as_ref().err().is_some_and(|error| error.to_string().contains("cancelled")) { "cancelled" } else { "failed" },
                        "backup_name": safe_file_name(&backup_path),
                        "error_category": restore.as_ref().err().map(|error| error_category(&error.to_string())),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.restore.completed", &audit)
                        .await
                        .map_err(|error| error.to_string())?;
                    restore
                        .map(|value| serde_json::to_vec(&value).map_err(|error| error.to_string()))
                        .map_err(|error| error.to_string())?
                    }
                    .await;
                state
                    .lock()
                    .await
                    .backup_cancellations
                    .remove(&operation_id);
                let _ = reply.send(result);
            });
        }
        CoreCommand::CancelDatabaseOperation {
            operation_id,
            reply,
        } => {
            let accepted = state
                .lock()
                .await
                .backup_cancellations
                .get(&operation_id)
                .map(CancellationToken::cancel)
                .is_some();
            let result = serde_json::to_vec(&serde_json::json!({
                "operation_id": operation_id,
                "accepted": accepted,
            }))
            .map_err(|error| error.to_string());
            let _ = reply.send(result);
        }
        CoreCommand::SaveResearchEvidence {
            work_item_id,
            source_kind,
            source_ref,
            title,
            publisher,
            content_type,
            raw_excerpt,
            retrieved_at_ms,
            ttl_ms,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if work_item_id.trim().is_empty() {
                    return Err("work_item_id must not be empty".to_string());
                }
                let source = crate::research::SourceMetadata::new(
                    source_ref,
                    title,
                    publisher,
                    content_type,
                    retrieved_at_ms,
                )
                .map_err(|error| error.to_string())?;
                let captured_at_ms = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let evidence = crate::research::ResearchEvidence::capture(
                    source,
                    raw_excerpt,
                    captured_at_ms,
                    ttl_ms,
                )
                .map_err(|error| error.to_string())?;
                let id = uuid::Uuid::new_v4().to_string();
                let record = evohime_local_storage::research_store::ResearchEvidenceRecord {
                    id: id.clone(),
                    source_kind: source_kind.clone(),
                    source_ref: evidence.source.url.clone(),
                    redacted_excerpt: evidence.excerpt.clone(),
                    source_hash: evidence.excerpt_sha256.clone(),
                    fetched_at: evidence.captured_at_ms.to_string(),
                    ttl_seconds: evidence.ttl_ms.div_ceil(1_000),
                    provenance_link: Some(work_item_id.clone()),
                };
                journal.save_research_evidence(&record).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    work_item_id.clone(),
                    "research.evidence.saved",
                    [
                        ("evidence_id".to_owned(), id.clone()),
                        ("source_kind".to_owned(), source_kind),
                        ("source_hash".to_owned(), evidence.excerpt_sha256.clone()),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({
                    "id": id,
                    "work_item_id": work_item_id,
                    "evidence": evidence,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListResearchEvidence {
            work_item_id,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let records = journal.list_research_evidence(&work_item_id).await?;
                serde_json::to_vec(&serde_json::json!({
                    "work_item_id": work_item_id,
                    "records": records,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RunResearchFetch {
            work_item_id,
            url,
            title,
            allowed_domains,
            max_bytes,
            max_latency_ms,
            max_cost_micros,
            ttl_ms,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if work_item_id.trim().is_empty() {
                    return Err("work_item_id must not be empty".to_string());
                }
                let policy = crate::research_pipeline::ResearchPolicy {
                    network_allowed: true,
                    allowed_domains,
                    max_bytes,
                    max_latency_ms,
                    max_cost_micros,
                };
                let fetch_result = crate::research_fetch::run_research_fetch(
                    &work_item_id,
                    &url,
                    &title,
                    &policy,
                    ttl_ms,
                    false,
                )
                .await;
                match fetch_result {
                    Ok(outcome) => {
                        let id = uuid::Uuid::new_v4().to_string();
                        let record =
                            evohime_local_storage::research_store::ResearchEvidenceRecord {
                                id: id.clone(),
                                source_kind: "url".to_string(),
                                source_ref: outcome.evidence.source.url.clone(),
                                redacted_excerpt: outcome.evidence.excerpt.clone(),
                                source_hash: outcome.evidence.excerpt_sha256.clone(),
                                fetched_at: outcome.evidence.captured_at_ms.to_string(),
                                ttl_seconds: outcome.evidence.ttl_ms.div_ceil(1_000),
                                provenance_link: Some(work_item_id.clone()),
                            };
                        journal.save_research_evidence(&record).await?;
                        TaskCoordinator::record_audit(
                            &state,
                            crate::audit::AuditKind::Evidence,
                            work_item_id.clone(),
                            "research.fetch.completed",
                            [
                                ("evidence_id".to_owned(), id.clone()),
                                ("url".to_owned(), outcome.citation.url.clone()),
                                (
                                    "source_hash".to_owned(),
                                    outcome.citation.source_hash.clone(),
                                ),
                            ],
                        )
                        .await;
                        serde_json::to_vec(&serde_json::json!({
                            "id": id,
                            "work_item_id": work_item_id,
                            "state": outcome.state,
                            "evidence": outcome.evidence,
                            "citation": outcome.citation,
                        }))
                        .map_err(|error| error.to_string())
                    }
                    Err(error) => {
                        TaskCoordinator::record_audit(
                            &state,
                            crate::audit::AuditKind::Failure,
                            work_item_id.clone(),
                            "research.fetch.failed",
                            [
                                ("url".to_owned(), url.clone()),
                                ("state".to_owned(), format!("{:?}", error.state)),
                                ("error".to_owned(), error.message.clone()),
                            ],
                        )
                        .await;
                        Err(error.message)
                    }
                }
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
