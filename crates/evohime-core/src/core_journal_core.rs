use super::*;

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
    ) -> Result<Vec<(String, evohime_local_storage::domains::audit::ActionState)>, StorageError>
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
            evohime_local_storage::domains::runs::list_running_runs(database.connection())?;
        let mut recovered = 0;
        for run in runs {
            if evohime_local_storage::domains::runs::transition_run(
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
            evohime_local_storage::domains::agents::RetainedChildStore::reconcile_all_unknown(
                database.connection(),
            )
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let expired = evohime_local_storage::domains::agents::RetainedChildStore::expire_due(
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
        let repository = evohime_local_storage::domains::receipts::ModelProvenanceRepository::new(
            database.connection(),
        );
        for compression in &ledger.compression {
            for original_id in &compression.source_ids {
                let shadow_id = format!("{request_id}:summary:{original_id}");
                repository
                    .append_shadow_original(
                        &evohime_local_storage::domains::receipts::ShadowOriginalRecord {
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
                    &evohime_local_storage::domains::receipts::ShadowOriginalRecord {
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
}
