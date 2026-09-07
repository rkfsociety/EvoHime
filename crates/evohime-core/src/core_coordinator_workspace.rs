use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::IndexWorkspace {
            workspace_path,
            enable_embeddings,
            reply,
        } => {
            let key = workspace_path.replace('\\', "/").to_lowercase();
            let cancellation = CancellationToken::new();
            let (journal, events) = {
                let mut guard = state.lock().await;
                if guard.workspace_index_cancellations.contains_key(&key) {
                    let _ = reply.send(Err("workspace index run is already active".into()));
                    return;
                }
                guard
                    .workspace_index_cancellations
                    .insert(key.clone(), cancellation.clone());
                (guard.journal.clone(), guard.events.clone())
            };
            let state_after = Arc::clone(&state);
            let Some(background_permit) = state.lock().await.background_tasks.try_acquire() else {
                state
                    .lock()
                    .await
                    .workspace_index_cancellations
                    .remove(&key);
                let _ = reply.send(Err("background task capacity is exhausted".into()));
                return;
            };
            tokio::spawn(async move {
                let _background_permit = background_permit;
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let root = std::path::PathBuf::from(&workspace_path);
                    let progress_path = workspace_path.clone();
                    let summary = journal
                        .index_workspace_knowledge(&root, false, &cancellation, move |progress| {
                            let _ = events.blocking_send(CoreEvent::WorkspaceIndexProgress {
                                workspace_path: progress_path.clone(),
                                progress,
                            });
                        })
                        .await
                        .map_err(|error| error.to_string())?;
                    let vector_index_id = if enable_embeddings {
                        journal
                            .build_workspace_vector_index(&root, &cancellation)
                            .await
                            .map_err(|error| error.to_string())?
                    } else {
                        None
                    };
                    serde_json::to_vec(&serde_json::json!({
                        "summary": summary,
                        "vector_index_id": vector_index_id,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                state_after
                    .lock()
                    .await
                    .workspace_index_cancellations
                    .remove(&key);
                let _ = reply.send(result);
            });
        }
        CoreCommand::RebuildIndex {
            workspace_path,
            enable_embeddings,
            reply,
        } => {
            let key = workspace_path.replace('\\', "/").to_lowercase();
            let cancellation = CancellationToken::new();
            let (journal, events) = {
                let mut guard = state.lock().await;
                if guard.workspace_index_cancellations.contains_key(&key) {
                    let _ = reply.send(Err("workspace index run is already active".into()));
                    return;
                }
                guard
                    .workspace_index_cancellations
                    .insert(key.clone(), cancellation.clone());
                (guard.journal.clone(), guard.events.clone())
            };
            let state_after = Arc::clone(&state);
            let Some(background_permit) = state.lock().await.background_tasks.try_acquire() else {
                state
                    .lock()
                    .await
                    .workspace_index_cancellations
                    .remove(&key);
                let _ = reply.send(Err("background task capacity is exhausted".into()));
                return;
            };
            tokio::spawn(async move {
                let _background_permit = background_permit;
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let root = std::path::PathBuf::from(&workspace_path);
                    let progress_path = workspace_path.clone();
                    let summary = journal
                        .index_workspace_knowledge(&root, true, &cancellation, move |progress| {
                            let _ = events.blocking_send(CoreEvent::WorkspaceIndexProgress {
                                workspace_path: progress_path.clone(),
                                progress,
                            });
                        })
                        .await
                        .map_err(|error| error.to_string())?;
                    let vector_index_id = if enable_embeddings {
                        journal
                            .build_workspace_vector_index(&root, &cancellation)
                            .await
                            .map_err(|error| error.to_string())?
                    } else {
                        None
                    };
                    serde_json::to_vec(&serde_json::json!({
                        "summary": summary,
                        "vector_index_id": vector_index_id,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                state_after
                    .lock()
                    .await
                    .workspace_index_cancellations
                    .remove(&key);
                let _ = reply.send(result);
            });
        }
        CoreCommand::CancelWorkspaceIndex {
            workspace_path,
            reply,
        } => {
            let key = workspace_path.replace('\\', "/").to_lowercase();
            let cancelled = state
                .lock()
                .await
                .workspace_index_cancellations
                .get(&key)
                .map(|token| {
                    token.cancel();
                    true
                })
                .unwrap_or(false);
            let _ = reply.send(
                serde_json::to_vec(&serde_json::json!({ "cancelled": cancelled }))
                    .map_err(|error| error.to_string()),
            );
        }
        CoreCommand::SearchWorkspaceKnowledge {
            workspace_path,
            query,
            path_filter,
            language_filter,
            hybrid,
            reply,
        } => {
            let (journal, event_sender) = {
                let state = state.lock().await;
                (state.journal.clone(), state.events.clone())
            };
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let root = std::path::PathBuf::from(&workspace_path);
                let progress_sender = event_sender.clone();
                let progress_workspace = workspace_path.clone();
                let search = journal
                    .search_workspace_knowledge_with_progress(
                        &root,
                        &query,
                        crate::workspace_rag::QueryFilters {
                            path: path_filter,
                            language: language_filter,
                        },
                        hybrid,
                        move |progress| {
                            let _ = progress_sender.blocking_send(
                                CoreEvent::WorkspaceRetrievalProgress {
                                    workspace_path: progress_workspace.clone(),
                                    progress,
                                },
                            );
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let context = journal
                    .build_workspace_evidence_context(&root, &search)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "search": search,
                    "context": context,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetIndexStatus {
            workspace_path,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let status = journal
                    .workspace_index_status(std::path::Path::new(&workspace_path))
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({ "status": status }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::SubmitFeedback {
            run_id,
            task_id,
            subject_ref,
            signal,
            correction,
            rejection_reason,
            outcome,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let signal_parsed = match signal.as_str() {
                    "useful" => evohime_local_storage::feedback_store::FeedbackSignal::Useful,
                    "not_useful" => {
                        evohime_local_storage::feedback_store::FeedbackSignal::NotUseful
                    }
                    "neutral" => evohime_local_storage::feedback_store::FeedbackSignal::Neutral,
                    other => return Err(format!("unknown feedback signal: {other}")),
                };
                let created_at_ms = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let id = uuid::Uuid::new_v4().to_string();
                let record = evohime_local_storage::feedback_store::FeedbackRecord::new(
                    evohime_local_storage::feedback_store::FeedbackRecordInput {
                        id: id.clone(),
                        run_id: run_id.clone(),
                        task_id,
                        subject_ref,
                        signal: signal_parsed,
                        correction,
                        rejection_reason,
                        outcome,
                        provenance: "user:feedback".to_owned(),
                        created_at: created_at_ms.to_string(),
                    },
                )
                .map_err(|error| error.to_string())?;
                journal.save_feedback(&record).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    run_id.clone(),
                    "feedback.submitted",
                    [
                        ("feedback_id".to_owned(), record.id.clone()),
                        ("signal".to_owned(), signal),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "record": record }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListFeedback {
            run_id,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let records = journal.list_feedback(&run_id, limit).await?;
                let aggregate = journal.aggregate_feedback(20, 20).await?;
                serde_json::to_vec(&serde_json::json!({
                    "records": records,
                    "aggregate": aggregate,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetContextLedger {
            task_id,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let projections = journal
                    .context_ledger_projection(&task_id, bounded_limit(limit))
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({ "entries": projections }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListTaskScratchpad {
            task_id,
            category,
            status,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let entries = journal
                    .scratchpad_projection(
                        &task_id,
                        category.as_deref(),
                        status.as_deref(),
                        bounded_limit(limit),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({ "entries": entries }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
