#[derive(Clone)]
pub struct EventJournal {
    pub(crate) database: Arc<Mutex<LocalDatabase>>,
    pub(crate) database_path: Arc<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableReplayBatch {
    pub events: Vec<EventRecord>,
    pub gap_detected: bool,
    pub first_available_sequence: Option<i64>,
    pub last_sequence: i64,
}

pub(crate) fn default_build_policy() -> crate::scope::BuildScope {
    crate::scope::BuildScope {
        allowed_paths: Vec::new(),
        allowed_operations: vec!["write".into(), "create".into()],
        expected_outputs: Vec::new(),
        protected_paths: vec![".git".into(), ".evohime".into()],
        allowed_file_types: Vec::new(),
        max_files_changed: 20,
        max_bytes_changed: 2 * 1024 * 1024,
        allow_create: true,
        allow_delete: false,
        allow_rename: false,
        baseline_snapshot_id: None,
        acceptance_criteria: String::new(),
        risk_class: "medium".into(),
        timeout_ms: 30_000,
    }
}

pub(crate) fn harden_build_policy(mut policy: crate::scope::BuildScope) -> crate::scope::BuildScope {
    for required in [".git", ".evohime"] {
        if !policy.protected_paths.iter().any(|path| path == required) {
            policy.protected_paths.push(required.into());
        }
    }
    policy
}

pub(crate) fn safe_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("database-backup")
        .chars()
        .take(128)
        .collect()
}

pub(crate) fn safe_file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("events")
        .chars()
        .filter(|value| value.is_ascii_alphanumeric() || *value == '-' || *value == '_')
        .take(64)
        .collect::<String>()
        .trim()
        .to_owned()
}

pub(crate) fn error_category(error: &str) -> &'static str {
    if error.contains("checksum") {
        "checksum"
    } else if error.contains("schema") {
        "schema"
    } else if error.contains("approval") {
        "approval"
    } else if error.contains("destination") {
        "destination"
    } else {
        "storage"
    }
}

/// Параметры одной метрики инструмента, записываемой в durable journal.
#[derive(Debug, Clone, Copy)]
pub struct ToolMetric<'a> {
    pub task_id: &'a str,
    pub tool_name: &'a str,
    pub iteration: usize,
    pub ok: bool,
    pub failure_kind: Option<&'a str>,
    pub recovery_hint: bool,
    pub escalated: bool,
}

/// Параметры перехода durable recovery state machine.
#[derive(Debug, Clone, Copy)]
pub struct RecoveryTransition<'a> {
    pub run_id: &'a str,
    pub state: RecoveryState,
    pub effect_id: &'a str,
    pub idempotency_key: &'a str,
    pub verifier: &'a str,
    pub evidence_json: &'a [u8],
    pub decision: &'a str,
}

/// Данные session-only заметки, которые никогда не становятся persistent memory.
#[derive(Debug, Clone, Copy)]
pub struct SessionMemoryNote<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub scope: evohime_local_storage::memory_store::MemoryScope,
    pub scope_id: &'a str,
    pub kind: &'a str,
    pub statement: &'a str,
    pub created_at: &'a str,
    pub expires_at: &'a str,
}

