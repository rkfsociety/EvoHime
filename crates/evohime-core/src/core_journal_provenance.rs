use super::*;

impl EventJournal {
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
}
