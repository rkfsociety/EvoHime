use super::*;

impl EventJournal {
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
            evohime_local_storage::domains::audit::MessageAcceptance,
            i64,
        ),
        StorageError,
    > {
        use sha2::{Digest, Sha256};

        let draft = crate::conversation_event_log::user_message_draft(content)
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
        let database = self.database.lock().await;
        let acceptance = evohime_local_storage::domains::audit::accept_message(
            database.connection(),
            evohime_local_storage::domains::audit::AcceptMessageInput {
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
        evohime_local_storage::domains::audit::ConversationEventPage,
        StorageError,
    > {
        let database = self.database.lock().await;
        Ok(
            evohime_local_storage::domains::audit::history_after(
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
            evohime_local_storage::domains::audit::task_binding(
                database.connection(),
                task_id,
            )?
        else {
            return Ok(());
        };
        for draft in drafts {
            let stored = evohime_local_storage::domains::audit::append_event(
                database.connection(),
                evohime_local_storage::domains::audit::NewConversationEvent {
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
            evohime_local_storage::domains::audit::claim_message_dispatch(
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
            evohime_local_storage::domains::audit::finish_message_dispatch(
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
        evohime_local_storage::domains::audit::ConversationEventPage,
        StorageError,
    > {
        let database = self.database.lock().await;
        Ok(
            evohime_local_storage::domains::audit::history_before(
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
            evohime_local_storage::domains::workflow::ArtifactStore::new(database.connection());
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
            evohime_local_storage::domains::workflow::ArtifactStore::new(database.connection());
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
}