impl EventJournal {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        Ok(Self {
            database: Arc::new(Mutex::new(LocalDatabase::open(&path)?)),
            database_path: Arc::new(path),
        })
    }

    /// Startup gate for Core: reconcile active dispatchable requests before
    /// accepting a new model call, then run one bounded retention pass.
    pub async fn recover_model_provenance_on_startup(
        &self,
    ) -> Result<(usize, usize), StorageError> {
        let recovered = self.recover_model_requests().await?;
        let cutoff = task_memory::now_millis() as i64
            - evohime_model_provenance::PROVENANCE_RETENTION_DAYS * 24 * 60 * 60 * 1000;
        let pruned = self.retain_model_provenance(cutoff).await?;
        Ok((recovered, pruned))
    }

    /// Публикует один bounded `core_start` execution-ledger event для этого
    /// Core instance (план 08-2 п.5). Вызывается ровно один раз при старте,
    /// до `reconcile_ledger_on_startup`.
    pub async fn record_ledger_core_start(
        &self,
        core_instance_id: &str,
    ) -> Result<i64, StorageError> {
        let database = self.database.lock().await;
        database.record_core_start(core_instance_id)
    }

    /// Reconciliation незавершённых typed actions при старте Core (план
    /// 08-2 п.5): классифицирует по dispatch marker в `run_effects` и
    /// публикует read-only reconciliation-события, не переписывая исходные.
    pub async fn reconcile_ledger_on_startup(
        &self,
    ) -> Result<Vec<(String, evohime_local_storage::execution_ledger::ActionState)>, StorageError>
    {
        let database = self.database.lock().await;
        database.reconcile_ledger_on_startup()
    }

    /// Общий доступ к базе для контрактов плана 01: ledger, scratchpad и
    /// artifact store работают против той же мигрированной базы.
    pub fn database(&self) -> &Arc<Mutex<LocalDatabase>> {
        &self.database
    }

    pub fn database_path(&self) -> &std::path::Path {
        self.database_path.as_ref()
    }

    /// После перезапуска незавершённый continuation не возобновляется
    /// вслепую: Core переводит его в blocked до явного запуска пользователем.
    pub async fn recover_continuation_runs(&self) -> Result<usize, StorageError> {
        let database = self.database.lock().await;
        let runs =
            evohime_local_storage::continuation_store::list_running_runs(database.connection())?;
        let mut recovered = 0;
        for run in runs {
            if evohime_local_storage::continuation_store::transition_run(
                database.connection(),
                &run.run_id,
                "running",
                "blocked",
                Some("core_restart_requires_explicit_resume"),
                crate::task_memory::now_millis() as i64,
            )? {
                recovered += 1;
            }
        }
        Ok(recovered)
    }

    /// A dispatched retained-child message has an uncertain external outcome
    /// after restart. Mark it unknown and never retry it blindly.
    pub async fn recover_retained_children(&self) -> Result<(u32, u32), StorageError> {
        let database = self.database.lock().await;
        let unknown =
            evohime_local_storage::retained_child_store::RetainedChildStore::reconcile_all_unknown(
                database.connection(),
            )
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let expired = evohime_local_storage::retained_child_store::RetainedChildStore::expire_due(
            database.connection(),
            task_memory::now_millis(),
        )
        .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        Ok((unknown, expired))
    }

    /// A Core restart cannot rehydrate worker process memory. Running kernel
    /// manifests are fenced as crashed and require an explicit reset/start
    /// path instead of an automatic retry.
    pub async fn recover_analysis_kernels(&self) -> Result<usize, StorageError> {
        let database = self.database.lock().await;
        let store =
            evohime_local_storage::analysis_kernel::AnalysisKernelStore::new(database.connection());
        let sessions = store.list_running_sessions()?;
        let mut recovered = 0;
        let mut crashed_ids = Vec::new();
        for session in sessions {
            store.set_status(
                &session.id,
                session.revision,
                evohime_local_storage::analysis_kernel::KernelStatus::Crashed,
                task_memory::now_millis() as i64,
            )?;
            crashed_ids.push(session.id.clone());
            store.append_event(
                &session.id,
                "runtime.recovered",
                br#"{"disposition":"crashed_no_memory_rehydrate"}"#,
                task_memory::now_millis() as i64,
            )?;
            recovered += 1;
        }
        drop(database);
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
            for kernel_id in crashed_ids {
                let _ = crate::analysis_kernel::supervisor_command(serde_json::json!({
                    "op": "kernel_stop",
                    "kernel_id": kernel_id,
                }))
                .await;
            }
        }
        Ok(recovered)
    }

    /// Builds and atomically publishes one Core-owned workspace RAG
    /// generation. Progress is bounded by the scanner contract; callers may
    /// forward the returned final projection to UI without exposing paths
    /// outside the selected workspace.
    pub async fn index_workspace_knowledge(
        &self,
        workspace_root: &std::path::Path,
        rebuild: bool,
        cancellation: &CancellationToken,
        progress: impl FnMut(crate::workspace_rag::IndexProgress) + Send + 'static,
    ) -> Result<crate::workspace_rag::IndexSummary, crate::workspace_rag::RagError> {
        let database_path = self.database_path.as_ref().clone();
        let workspace_root = workspace_root.to_path_buf();
        let cancellation = cancellation.clone();
        tokio::task::spawn_blocking(move || {
            let mut database = LocalDatabase::open(database_path).map_err(|error| {
                crate::workspace_rag::RagError::InvalidConfig(error.to_string())
            })?;
            crate::workspace_rag::index_workspace(
                database.connection_mut(),
                &workspace_root,
                &crate::workspace_rag::IndexConfig::default(),
                rebuild,
                || cancellation.is_cancelled(),
                progress,
            )
        })
        .await
        .map_err(|error| crate::workspace_rag::RagError::InvalidConfig(error.to_string()))?
    }

    pub async fn workspace_index_status(
        &self,
        workspace_root: &std::path::Path,
    ) -> Result<crate::workspace_rag::IndexStatus, crate::workspace_rag::RagError> {
        let database = self.database.lock().await;
        crate::workspace_rag::get_index_status(database.connection(), workspace_root)
    }

    pub async fn search_workspace_knowledge(
        &self,
        workspace_root: &std::path::Path,
        query: &str,
        filters: crate::workspace_rag::QueryFilters,
        hybrid: bool,
    ) -> Result<crate::workspace_rag::SearchResult, crate::workspace_rag::RagError> {
        self.search_workspace_knowledge_with_progress(
            workspace_root,
            query,
            filters,
            hybrid,
            |_| {},
        )
        .await
    }

    pub async fn search_workspace_knowledge_with_progress(
        &self,
        workspace_root: &std::path::Path,
        query: &str,
        filters: crate::workspace_rag::QueryFilters,
        hybrid: bool,
        progress: impl FnMut(crate::workspace_rag::RetrievalProgress),
    ) -> Result<crate::workspace_rag::SearchResult, crate::workspace_rag::RagError> {
        let database = self.database.lock().await;
        crate::workspace_rag::search_workspace_with_progress(
            crate::workspace_rag::SearchWorkspaceInput {
                connection: database.connection(),
                workspace_root,
                query,
                filters,
                limits: &crate::workspace_rag::RetrievalLimits::default(),
                hybrid: &crate::workspace_rag::HybridConfig {
                    enabled: hybrid,
                    ..Default::default()
                },
                loop_config: &crate::workspace_rag::LoopConfig::default(),
                progress,
            },
        )
    }

    pub async fn build_workspace_evidence_context(
        &self,
        workspace_root: &std::path::Path,
        search: &crate::workspace_rag::SearchResult,
    ) -> Result<crate::workspace_rag::ContextBuildResult, crate::workspace_rag::RagError> {
        let database = self.database.lock().await;
        let context = crate::workspace_rag::build_evidence_context(
            database.connection(),
            workspace_root,
            search,
            8_192,
            12,
            32,
        )?;
        crate::workspace_rag::finalize_citations(
            database.connection(),
            workspace_root,
            search,
            context,
        )
    }

    pub async fn finalize_workspace_evidence_context(
        &self,
        workspace_root: &std::path::Path,
        search: &crate::workspace_rag::SearchResult,
        context: crate::workspace_rag::ContextBuildResult,
    ) -> Result<crate::workspace_rag::ContextBuildResult, crate::workspace_rag::RagError> {
        let database = self.database.lock().await;
        crate::workspace_rag::finalize_citations(
            database.connection(),
            workspace_root,
            search,
            context,
        )
    }

    pub async fn build_workspace_vector_index(
        &self,
        workspace_root: &std::path::Path,
        cancellation: &CancellationToken,
    ) -> Result<Option<String>, crate::workspace_rag::RagError> {
        let database_path = self.database_path.as_ref().clone();
        let workspace_root = workspace_root.to_path_buf();
        let cancellation = cancellation.clone();
        tokio::task::spawn_blocking(move || {
            let mut database = LocalDatabase::open(database_path).map_err(|error| {
                crate::workspace_rag::RagError::InvalidConfig(error.to_string())
            })?;
            crate::workspace_rag::build_vector_index(
                database.connection_mut(),
                &workspace_root,
                &crate::workspace_rag::HybridConfig {
                    enabled: true,
                    ..Default::default()
                },
                || cancellation.is_cancelled(),
            )
        })
        .await
        .map_err(|error| crate::workspace_rag::RagError::InvalidConfig(error.to_string()))?
    }

    pub async fn verify_workspace_document_provenance(
        &self,
        workspace_root: &std::path::Path,
        relative_path: &str,
        chunk_hash: &str,
    ) -> Result<bool, crate::workspace_rag::RagError> {
        let database = self.database.lock().await;
        crate::workspace_rag::verify_document_provenance(
            database.connection(),
            workspace_root,
            relative_path,
            chunk_hash,
        )
    }

    /// Атомарная запись `context_ledger` до model call.
    pub async fn record_context_ledger(
        &self,
        entry: &evohime_context_budget::ledger::ContextLedgerEntry,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        let store = evohime_local_storage::context_ledger_store::ContextLedgerStore::new(
            database.connection(),
        )?;
        store.append(entry)
    }

    /// Фиксирует решения compaction/prune в append-only shadow graph до
    /// dispatch. На этом уровне ledger уже содержит идентичности исходных
    /// items, но не их raw payload; поэтому такие записи явно остаются
    /// `metadata_hash_only`, а не выдаются за полную реконструкцию.
    pub async fn record_context_shadowing(
        &self,
        request_id: &str,
        ledger: &evohime_context_budget::ledger::ContextLedgerEntry,
        source_refs: &[evohime_model_provenance::SourceRef],
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        let repository = evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        );
        for compression in &ledger.compression {
            for original_id in &compression.source_ids {
                let shadow_id = format!("{request_id}:summary:{original_id}");
                repository
                    .append_shadow_original(
                        &evohime_local_storage::model_provenance::ShadowOriginalRecord {
                            shadow_id,
                            ledger_id: ledger.id.clone(),
                            request_id: request_id.to_owned(),
                            original_kind: "compression".into(),
                            original_id: original_id.clone(),
                            operation: "summary".into(),
                            parent_shadow_id: None,
                            content_block_hash: None,
                            source_state: "metadata_hash_only".into(),
                            original_content_hash: None,
                            byte_len: 0,
                            created_at: task_memory::now_millis() as i64,
                        },
                        None,
                    )
                    .map_err(|error| StorageError::Context(error.to_string()))?;
            }
        }
        for dropped in &ledger.dropped_items {
            let shadow_id = format!("{request_id}:prune:{}", dropped.id);
            repository
                .append_shadow_original(
                    &evohime_local_storage::model_provenance::ShadowOriginalRecord {
                        shadow_id,
                        ledger_id: ledger.id.clone(),
                        request_id: request_id.to_owned(),
                        original_kind: "dropped".into(),
                        original_id: dropped.id.clone(),
                        operation: "prune".into(),
                        parent_shadow_id: None,
                        content_block_hash: None,
                        source_state: "metadata_hash_only".into(),
                        original_content_hash: None,
                        byte_len: 0,
                        created_at: task_memory::now_millis() as i64,
                    },
                    None,
                )
                .map_err(|error| StorageError::Context(error.to_string()))?;
        }
        for shadow in repository
            .list_shadow_originals(request_id, 4096)
            .map_err(|error| StorageError::Context(error.to_string()))?
        {
            for (source_ref_ordinal, source_ref) in source_refs.iter().enumerate() {
                database
                    .connection()
                    .execute(
                        "INSERT OR IGNORE INTO context_shadow_source_refs(shadow_id,request_id,source_ref_ordinal,source_ordinal) SELECT ?1,?2,?3,ordinal FROM model_request_sources WHERE request_id=?2 AND source_ref_id=?4",
                        rusqlite::params![
                            shadow.shadow_id,
                            request_id,
                            source_ref_ordinal as i64,
                            source_ref.source_ref_id
                        ],
                    )
                    .map_err(StorageError::from)?;
            }
        }
        repository
            .compact_shadow_for_task(&ledger.task_id)
            .map_err(|error| StorageError::Context(error.to_string()))?;
        Ok(())
    }

    /// Единая Core-owned граница provenance: envelope валидируется и
    /// сохраняется до разрешения provider dispatch. Renderer этот API не
    /// видит; он вызывается только из Core model-call orchestration.
    pub async fn commit_model_request(
        &self,
        envelope: &evohime_model_provenance::ModelRequestEnvelopeV1,
        mode: evohime_local_storage::model_provenance::CommitMode,
    ) -> Result<evohime_local_storage::model_provenance::ModelRequestRecord, StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .commit_envelope(envelope, mode)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    /// Durable marker ставится непосредственно перед provider call. Marker
    /// не утверждает, что provider ответил, поэтому recovery может честно
    /// различить crash до и после возможного dispatch.
    pub async fn mark_model_dispatch(&self, request_id: &str, at: i64) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .mark_dispatch(request_id, at)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    pub async fn append_model_request_receipt(
        &self,
        keys: &Arc<ReceiptKeyManager>,
        record: &evohime_local_storage::model_provenance::ModelRequestRecord,
    ) -> Result<(), StorageError> {
        let mut database = self.database.lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let signed = {
            let mut runtime = ReceiptRuntime::new(database.connection_mut(), &signer)
                .map_err(|error| StorageError::Context(error.to_string()))?;
            runtime
                .append_model_request_receipt(evohime_receipts::runtime::ModelRequestReceiptInput {
                    request_id: &record.request_id,
                    logical_request_id: &record.logical_request_id,
                    ledger_id: &record.ledger_id,
                    attempt: record.attempt,
                    provider: &record.provider,
                    model: &record.model,
                    envelope_hash: record.envelope_hash.as_deref().ok_or_else(|| {
                        StorageError::Context("request receipt requires full envelope".into())
                    })?,
                    context_projection_hash: &record.context_projection_hash,
                    route_snapshot_hash: &record.route_snapshot_hash,
                    policy_snapshot_hash: &record.policy_snapshot_hash,
                })
                .map_err(|error| StorageError::Context(error.to_string()))?
        };
        let repository = evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        );
        repository
            .link_request_receipt(
                &evohime_local_storage::model_provenance::RequestReceiptRecord {
                    receipt_id: signed.receipt_id,
                    request_id: signed.request_id,
                    receipt_hash: signed.receipt_hash,
                    request_envelope_hash: record.envelope_hash.clone().unwrap_or_default(),
                    previous_receipt_hash: signed.previous_receipt_hash,
                    key_id: signed.key_id,
                    created_at: signed.created_at_ms,
                },
                &signed.canonical_payload,
            )
            .map_err(|error| StorageError::Context(error.to_string()))
    }

    pub async fn export_model_provenance(
        &self,
        request_id: &str,
        destination: &std::path::Path,
        keys: &Arc<ReceiptKeyManager>,
    ) -> Result<std::path::PathBuf, StorageError> {
        let database = self.database.lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .export_bundle(request_id, destination, &signer)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    /// Stores the provider outcome and closes one previously dispatch-marked
    /// request. The response body is Core-owned and never crosses IPC.
    pub async fn record_model_response(
        &self,
        response: &evohime_local_storage::model_provenance::ModelResponseRecord,
        status: evohime_model_provenance::RequestStatus,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        let repository = evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        );
        repository
            .insert_response(response)
            .and_then(|_| {
                repository.set_status(&response.request_id, status, response.completed_at)
            })
            .map_err(|error| StorageError::Context(error.to_string()))
    }

    pub async fn record_model_tool_intent(
        &self,
        intent: &evohime_local_storage::model_provenance::ToolIntentRecord,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .insert_tool_intent(intent)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    pub async fn link_tool_receipt(
        &self,
        task_id: &str,
        tool_name: &str,
        action_id: &str,
        terminal_receipt_hash: &str,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .link_tool_receipt(task_id, tool_name, action_id, terminal_receipt_hash)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    pub async fn capture_model_workspace_evidence(
        &self,
        request_id: &str,
        source_ref_id: &str,
        path: &std::path::Path,
        source_version: &str,
    ) -> Result<String, StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .capture_workspace_evidence(request_id, source_ref_id, path, source_version)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    pub async fn recover_model_requests(&self) -> Result<usize, StorageError> {
        let database = self.database.lock().await;
        let recovered = evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .recover_active()
        .map_err(|error| StorageError::Context(error.to_string()))?;
        if recovered > 0 {
            let payload = serde_json::to_vec(&serde_json::json!({
                "recovered_requests": recovered,
                "policy": "conservative_no_blind_retry",
            }))
            .map_err(|error| StorageError::Context(error.to_string()))?;
            database.append_event("system", "model_provenance.recovery", &payload)?;
        }
        Ok(recovered)
    }

    pub async fn retain_model_provenance(&self, cutoff: i64) -> Result<usize, StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
            database.connection(),
        )
        .retention_pass(cutoff)
        .map_err(|error| StorageError::Context(error.to_string()))
    }

    /// Append-only запись фактического usage провайдера.
    pub async fn record_context_usage(
        &self,
        usage: &evohime_context_budget::ledger::ContextLedgerUsage,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        let store = evohime_local_storage::context_ledger_store::ContextLedgerStore::new(
            database.connection(),
        )?;
        store.record_usage(usage)
    }

    /// Bounded projection ledger задачи для UI (этап 01.5).
    pub async fn context_ledger_projection(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<
        Vec<evohime_local_storage::context_ledger_store::ContextLedgerProjection>,
        StorageError,
    > {
        let database = self.database.lock().await;
        let store = evohime_local_storage::context_ledger_store::ContextLedgerStore::new(
            database.connection(),
        )?;
        store.projection(task_id, limit)
    }

    /// Запись заметки scratchpad. Подтверждённая запись не перезаписывается
    /// на месте: при попытке silent override возвращается ошибка.
    pub async fn write_scratchpad_entry(
        &self,
        entry: &evohime_context_budget::scratchpad::ScratchpadEntry,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection())
            .upsert(entry)
    }

    /// Подтверждённые записи scratchpad задачи: только они возвращаются в
    /// рабочий контекст после restart.
    pub async fn confirmed_scratchpad(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<evohime_context_budget::scratchpad::ScratchpadEntry>, StorageError> {
        use evohime_context_budget::item::ScratchpadStatus;
        let database = self.database.lock().await;
        evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection()).list(
            task_id,
            None,
            Some(ScratchpadStatus::Confirmed),
            limit,
        )
    }

    /// Восстановление scratchpad после restart: `confirmed` возвращаются в
    /// рабочий контекст, остальные изолируются в recovery view.
    pub async fn recover_scratchpad(
        &self,
        task_id: &str,
        current_step: u32,
    ) -> Result<(usize, usize), StorageError> {
        let now = task_memory::now_millis() as i64;
        let database = self.database.lock().await;
        let store =
            evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection());
        store.mark_unconfirmed_as_recovered(task_id, now, current_step)?;
        let (restored, isolated) = store.recover(task_id, now, current_step)?;
        store.discard_expired_recovered(
            task_id,
            evohime_context_budget::scratchpad::RecoveryPolicy::default(),
            now,
            current_step,
        )?;
        Ok((restored.len(), isolated.len()))
    }

    /// Выгрузка перечисленных записей scratchpad в artifact store. Содержимое
    /// заменяется bounded summary с hash и locator; запись остаётся `confirmed`,
    /// а её ревизия не меняется.
    pub async fn offload_scratchpad_entries(
        &self,
        task_id: &str,
        ids: &[String],
        now: i64,
    ) -> Result<usize, StorageError> {
        let database = self.database.lock().await;
        let store =
            evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection());
        let artifacts =
            evohime_local_storage::artifact_store::ArtifactStore::new(database.connection());
        let kind = evohime_context_budget::item::ItemKind::Scratchpad.as_str();
        let mut offloaded = 0;
        for id in ids {
            let Some(mut entry) = store.get(id)? else {
                continue;
            };
            if entry.artifact_locator.is_some() || !entry.privacy.allows_offload() {
                continue;
            }
            let result =
                artifacts.offload(kind, task_id, task_id, &entry.content, entry.privacy, now)?;
            entry.artifact_locator = Some(result.reference.locator);
            entry.updated_at = now;
            store.upsert(&entry)?;
            offloaded += 1;
        }
        Ok(offloaded)
    }

    /// Bounded projection scratchpad задачи для UI (этап 01.5).
    pub async fn scratchpad_projection(
        &self,
        task_id: &str,
        category: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<evohime_local_storage::scratchpad_store::ScratchpadProjection>, StorageError>
    {
        use evohime_context_budget::{item::ScratchpadStatus, scratchpad::ScratchpadCategory};
        let database = self.database.lock().await;
        let store =
            evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection());
        let category = category.and_then(ScratchpadCategory::parse);
        let status = status.and_then(|value| match value {
            "draft" => Some(ScratchpadStatus::Draft),
            "confirmed" => Some(ScratchpadStatus::Confirmed),
            "recovered" => Some(ScratchpadStatus::Recovered),
            _ => None,
        });
        store.projection(task_id, category, status, limit, 200)
    }

    /// Очистка task-scoped scratchpad вместе с закреплениями задачи.
    pub async fn clear_task_scratchpad(&self, task_id: &str) -> Result<usize, StorageError> {
        let database = self.database.lock().await;
        let commands = evohime_local_storage::context_command_store::ContextCommandStore::new(
            database.connection(),
        );
        commands.check_rate_limit(
            task_id,
            "clear_task_scratchpad",
            task_memory::now_millis() as i64,
        )?;
        let store =
            evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection());
        let removed = store.clear_task(task_id)?;
        commands.clear_task(task_id, task_memory::now_millis() as i64)?;
        Ok(removed)
    }

    /// Atomically accepts one outgoing message into the Core-owned
    /// conversation log before the task is dispatched. A retry with the same
    /// client id returns the original event and task binding; conflicting
    /// content fails closed.
    pub async fn accept_conversation_message(
        &self,
        conversation_id: &str,
        workspace_id: &str,
        task_id: &str,
        client_message_id: &str,
        content: &str,
    ) -> Result<
        (
            evohime_local_storage::conversation_event_log_store::MessageAcceptance,
            i64,
        ),
        StorageError,
    > {
        use sha2::{Digest, Sha256};

        let draft = crate::conversation_event_log::user_message_draft(content)
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
        let database = self.database.lock().await;
        let acceptance = evohime_local_storage::conversation_event_log_store::accept_message(
            database.connection(),
            evohime_local_storage::conversation_event_log_store::AcceptMessageInput {
                conversation_id,
                workspace_id,
                task_id,
                client_message_id,
                authoritative_payload: &draft.authoritative_payload,
                renderer_payload: &draft.renderer_payload,
                content_hash: &content_hash,
                timestamp_ms: task_memory::now_millis() as i64,
            },
        )?;
        let renderer = crate::conversation_event_log::renderer_event(&acceptance.event)
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let delivery_sequence = database.append_event(
            task_id,
            "conversation.event",
            &serde_json::to_vec(&renderer)?,
        )?;
        Ok((acceptance, delivery_sequence))
    }

    pub async fn conversation_history_after(
        &self,
        conversation_id: &str,
        after_sequence: u64,
        limit: usize,
    ) -> Result<
        evohime_local_storage::conversation_event_log_store::ConversationEventPage,
        StorageError,
    > {
        let database = self.database.lock().await;
        Ok(
            evohime_local_storage::conversation_event_log_store::history_after(
                database.connection(),
                conversation_id,
                after_sequence,
                limit,
            )?,
        )
    }

    pub async fn record_conversation_usage(
        &self,
        task_id: &str,
        payload: serde_json::Value,
    ) -> Result<(), StorageError> {
        let bytes = serde_json::to_vec(&payload)?;
        let drafts = crate::conversation_event_log::project_core_event("model.usage", &bytes)
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let database = self.database.lock().await;
        let Some((conversation_id, client_message_id, workspace_id)) =
            evohime_local_storage::conversation_event_log_store::task_binding(
                database.connection(),
                task_id,
            )?
        else {
            return Ok(());
        };
        for draft in drafts {
            let stored = evohime_local_storage::conversation_event_log_store::append_event(
                database.connection(),
                evohime_local_storage::conversation_event_log_store::NewConversationEvent {
                    conversation_id: &conversation_id,
                    workspace_id: &workspace_id,
                    kind: &draft.kind,
                    category: &draft.category,
                    authoritative_payload: &draft.authoritative_payload,
                    renderer_payload: &draft.renderer_payload,
                    correlation_id: Some(&client_message_id),
                    causation_id: None,
                    task_id: Some(task_id),
                    run_id: None,
                    turn_id: Some(task_id),
                    client_message_id: None,
                    persistence_class: &draft.persistence_class,
                    sensitivity: &draft.sensitivity,
                    timestamp_ms: task_memory::now_millis() as i64,
                },
            )?;
            let renderer = crate::conversation_event_log::renderer_event(&stored)
                .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
            database.append_event(
                task_id,
                "conversation.event",
                &serde_json::to_vec(&renderer)?,
            )?;
        }
        Ok(())
    }

    pub async fn claim_conversation_dispatch(
        &self,
        conversation_id: &str,
        client_message_id: &str,
    ) -> Result<bool, StorageError> {
        let database = self.database.lock().await;
        Ok(
            evohime_local_storage::conversation_event_log_store::claim_message_dispatch(
                database.connection(),
                conversation_id,
                client_message_id,
            )?,
        )
    }

    pub async fn finish_conversation_dispatch(
        &self,
        conversation_id: &str,
        client_message_id: &str,
        dispatched: bool,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        Ok(
            evohime_local_storage::conversation_event_log_store::finish_message_dispatch(
                database.connection(),
                conversation_id,
                client_message_id,
                dispatched,
            )?,
        )
    }

    pub async fn conversation_history_before(
        &self,
        conversation_id: &str,
        before_sequence: u64,
        limit: usize,
    ) -> Result<
        evohime_local_storage::conversation_event_log_store::ConversationEventPage,
        StorageError,
    > {
        let database = self.database.lock().await;
        Ok(
            evohime_local_storage::conversation_event_log_store::history_before(
                database.connection(),
                conversation_id,
                before_sequence,
                limit,
            )?,
        )
    }

    /// Запрос `summarize now` на текущую сборку контекста задачи.
    pub async fn request_context_summarize(&self, task_id: &str) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::context_command_store::ContextCommandStore::new(
            database.connection(),
        )
        .request_summarize(task_id, task_memory::now_millis() as i64)
    }

    /// `pin/unpin item` для сборки контекста задачи.
    pub async fn set_context_pin(
        &self,
        task_id: &str,
        item_id: &str,
        pinned: bool,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::context_command_store::ContextCommandStore::new(
            database.connection(),
        )
        .set_pin(task_id, item_id, pinned, task_memory::now_millis() as i64)
    }

    /// Чтение полного содержимого артефакта: доступ ограничен задачей-владельцем
    /// и её детьми, а `content_hash` сверяется заново.
    pub async fn read_context_artifact(
        &self,
        task_id: &str,
        locator: &str,
    ) -> Result<String, StorageError> {
        let database = self.database.lock().await;
        let store =
            evohime_local_storage::artifact_store::ArtifactStore::new(database.connection());
        let reference = store
            .get_ref(locator)?
            .ok_or_else(|| StorageError::Context(format!("artifact {locator} was not found")))?;
        let kind = evohime_context_budget::item::ItemKind::ToolResult.as_str();
        store.read(
            locator,
            task_id,
            std::slice::from_ref(&reference.owner_task_id),
            kind,
            task_memory::now_millis() as i64,
        )
    }

    /// Каскад `forget memory` (01.5): вместе с записью памяти удаляются
    /// производные scratchpad-ссылки и task artifacts. Факт удаления остаётся
    /// в аудите в redacted виде.
    pub async fn forget_context_derivatives(
        &self,
        task_id: &str,
        memory_id: &str,
    ) -> Result<(usize, usize), StorageError> {
        let now = task_memory::now_millis() as i64;
        let database = self.database.lock().await;
        let scratchpad =
            evohime_local_storage::scratchpad_store::ScratchpadStore::new(database.connection());
        let removed_notes = scratchpad.forget(memory_id)?;
        let artifacts =
            evohime_local_storage::artifact_store::ArtifactStore::new(database.connection());
        let removed_artifacts =
            artifacts.forget_task_artifacts(task_id, now, "forget memory cascade")?;
        let commands = evohime_local_storage::context_command_store::ContextCommandStore::new(
            database.connection(),
        );
        commands.audit(
            task_id,
            "forget_memory_cascade",
            Some(memory_id),
            evohime_local_storage::context_command_store::CommandOutcome::Applied,
            now,
        )?;
        Ok((removed_notes, removed_artifacts))
    }

    /// Ротация ledger. Возвращает число удалённых записей.
    pub async fn prune_context_ledger(&self, now: i64) -> Result<u64, StorageError> {
        let database = self.database.lock().await;
        let store = evohime_local_storage::context_ledger_store::ContextLedgerStore::new(
            database.connection(),
        )?;
        store.prune(now)
    }

    pub async fn record(&self, event: &CoreEvent) -> Result<i64, StorageError> {
        let task_id = match event {
            CoreEvent::ModelContext { task_id, .. }
            | CoreEvent::RoutingTrace { task_id, .. }
            | CoreEvent::PendingRoutingApproval { task_id, .. }
            | CoreEvent::TaskStarted { task_id, .. }
            | CoreEvent::AssistantDelta { task_id, .. }
            | CoreEvent::ToolStarted { task_id, .. }
            | CoreEvent::ToolOutput { task_id, .. }
            | CoreEvent::ApprovalRequired { task_id, .. }
            | CoreEvent::TaskCompleted { task_id, .. }
            | CoreEvent::TaskFailed { task_id, .. }
            | CoreEvent::TaskStopped { task_id } => task_id,
            CoreEvent::ReviewProgress { review_id, .. } => review_id,
            CoreEvent::RevisionProgress { revision_id, .. } => revision_id,
            CoreEvent::StorageProgress { operation_id, .. } => operation_id,
            CoreEvent::WorkspaceIndexProgress { .. }
            | CoreEvent::WorkspaceRetrievalProgress { .. } => "workspace-rag",
            CoreEvent::ReviewHistoryCleared { marker_id } => marker_id,
            CoreEvent::ChildWorkflowProjection { task_id, .. } => task_id,
            CoreEvent::WorkflowProgress { run_id, .. } => run_id,
            CoreEvent::WorkspaceBootstrapManifest { workspace_id, .. } => workspace_id,
            CoreEvent::TeamCoordinationPolicies { team_id, .. } => team_id,
            CoreEvent::TypedAgentHandoffContract { handoff_id, .. } => handoff_id,
            CoreEvent::SchemaDrivenAgentConfiguration { scope, .. } => scope,
            CoreEvent::ExperienceReplayLibrary { scope, .. } => scope,
            CoreEvent::RuntimeInterventionPipeline { run_id, .. } => run_id,
            CoreEvent::CodeDiagnosticsFeedbackLoop {
                workspace_root_id, ..
            } => workspace_root_id,
            CoreEvent::WorkflowOptimizationLab { run_id, .. } => run_id,
            CoreEvent::CoreTopicSubscriptionEventBus { .. } => "core-topic-bus",
            CoreEvent::DependencyAwareTaskGraph { graph_id, .. } => graph_id,
            CoreEvent::DeclarativeAgentComponentRegistry { registry_id, .. } => registry_id,
            CoreEvent::TypedContextReferences { ref_id, .. } => ref_id,
            CoreEvent::SafeUiExtensionFramework { extension_id, .. } => extension_id,
            CoreEvent::CapabilityWorkbench { instance_id, .. } => instance_id,
            CoreEvent::TeamCoordinator { work_item_id, .. } => work_item_id,
            CoreEvent::ProjectInstructionStack { workspace_root, .. } => workspace_root,
            CoreEvent::WorkspaceSets { set_id, .. } => set_id,
            CoreEvent::KnowledgeSourceRegistryProjectRole { source_id, .. } => source_id,
            CoreEvent::DurableRemoteTaskBridge { remote_task_id, .. } => remote_task_id,
            CoreEvent::MessageInterventionPolicies { operation, .. } => operation,
            CoreEvent::BatchInvocationRuntime { batch_id, .. } => batch_id,
            CoreEvent::PolicyAwareToolResultCache { cache_key, .. } => cache_key,
            CoreEvent::CodeAnchoredIntentMarkers { operation, .. } => operation,
            CoreEvent::ModelPurposeRouting { operation, .. } => operation,
            CoreEvent::LocalModelRuntimeManager { operation, .. } => operation,
            CoreEvent::ArchitectureSnapshot { operation, .. } => operation,
            CoreEvent::AgentGitChangeSets { change_set_id, .. } => change_set_id,
            CoreEvent::ArchitectEditorModelPipeline { pipeline_id, .. } => pipeline_id,
            CoreEvent::EventVisualizerRegistry { visualizer_id, .. } => visualizer_id,
            CoreEvent::ReasoningOperatorLibrary { operator_id, .. } => operator_id,
            CoreEvent::OutputGuardrailPipeline { pipeline_id, .. } => pipeline_id,
            CoreEvent::CustomizationInventory { item_id, .. } => item_id,
            CoreEvent::StandingApprovalProfiles { profile_id, .. } => profile_id,
            CoreEvent::ApprovalPolicyProfiles { profile_id, .. } => profile_id,
            CoreEvent::CheckpointForking { fork_run_id, .. } => fork_run_id,
            CoreEvent::PrivacyTelemetryGovernance { category, .. } => category,
            CoreEvent::ConversationBridgeAdapters { bridge_id, .. } => bridge_id,
            CoreEvent::MemoryViewsAndAdaptiveRecall { view_id, .. } => view_id,
            CoreEvent::ModelEditProtocolRegistry { protocol_id, .. } => protocol_id,
            CoreEvent::RemoteConversationChannels { connection_id, .. } => connection_id,
            CoreEvent::PromptCachePlanner { plan_id, .. } => plan_id,
            CoreEvent::DeclarativeRuntimeComponents { component_id, .. } => component_id,
            CoreEvent::GuidedCalibrationSessions { session_id, .. } => session_id,
            CoreEvent::ExtensionConformanceKit { subject_id, .. } => subject_id,
            CoreEvent::PersistentAgentOrganizationRegistry { agent_id, .. } => agent_id,
        };
        let event_type = match event {
            CoreEvent::ModelContext { .. } => "model.context",
            CoreEvent::RoutingTrace { .. } => "routing.terminal",
            CoreEvent::PendingRoutingApproval { .. } => "routing.pending_approval",
            CoreEvent::TaskStarted { .. } => "task.started",
            CoreEvent::AssistantDelta { .. } => "agent.message.delta",
            CoreEvent::ToolStarted { .. } => "tool.started",
            CoreEvent::ToolOutput { .. } => "tool.output",
            CoreEvent::ApprovalRequired { .. } => "approval.required",
            CoreEvent::TaskCompleted { .. } => "task.completed",
            CoreEvent::TaskFailed { .. } => "task.failed",
            CoreEvent::TaskStopped { .. } => "task.stopped",
            CoreEvent::ReviewProgress { .. } => "review.progress",
            CoreEvent::RevisionProgress { .. } => "revision.progress",
            CoreEvent::StorageProgress { .. } => "storage.progress",
            CoreEvent::WorkspaceIndexProgress { .. } => "workspace.index_progress",
            CoreEvent::WorkspaceRetrievalProgress { .. } => "workspace.retrieval_progress",
            CoreEvent::ReviewHistoryCleared { .. } => "review.history_cleared",
            CoreEvent::ChildWorkflowProjection { .. } => "child.workflow",
            CoreEvent::WorkflowProgress { .. } => "workflow.progress",
            CoreEvent::WorkspaceBootstrapManifest { .. } => "workspace_bootstrap_manifest.result",
            CoreEvent::TeamCoordinationPolicies { .. } => "team_coordination_policies.result",
            CoreEvent::TypedAgentHandoffContract { .. } => "typed_agent_handoff_contract.result",
            CoreEvent::SchemaDrivenAgentConfiguration { .. } => {
                "schema_driven_agent_configuration.result"
            }
            CoreEvent::ExperienceReplayLibrary { .. } => "experience_replay_library.result",
            CoreEvent::RuntimeInterventionPipeline { .. } => "runtime_intervention_pipeline.result",
            CoreEvent::CodeDiagnosticsFeedbackLoop { .. } => {
                "code_diagnostics_feedback_loop.result"
            }
            CoreEvent::WorkflowOptimizationLab { .. } => "workflow_optimization_lab.result",
            CoreEvent::CoreTopicSubscriptionEventBus { .. } => {
                "core_topic_subscription_event_bus.result"
            }
            CoreEvent::DependencyAwareTaskGraph { .. } => "dependency_aware_task_graph.result",
            CoreEvent::DeclarativeAgentComponentRegistry { .. } => {
                "declarative_agent_component_registry.result"
            }
            CoreEvent::TypedContextReferences { .. } => "typed_context_references.result",
            CoreEvent::SafeUiExtensionFramework { .. } => "safe_ui_extension_framework.result",
            CoreEvent::CapabilityWorkbench { .. } => "capability_workbench.result",
            CoreEvent::TeamCoordinator { .. } => "team_coordinator.result",
            CoreEvent::ProjectInstructionStack { .. } => "project_instruction_stack.result",
            CoreEvent::WorkspaceSets { .. } => "workspace_sets.result",
            CoreEvent::KnowledgeSourceRegistryProjectRole { .. } => {
                "knowledge_source_registry.result"
            }
            CoreEvent::DurableRemoteTaskBridge { .. } => "durable_remote_task_bridge.result",
            CoreEvent::MessageInterventionPolicies { .. } => "message_intervention_policies.result",
            CoreEvent::BatchInvocationRuntime { .. } => "batch_invocation_runtime.result",
            CoreEvent::PolicyAwareToolResultCache { .. } => "policy_aware_tool_result_cache.result",
            CoreEvent::CodeAnchoredIntentMarkers { .. } => "code_anchored_intent_markers.result",
            CoreEvent::ModelPurposeRouting { .. } => "model_purpose_routing.result",
            CoreEvent::LocalModelRuntimeManager { .. } => "local_model_runtime_manager.result",
            CoreEvent::ArchitectureSnapshot { .. } => "architecture_snapshot.result",
            CoreEvent::AgentGitChangeSets { .. } => "agent_git_change_sets.result",
            CoreEvent::ArchitectEditorModelPipeline { .. } => "architect_editor_pipeline.result",
            CoreEvent::EventVisualizerRegistry { .. } => "event_visualizer_registry.result",
            CoreEvent::ReasoningOperatorLibrary { .. } => "reasoning_operator_library.result",
            CoreEvent::OutputGuardrailPipeline { .. } => "output_guardrail_pipeline.result",
            CoreEvent::CustomizationInventory { .. } => "customization_inventory.result",
            CoreEvent::StandingApprovalProfiles { .. } => "standing_approval_profiles.result",
            CoreEvent::ApprovalPolicyProfiles { .. } => "approval_policy_profiles.result",
            CoreEvent::CheckpointForking { .. } => "checkpoint_forking.result",
            CoreEvent::PrivacyTelemetryGovernance { .. } => "privacy_telemetry_governance.result",
            CoreEvent::ConversationBridgeAdapters { .. } => "conversation_bridge_adapters.result",
            CoreEvent::MemoryViewsAndAdaptiveRecall { .. } => {
                "memory_views_and_adaptive_recall.result"
            }
            CoreEvent::ModelEditProtocolRegistry { .. } => "model_edit_protocol_registry.result",
            CoreEvent::RemoteConversationChannels { .. } => "remote_conversation_channels.result",
            CoreEvent::PromptCachePlanner { .. } => "prompt_cache_planner.result",
            CoreEvent::DeclarativeRuntimeComponents { .. } => {
                "declarative_runtime_components.result"
            }
            CoreEvent::GuidedCalibrationSessions { .. } => "guided_calibration_sessions.result",
            CoreEvent::ExtensionConformanceKit { .. } => "extension_conformance_kit.result",
            CoreEvent::PersistentAgentOrganizationRegistry { .. } => {
                "persistent_agent_organization_registry.result"
            }
        };
        let payload = match event {
            CoreEvent::StorageProgress { progress, .. } => {
                serde_json::to_vec(progress).expect("storage progress serializes")
            }
            CoreEvent::WorkspaceIndexProgress { progress, .. } => {
                serde_json::to_vec(progress).expect("workspace index progress serializes")
            }
            CoreEvent::WorkspaceRetrievalProgress { progress, .. } => {
                serde_json::to_vec(progress).expect("workspace retrieval progress serializes")
            }
            CoreEvent::ChildWorkflowProjection { projection, .. } => {
                serde_json::to_vec(projection).expect("child projection serializes")
            }
            CoreEvent::WorkflowProgress { projection, .. } => {
                serde_json::to_vec(projection).expect("workflow projection serializes")
            }
            CoreEvent::WorkspaceBootstrapManifest { .. } => {
                serde_json::to_vec(event).expect("bootstrap projection serializes")
            }
            CoreEvent::TeamCoordinationPolicies { .. } => {
                serde_json::to_vec(event).expect("team coordination projection serializes")
            }
            CoreEvent::MemoryViewsAndAdaptiveRecall { .. } => {
                serde_json::to_vec(event).expect("memory view projection serializes")
            }
            CoreEvent::ModelEditProtocolRegistry { .. } => {
                serde_json::to_vec(event).expect("model edit projection serializes")
            }
            CoreEvent::RemoteConversationChannels { .. } => {
                serde_json::to_vec(event).expect("remote channel projection serializes")
            }
            CoreEvent::PromptCachePlanner { .. } => {
                serde_json::to_vec(event).expect("prompt cache projection serializes")
            }
            CoreEvent::TypedAgentHandoffContract { .. } => {
                serde_json::to_vec(event).expect("handoff projection serializes")
            }
            _ => serde_json::to_vec(event).expect("core events serialize"),
        };
        // Conversation projection is additive and must not break the existing
        // bounded global journal. If a legacy event is too large or malformed,
        // keep its payload out of the conversation log and record only a
        // non-authoritative metadata marker for the bound conversation.
        let projected = crate::conversation_event_log::project_core_event(event_type, &payload)
            .or_else(|_| {
                crate::conversation_event_log::project_core_event(
                    "conversation.projection_failed",
                    br#"{}"#,
                )
            })
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let database = self.database.lock().await;
        let mut last_sequence = database.append_event(task_id, event_type, &payload)?;
        if let Some((conversation_id, client_message_id, workspace_id)) =
            evohime_local_storage::conversation_event_log_store::task_binding(
                database.connection(),
                task_id,
            )?
        {
            for draft in projected {
                let stored = evohime_local_storage::conversation_event_log_store::append_event(
                    database.connection(),
                    evohime_local_storage::conversation_event_log_store::NewConversationEvent {
                        conversation_id: &conversation_id,
                        workspace_id: &workspace_id,
                        kind: &draft.kind,
                        category: &draft.category,
                        authoritative_payload: &draft.authoritative_payload,
                        renderer_payload: &draft.renderer_payload,
                        correlation_id: Some(&client_message_id),
                        causation_id: Some(&client_message_id),
                        task_id: Some(task_id),
                        run_id: Some(task_id),
                        turn_id: Some(task_id),
                        client_message_id: Some(&client_message_id),
                        persistence_class: &draft.persistence_class,
                        sensitivity: &draft.sensitivity,
                        timestamp_ms: task_memory::now_millis() as i64,
                    },
                )?;
                let renderer = crate::conversation_event_log::renderer_event(&stored)
                    .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
                last_sequence = database.append_event(
                    task_id,
                    "conversation.event",
                    &serde_json::to_vec(&renderer)?,
                )?;
            }
        }
        Ok(last_sequence)
    }

    pub async fn record_tool_metric(&self, metric: ToolMetric<'_>) -> Result<i64, StorageError> {
        let database = self.database.lock().await;
        database.record_tool_metric(evohime_local_storage::ToolMetricInput {
            task_id: metric.task_id,
            tool_name: metric.tool_name,
            iteration: metric.iteration.min(i64::MAX as usize) as i64,
            ok: metric.ok,
            failure_kind: metric.failure_kind,
            recovery_hint: metric.recovery_hint,
            escalated: metric.escalated,
        })
    }

    pub async fn tool_metrics(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<ToolMetricRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_tool_metrics(task_id, limit)
    }

    pub async fn search_lessons(
        &self,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::search_lessons(
            database.connection(),
            evohime_local_storage::memory_store::MemoryScope::Project,
            scope_id,
            query,
            now,
            limit,
        )
        .map_err(|error| StorageError::InvalidRecovery(error.to_string()))
    }

    pub async fn record_lesson(
        &self,
        record: &evohime_local_storage::memory_store::MemoryRecord,
    ) -> Result<evohime_local_storage::memory_store::MemoryRecord, StorageError> {
        crate::memory_governance::MemoryWriteGate::validate(record)
            .map_err(|error| StorageError::InvalidRecovery(error.to_string()))?;
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::upsert_lesson(
            database.connection(),
            record,
        )
        .map_err(|error| StorageError::InvalidRecovery(error.to_string()))
    }

    pub async fn replay(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_events_after(after_sequence, limit)
    }

    /// Highest recorded sequence; zero when nothing has been journalled yet.
    pub async fn latest_sequence(&self) -> i64 {
        let database = self.database.lock().await;
        database.latest_event_sequence().unwrap_or(0)
    }

    pub async fn replay_bounded(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<DurableReplayBatch, StorageError> {
        const MAX_DURABLE_REPLAY_EVENTS: usize = 512;
        let records = {
            let database = self.database.lock().await;
            database.read_events_after(after_sequence, limit.min(MAX_DURABLE_REPLAY_EVENTS))?
        };
        let first_available_sequence = records.first().map(|record| record.sequence_id);
        let gap_detected =
            first_available_sequence.is_some_and(|first| after_sequence.saturating_add(1) < first);
        let last_sequence = records
            .last()
            .map(|record| record.sequence_id)
            .unwrap_or(after_sequence);
        Ok(DurableReplayBatch {
            events: records,
            gap_detected,
            first_available_sequence,
            last_sequence,
        })
    }

    pub async fn review_history(&self, limit: usize) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_review_events(limit)
    }

    pub async fn preview_database_backup(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<BackupPreview, StorageError> {
        LocalDatabase::preview_backup(path)
    }

    pub async fn create_database_backup(
        &self,
        path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress),
    ) -> Result<BackupResult, StorageError> {
        let database = self.database.lock().await;
        database.create_backup(path, app_version, progress)
    }

    pub async fn create_database_backup_with_cancel(
        &self,
        path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress),
        cancelled: impl FnMut() -> bool,
    ) -> Result<BackupResult, StorageError> {
        let database = self.database.lock().await;
        database.create_backup_with_cancel(path, app_version, progress, cancelled)
    }

    pub async fn restore_database(
        &self,
        backup_path: impl AsRef<std::path::Path>,
        safety_path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress),
    ) -> Result<RestoreResult, StorageError> {
        let mut database = self.database.lock().await;
        database.restore_backup(backup_path, safety_path, app_version, progress)
    }

    pub async fn restore_database_with_cancel(
        &self,
        backup_path: impl AsRef<std::path::Path>,
        safety_path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress),
        cancelled: impl FnMut() -> bool,
    ) -> Result<RestoreResult, StorageError> {
        let mut database = self.database.lock().await;
        database.restore_backup_with_cancel(
            backup_path,
            safety_path,
            app_version,
            progress,
            cancelled,
        )
    }

    /// Bounded, read-only storage facts for diagnostics (Core Doctor).
    pub async fn storage_snapshot(&self) -> Result<(PathBuf, u32), StorageError> {
        let database = self.database.lock().await;
        Ok((database.path().to_path_buf(), database.schema_version()?))
    }

    /// Bounded, read-only recovery facts for diagnostics (Core Doctor). This
    /// only performs SELECTs and never mutates run/effect state.
    pub async fn recovery_probe(&self) -> Result<crate::doctor::RecoveryProbe, StorageError> {
        let database = self.database.lock().await;
        let health = database.read_recovery_health()?;
        let state = if health.unknown_effects > 0 || health.lease_expired {
            "BLOCKED"
        } else if health.resumable_runs > 0 {
            "RESUMABLE"
        } else {
            "CLEAN"
        };
        Ok(crate::doctor::RecoveryProbe {
            state: state.into(),
            unknown_effects: health.unknown_effects.max(0) as u32,
            lease_expired: health.lease_expired,
            resumable_runs: health.resumable_runs.max(0) as u32,
        })
    }

    pub async fn transition_recovery(
        &self,
        transition: RecoveryTransition<'_>,
    ) -> Result<RunRecoveryRecord, StorageError> {
        let database = self.database.lock().await;
        database.transition_recovery(evohime_local_storage::RecoveryTransitionInput {
            run_id: transition.run_id,
            next: transition.state,
            effect_id: transition.effect_id,
            idempotency_key: transition.idempotency_key,
            verifier: transition.verifier,
            evidence_json: transition.evidence_json,
            decision: transition.decision,
        })
    }

    pub async fn create_project(
        &self,
        id: &str,
        title: &str,
        workspace_path: &str,
        source_ref: Option<&str>,
    ) -> Result<evohime_local_storage::ProjectRecord, StorageError> {
        let database = self.database.lock().await;
        database.create_project(id, title, workspace_path, source_ref)
    }

    pub async fn get_project(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::ProjectRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_project(id)
    }

    pub async fn get_project_by_workspace_path(
        &self,
        workspace_path: &str,
    ) -> Result<Option<evohime_local_storage::ProjectRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_project_by_workspace_path(workspace_path)
    }

    /// Persists one redacted, bounded research evidence record against the
    /// real `research_evidence` table (SCHEMA_VERSION 8).
    pub async fn save_research_evidence(
        &self,
        record: &evohime_local_storage::research_store::ResearchEvidenceRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::research_store::ResearchEvidenceSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists research evidence records tied to a work item, oldest id first.
    pub async fn list_research_evidence(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<evohime_local_storage::research_store::ResearchEvidenceRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::research_store::ResearchEvidenceSql::list_by_provenance(
            database.connection(),
            work_item_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one bounded, redacted Memory v1 record against the real
    /// `memory_entries` table (SCHEMA_VERSION 8).
    pub async fn save_memory(
        &self,
        record: &evohime_local_storage::memory_store::MemoryRecord,
    ) -> Result<(), String> {
        let mut governed = record.clone();
        if matches!(
            governed.extraction.confirmation_state.as_str(),
            "candidate" | "pending_confirmation"
        ) && governed.extraction.authority == "user_asserted"
        {
            governed.extraction.authority = "model_proposed".to_owned();
        }
        crate::memory_governance::MemoryWriteGate::validate(&governed)
            .map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::insert(
            database.connection(),
            &governed,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists non-forgotten Memory v1 records for one exact scope.
    pub async fn list_memory(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::list(
            database.connection(),
            scope,
            scope_id,
            include_archived,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Lexical, deterministic search over Memory v1 records for one exact
    /// scope.
    pub async fn search_memory(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::search(
            database.connection(),
            scope,
            scope_id,
            query,
            now,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Searches project-scoped memories for the current workspace so the
    /// agent can use user-created facts and decisions, not only automatic
    /// failure lessons.
    pub async fn search_workspace_memory(
        &self,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        self.search_memory(
            evohime_local_storage::memory_store::MemoryScope::Project,
            scope_id,
            query,
            now,
            limit,
        )
        .await
    }

    /// Archives a memory record. Returns `false` if no matching, non-forgotten
    /// record was found.
    pub async fn archive_memory(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::archive(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Forgets (erases title/content of) a memory record. Returns `false` if
    /// no matching row was found.
    pub async fn forget_memory(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::forget(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Reads one memory record by id, including body. Privacy redaction is
    /// applied by the caller, not here.
    pub async fn get_memory(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::get_by_id(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Records in one `confirmation_state` for one exact scope: the pending
    /// queue and the rejected/superseded history use the same path.
    pub async fn list_memory_by_state(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
        state: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::list_by_state(
            database.connection(),
            scope,
            scope_id,
            state,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Per-state counters for OperationsPanel; never exposes any body.
    pub async fn count_memory_by_state(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
    ) -> Result<Vec<(String, i64)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::count_by_state(
            database.connection(),
            scope,
            scope_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Active records of one kind in one scope: the input for deterministic
    /// conflict detection in `memory_extraction::detect_conflict`.
    pub async fn memory_conflict_candidates(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
        kind: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::conflict_candidates(
            database.connection(),
            scope,
            scope_id,
            kind,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Idempotent state transition. Repeated confirm/reject is safe and
    /// returns the actual current state.
    pub async fn transition_memory_state(&self, id: &str, target: &str) -> Result<String, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::transition_state(
            database.connection(),
            id,
            target,
        )
        .map_err(|error| error.to_string())
    }

    /// Replaces a pending candidate's statement with one the user wrote.
    pub async fn revise_pending_memory(&self, id: &str, statement: &str) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::revise_pending_statement(
            database.connection(),
            id,
            statement,
        )
        .map_err(|error| error.to_string())
    }

    /// Applies an explicit user choice: `old_id` is superseded by `new_id`.
    pub async fn supersede_memory(
        &self,
        old_id: &str,
        new_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::supersede(
            database.connection(),
            old_id,
            new_id,
            reason,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn memory_supersession_chain(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::supersession_chain(
            database.connection(),
            id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Marks due records `expired` so they leave retrieval without any
    /// hidden action on stale content.
    pub async fn expire_due_memory(&self, now: &str) -> Result<usize, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::expire_due(database.connection(), now)
            .map_err(|error| error.to_string())
    }

    /// Logical deletion plus a tombstone that carries only metadata and a
    /// digest — never the original text.
    pub async fn forget_memory_with_tombstone(
        &self,
        id: &str,
        tombstone_id: &str,
        reason_class: &str,
        forgotten_at: &str,
    ) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::forget_with_tombstone(
            database.connection(),
            id,
            tombstone_id,
            reason_class,
            forgotten_at,
        )
        .map_err(|error| error.to_string())
    }

    /// Registered aliases for the scope, feeding
    /// `memory_extraction::AliasTable`. Model inference can never add one.
    pub async fn list_memory_aliases(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::list_aliases(
            database.connection(),
            scope,
            scope_id,
        )
        .map_err(|error| error.to_string())
    }

    /// "Only for this session": a session-scoped row with automatic expiry
    /// that never becomes persistent memory.
    pub async fn save_memory_session_note(
        &self,
        note: SessionMemoryNote<'_>,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::insert_session_note(
            database.connection(),
            evohime_local_storage::memory_store::InsertSessionNoteInput {
                id: note.id,
                session_id: note.session_id,
                scope: note.scope,
                scope_id: note.scope_id,
                kind: note.kind,
                statement: note.statement,
                created_at: note.created_at,
                expires_at: note.expires_at,
            },
        )
        .map_err(|error| error.to_string())
    }

    pub async fn list_memory_session_notes(
        &self,
        session_id: &str,
        now: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::list_session_notes(
            database.connection(),
            session_id,
            now,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn purge_expired_memory_session_notes(&self, now: &str) -> Result<usize, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::purge_expired_session_notes(
            database.connection(),
            now,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one bounded, redacted feedback record against the real
    /// `feedback_entries` table (SCHEMA_VERSION 14). Feedback never leaves
    /// this local table; see `evohime_local_storage::feedback_store::external_telemetry_allowed`.
    pub async fn save_feedback(
        &self,
        record: &evohime_local_storage::feedback_store::FeedbackRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::feedback_store::FeedbackStoreSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists feedback tied to one run, newest first.
    pub async fn list_feedback(
        &self,
        run_id: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::feedback_store::FeedbackRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::feedback_store::FeedbackStoreSql::list_by_run(
            database.connection(),
            run_id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Local aggregation: signal counts plus top rejection reasons/outcomes
    /// by frequency. No data leaves the local store as part of this call.
    pub async fn aggregate_feedback(
        &self,
        reason_limit: u32,
        outcome_limit: u32,
    ) -> Result<evohime_local_storage::feedback_store::FeedbackAggregate, String> {
        let database = self.database.lock().await;
        evohime_local_storage::feedback_store::FeedbackStoreSql::aggregate(
            database.connection(),
            reason_limit,
            outcome_limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Installs (inserts) or updates (replaces by id) one bounded capability
    /// manifest against the real `capability_manifests` table.
    pub async fn save_capability_manifest(
        &self,
        record: &evohime_local_storage::capability_store::CapabilityManifestRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists installed capability manifests, newest-first.
    pub async fn list_capability_manifests(
        &self,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::capability_store::CapabilityManifestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::list(
            database.connection(),
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches one installed capability manifest by id (manifest name).
    pub async fn get_capability_manifest(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::capability_store::CapabilityManifestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::get_by_id(
            database.connection(),
            id,
        )
        .map_err(|error| error.to_string())
    }

    /// Removes one installed capability manifest by id. Returns `false` if
    /// no matching row was found.
    pub async fn remove_capability_manifest(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::delete_by_id(
            database.connection(),
            id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists (upserts by task_id) the reconciled capability-selection
    /// state for a task, so the pin/replace/auto choice survives reconnect.
    pub async fn save_capability_selection(
        &self,
        record: &evohime_local_storage::capability_selection_store::CapabilitySelectionRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_selection_store::CapabilitySelectionStoreSql::upsert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches the persisted capability-selection state for a task, if any.
    pub async fn get_capability_selection(
        &self,
        task_id: &str,
    ) -> Result<
        Option<evohime_local_storage::capability_selection_store::CapabilitySelectionRecord>,
        String,
    > {
        let database = self.database.lock().await;
        evohime_local_storage::capability_selection_store::CapabilitySelectionStoreSql::get_by_task_id(
            database.connection(),
            task_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one validated child handoff envelope.
    pub async fn save_child_handoff(
        &self,
        record: &evohime_local_storage::child_store::HandoffRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::insert_handoff(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists persisted child handoffs for a task, in sequence order.
    pub async fn list_child_handoffs(
        &self,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::child_store::HandoffRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::list_handoffs_by_task(
            database.connection(),
            task_id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one validated, read-only child task request.
    pub async fn save_child_task_request(
        &self,
        record: &evohime_local_storage::child_store::ChildTaskRequestRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::insert_child_task_request(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches one persisted child task request by its child_task_id.
    pub async fn get_child_task_request(
        &self,
        child_task_id: &str,
    ) -> Result<Option<evohime_local_storage::child_store::ChildTaskRequestRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::get_child_task_request(
            database.connection(),
            child_task_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one accepted child report.
    pub async fn save_child_report(
        &self,
        record: &evohime_local_storage::child_store::ChildReportRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::insert_child_report(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn next_child_parent_sequence(&self, parent_task_id: &str) -> Result<u64, String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::next_parent_sequence(
            database.connection(),
            parent_task_id,
        )
        .map(|value| value as u64)
        .map_err(|error| error.to_string())
    }

    pub async fn save_coordinator_checkpoint(
        &self,
        record: &evohime_local_storage::child_store::CoordinatorCheckpointRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::upsert_coordinator_checkpoint(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn get_coordinator_checkpoint(
        &self,
        child_task_id: &str,
    ) -> Result<Option<evohime_local_storage::child_store::CoordinatorCheckpointRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::latest_coordinator_checkpoint(
            database.connection(),
            child_task_id,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn list_child_dead_letters(
        &self,
        parent_task_id: &str,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::child_store::CoordinatorCheckpointRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::list_dead_letter_checkpoints(
            database.connection(),
            parent_task_id,
            now_ms,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn accept_typed_child_report(
        &self,
        request: &crate::child_contracts::TypedChildTaskRequest,
        report: &crate::child_contracts::TypedChildReport,
        now_ms: i64,
    ) -> Result<crate::child_contracts::TypedChildReport, String> {
        let database = self.database.lock().await;
        crate::child_workflow::accept_report_with_offload(
            database.connection(),
            request,
            report,
            now_ms,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn get_or_create_build_policy(
        &self,
        project_id: &str,
        default_policy: &crate::scope::BuildScope,
    ) -> Result<crate::scope::BuildScope, String> {
        let database = self.database.lock().await;
        if let Some(record) = database
            .get_project_policy(project_id)
            .map_err(|error| error.to_string())?
        {
            return serde_json::from_slice(&record.policy_json)
                .map(harden_build_policy)
                .map_err(|error| format!("invalid persisted build policy: {error}"));
        }
        let policy_json = serde_json::to_vec(default_policy).map_err(|error| error.to_string())?;
        database
            .upsert_project_policy(project_id, &policy_json, None)
            .map_err(|error| error.to_string())?;
        Ok(harden_build_policy(default_policy.clone()))
    }

    pub async fn get_build_policy(
        &self,
        project_id: &str,
        default_policy: &crate::scope::BuildScope,
    ) -> Result<(crate::scope::BuildScope, i64), String> {
        let database = self.database.lock().await;
        let record = match database
            .get_project_policy(project_id)
            .map_err(|error| error.to_string())?
        {
            Some(record) => record,
            None => {
                let policy_json =
                    serde_json::to_vec(default_policy).map_err(|error| error.to_string())?;
                database
                    .upsert_project_policy(project_id, &policy_json, None)
                    .map_err(|error| error.to_string())?
            }
        };
        let policy = serde_json::from_slice(&record.policy_json)
            .map(harden_build_policy)
            .map_err(|error| format!("invalid persisted build policy: {error}"))?;
        Ok((policy, record.version))
    }

    pub async fn save_build_policy(
        &self,
        project_id: &str,
        policy: &crate::scope::BuildScope,
        expected_version: Option<i64>,
    ) -> Result<ProjectPolicyRecord, String> {
        let policy_json = serde_json::to_vec(policy).map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        database
            .upsert_project_policy(project_id, &policy_json, expected_version)
            .map_err(|error| error.to_string())
    }

    pub async fn get_work_item(&self, id: &str) -> Result<Option<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_work_item(id)
    }

    pub async fn create_work_item(
        &self,
        item: &WorkItemRecord,
    ) -> Result<WorkItemRecord, StorageError> {
        let database = self.database.lock().await;
        database.create_work_item(item)
    }

    pub async fn update_work_item_status(
        &self,
        id: &str,
        expected_version: i64,
        status: &str,
    ) -> Result<WorkItemRecord, StorageError> {
        let database = self.database.lock().await;
        database.update_work_item_status(id, expected_version, status)
    }

    pub async fn add_dependency(
        &self,
        from_id: &str,
        to_id: &str,
        kind: &str,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        database.add_dependency(from_id, to_id, kind)
    }

    pub async fn list_work_items(
        &self,
        project_id: &str,
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.list_work_items(project_id)
    }

    pub async fn list_task_graph(
        &self,
        project_id: &str,
    ) -> Result<(Vec<WorkItemRecord>, Vec<(String, String, String)>), StorageError> {
        let database = self.database.lock().await;
        Ok((
            database.list_work_items(project_id)?,
            database.list_dependencies(project_id)?,
        ))
    }

    pub async fn next_ready_task(
        &self,
        project_id: &str,
    ) -> Result<Option<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.next_ready(project_id)
    }

    pub async fn import_prd(
        &self,
        provenance_id: &str,
        project_id: &str,
        origin: &str,
        version: &str,
        source_text: &str,
        tasks: &[ImportedTask],
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.import_prd(
            provenance_id,
            project_id,
            origin,
            version,
            source_text,
            tasks,
        )
    }

    pub async fn save_snapshot(
        &self,
        id: &str,
        run_id: &str,
        workspace_hash: &str,
        payload: &[u8],
    ) -> Result<evohime_local_storage::SnapshotRecord, StorageError> {
        let database = self.database.lock().await;
        database.save_snapshot(id, run_id, workspace_hash, payload)
    }

    pub async fn latest_snapshot_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<evohime_local_storage::SnapshotRecord>, StorageError> {
        let database = self.database.lock().await;
        database.latest_snapshot_for_task(task_id)
    }

    pub async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<evohime_local_storage::SnapshotRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_snapshot(snapshot_id)
    }

    pub async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<evohime_local_storage::RunRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_run(run_id)
    }

    pub async fn begin_build_effect(
        &self,
        run_id: &str,
        task_id: &str,
        intent_hash: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect_id = format!("effect-{run_id}");
        let checkpoint = RunCheckpointRecord {
            run_id: run_id.into(),
            checkpoint_id: format!("checkpoint-{run_id}"),
            stage: "build".into(),
            node_id: "bounded-build".into(),
            attempt: 1,
            input_hash: intent_hash.into(),
            state_json: serde_json::to_vec(&serde_json::json!({
                "stage": "build", "intent_hash": intent_hash
            }))?,
            pending_effects_json: serde_json::to_vec(&vec![effect_id.clone()])?,
            committed_at: String::new(),
        };
        let effect = RunEffectRecord {
            effect_id: effect_id.clone(),
            run_id: run_id.into(),
            node_id: "bounded-build".into(),
            kind: "bounded_build".into(),
            idempotency_key: format!("{run_id}:bounded-build"),
            immutable_intent_hash: intent_hash.into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        let run = RunRecord {
            id: run_id.into(),
            work_item_id: task_id.into(),
            status: "running".into(),
            policy_snapshot: Vec::new(),
            role_snapshot: Vec::new(),
            skill_snapshot: Vec::new(),
            model_route_snapshot: Vec::new(),
        };
        let stored = database.prepare_run_effect(&run, &checkpoint, &effect)?;
        if stored.immutable_intent_hash != intent_hash {
            return Err(StorageError::InvalidRunEffect(
                "intent hash conflict".into(),
            ));
        }
        match stored.state.as_str() {
            "prepared" => {
                database.acquire_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)?;
                database.mark_effect_executing(&effect_id)
            }
            "executing" => Err(StorageError::InvalidRunEffect(
                "effect is already executing".into(),
            )),
            "completed_success" | "completed_failure" | "unknown" => Err(
                StorageError::InvalidRunEffect(format!("effect is already {}", stored.state)),
            ),
            _ => Err(StorageError::InvalidRunEffect(format!(
                "unsupported state {}",
                stored.state
            ))),
        }
    }

    pub async fn complete_build_effect(
        &self,
        run_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect =
            database.complete_run_effect(&format!("effect-{run_id}"), success, result_hash)?;
        database.update_run_status(run_id, if success { "completed" } else { "failed" })?;
        database.release_run_lease(run_id, &format!("lease-{run_id}"), "core", 1)?;
        Ok(effect)
    }

    pub async fn heartbeat_build_effect(
        &self,
        run_id: &str,
    ) -> Result<evohime_local_storage::RunLeaseRecord, StorageError> {
        let database = self.database.lock().await;
        database.heartbeat_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)
    }

    pub async fn begin_agent_run(
        &self,
        run_id: &str,
        task_id: &str,
        intent_hash: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect_id = format!("effect-{run_id}");
        let effect = RunEffectRecord {
            effect_id: effect_id.clone(),
            run_id: run_id.into(),
            node_id: "agent-task".into(),
            kind: "agent_task".into(),
            idempotency_key: format!("{run_id}:agent-task"),
            immutable_intent_hash: intent_hash.into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        let stored = database.prepare_agent_run_effect(&effect, task_id)?;
        if stored.immutable_intent_hash != intent_hash {
            return Err(StorageError::InvalidRunEffect(
                "intent hash conflict".into(),
            ));
        }
        match stored.state.as_str() {
            "prepared" => {
                database.acquire_agent_run_lease(
                    run_id,
                    &format!("lease-{run_id}"),
                    "core",
                    1,
                    30,
                )?;
                database.mark_agent_effect_executing(&effect_id)
            }
            "executing" => Err(StorageError::InvalidRunEffect(
                "effect is already executing".into(),
            )),
            "completed_success" | "completed_failure" | "unknown" => Err(
                StorageError::InvalidRunEffect(format!("effect is already {}", stored.state)),
            ),
            _ => Err(StorageError::InvalidRunEffect(format!(
                "unsupported state {}",
                stored.state
            ))),
        }
    }

    pub async fn heartbeat_agent_run(
        &self,
        run_id: &str,
    ) -> Result<evohime_local_storage::RunLeaseRecord, StorageError> {
        let database = self.database.lock().await;
        database.heartbeat_agent_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)
    }

    pub async fn complete_agent_run(
        &self,
        run_id: &str,
        success: bool,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect =
            database.complete_agent_run_effect(&format!("effect-{run_id}"), success, None)?;
        database.release_agent_run_lease(run_id, &format!("lease-{run_id}"), "core", 1)?;
        Ok(effect)
    }

    pub async fn reconcile_build_effect(
        &self,
        run_id: &str,
        success: bool,
        evidence: &serde_json::Value,
    ) -> Result<evohime_local_storage::RunReconciliationRecord, StorageError> {
        let database = self.database.lock().await;
        let record = database.reconcile_run_effect(
            &format!("effect-{run_id}"),
            success,
            "bounded_build_snapshot",
            &serde_json::to_vec(evidence)?,
        )?;
        if success {
            database.update_run_status(run_id, "completed")?;
        }
        Ok(record)
    }

    pub async fn recover_after_restart(
        &self,
    ) -> Result<Vec<evohime_local_storage::RecoveredRunRecord>, StorageError> {
        let database = self.database.lock().await;
        database.recover_unknown_effects()
    }

    pub async fn recover_and_reconcile_after_restart(
        &self,
    ) -> Result<Vec<evohime_local_storage::RunReconciliationRecord>, StorageError> {
        let database = self.database.lock().await;
        let recovered = database.recover_unknown_effects()?;
        let mut reconciliations = Vec::with_capacity(recovered.len());
        for record in recovered {
            // Durable recovery state machine: RECOVERING -> RECONCILING -> terminal.
            // Each stage uses a distinct idempotency key so a crash between
            // stages replays safely (transition_recovery treats a repeated
            // (idempotency_key, state) pair as a no-op and rejects a reused
            // key against a different state).
            let recovery_transition =
                |state, idempotency_key: &str, verifier: &str, evidence: &[u8], decision: &str| {
                    if record.kind == "agent_task" {
                        database.transition_agent_recovery(
                            evohime_local_storage::RecoveryTransitionInput {
                                run_id: &record.run_id,
                                next: state,
                                effect_id: &record.effect_id,
                                idempotency_key,
                                verifier,
                                evidence_json: evidence,
                                decision,
                            },
                        )
                    } else {
                        database.transition_recovery(
                            evohime_local_storage::RecoveryTransitionInput {
                                run_id: &record.run_id,
                                next: state,
                                effect_id: &record.effect_id,
                                idempotency_key,
                                verifier,
                                evidence_json: evidence,
                                decision,
                            },
                        )
                    }
                };
            recovery_transition(
                RecoveryState::Recovering,
                &format!("{}:{}:recovering", record.run_id, record.effect_id),
                "startup",
                br#"{"reason":"process_restart"}"#,
                "recovery_started",
            )?;
            recovery_transition(
                RecoveryState::Reconciling,
                &format!("{}:{}:reconciling", record.run_id, record.effect_id),
                if record.kind == "agent_task" {
                    "task_event_journal"
                } else {
                    "bounded_build_snapshot"
                },
                br#"{"reason":"verifying_outcome"}"#,
                "verifier_started",
            )?;

            let (success, verifier, idempotency_key, evidence) = if record.kind == "agent_task" {
                let terminal_event = database
                    .read_task_events(&record.work_item_id, 256)?
                    .into_iter()
                    .rev()
                    .find(|event| {
                        matches!(
                            event.event_type.as_str(),
                            "task.completed" | "task.failed" | "task.stopped"
                        )
                    });
                let success = terminal_event
                    .as_ref()
                    .is_some_and(|event| event.event_type == "task.completed");
                let verifier = "task_event_journal";
                let idempotency_key = format!("{}:agent-task", record.run_id);
                let evidence = serde_json::json!({
                    "run_id": record.run_id,
                    "effect_id": record.effect_id,
                    "idempotency_key": idempotency_key,
                    "verifier": verifier,
                    "terminal_event": terminal_event.as_ref().map(|event| serde_json::json!({
                        "event_type": event.event_type,
                        "sequence_id": event.sequence_id,
                    })),
                    "decision": if success { "completed" } else { "blocked" },
                });
                (success, verifier, idempotency_key, evidence)
            } else {
                let snapshot = database.latest_snapshot_for_task(&record.work_item_id)?;
                let success = snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.run_id == record.run_id);
                let verifier = "bounded_build_snapshot";
                let idempotency_key = format!("{}:bounded-build", record.run_id);
                let evidence = serde_json::json!({
                    "run_id": record.run_id,
                    "effect_id": record.effect_id,
                    "idempotency_key": idempotency_key,
                    "verifier": verifier,
                    "snapshot_id": success.then(|| snapshot.as_ref().expect("successful reconciliation has snapshot").id.clone()),
                    "decision": if success { "applied" } else { "blocked" },
                });
                (success, verifier, idempotency_key, evidence)
            };
            let reconciliation = if record.kind == "agent_task" {
                database.reconcile_agent_run_effect(
                    &record.effect_id,
                    success,
                    verifier,
                    &serde_json::to_vec(&evidence)?,
                )?
            } else {
                database.reconcile_run_effect(
                    &record.effect_id,
                    success,
                    verifier,
                    &serde_json::to_vec(&evidence)?,
                )?
            };
            if success {
                database.update_run_status(&record.run_id, "completed")?;
            }
            database.append_event(
                &record.work_item_id,
                if success {
                    "run.reconciliation.completed"
                } else {
                    "run.recovery.blocked"
                },
                &serde_json::to_vec(&evidence)?,
            )?;
            database.append_event(
                &record.work_item_id,
                "run.reconciliation.audit",
                &serde_json::to_vec(&serde_json::json!({
                    "effect_id": record.effect_id,
                    "idempotency_key": idempotency_key,
                    "verifier": verifier,
                    "evidence": evidence,
                    "decision": if success { "applied" } else { "blocked" },
                }))?,
            )?;

            recovery_transition(
                if success {
                    RecoveryState::Resumable
                } else {
                    RecoveryState::Blocked
                },
                &format!(
                    "{}:{}:{}",
                    record.run_id,
                    record.effect_id,
                    if success { "resumable" } else { "blocked" }
                ),
                verifier,
                &serde_json::to_vec(&evidence)?,
                if success { "applied" } else { "blocked" },
            )?;

            reconciliations.push(reconciliation);
        }
        Ok(reconciliations)
    }

    pub async fn record_audit(
        &self,
        subject_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, StorageError> {
        let database = self.database.lock().await;
        database.append_event(subject_id, event_type, payload)
    }

    pub async fn task_history(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_task_events(task_id, limit)
    }

    pub async fn record_deduplicated(
        &self,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
        result: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let database = self.database.lock().await;
        database.record_deduplicated(client_id, request_id, command_hash, result)
    }

    /// Atomically records a TaskCheckpoint user action and its idempotency
    /// result. The event and dedup row must commit together: otherwise a
    /// reconnect between the two writes could either repeat the action or
    /// report a success that is absent from the journal.
    pub async fn record_task_checkpoint_action(
        &self,
        task_id: &str,
        request_id: &str,
        command_hash: &str,
        event_payload: &[u8],
        result: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let mut database = self.database.lock().await;
        let transaction = database.connection_mut().transaction()?;
        let existing = transaction
            .query_row(
                "SELECT command_hash, result FROM command_dedup
                 WHERE client_id = 'task-checkpoint-ipc' AND request_id = ?1",
                [request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        if let Some((stored_hash, stored_result)) = existing {
            if stored_hash == command_hash {
                transaction.commit()?;
                return Ok(Some(stored_result));
            }
            return Err(StorageError::DeduplicationConflict {
                client_id: "task-checkpoint-ipc".into(),
                request_id: request_id.into(),
            });
        }
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload)
             VALUES (?1, 'task.checkpoint.action', ?2)",
            rusqlite::params![task_id, event_payload],
        )?;
        transaction.execute(
            "INSERT INTO command_dedup(client_id, request_id, command_hash, result)
             VALUES ('task-checkpoint-ipc', ?1, ?2, ?3)",
            rusqlite::params![request_id, command_hash, result],
        )?;
        transaction.commit()?;
        Ok(None)
    }
}
