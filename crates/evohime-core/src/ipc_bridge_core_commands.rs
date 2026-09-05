impl IpcBridge {
    pub fn journal(&self) -> EventJournal {
        self.journal.clone()
    }

    /// Identity this process picked at construction (план 08-2/08-3
    /// `core_instance_id`) — used to publish the `core_start` ledger event
    /// under the exact id this bridge will stamp on every `EventEnvelope`.
    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
    }

    /// True when the client's own `CommandEnvelope` names a generation
    /// (`core_instance_id`/`session_epoch`) other than this process's
    /// current one. An empty/zero client field never counts as stale — it
    /// means the client has no known generation yet (first connect).
    pub(crate) fn stale_generation(&self, command: &generated::CommandEnvelope) -> bool {
        (!command.core_instance_id.is_empty() && command.core_instance_id != self.core_instance_id)
            || (command.session_epoch > 0 && command.session_epoch != self.session_epoch)
    }

    /// Builds a typed `ReplayGap` envelope (план 08-3): honestly filled
    /// bounds instead of the generic JSON `"reason"` field this used to be.
    pub(crate) fn replay_gap_envelope(
        &self,
        requested_after_sequence: u64,
        earliest_available_sequence: Option<u64>,
        latest_available_sequence: u64,
        reason: &str,
    ) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: latest_available_sequence,
            task_id: String::new(),
            event_type: "replay.gap".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ReplayGap(
                generated::ReplayGap {
                    requested_after_sequence,
                    earliest_available_sequence: earliest_available_sequence.unwrap_or(0),
                    latest_available_sequence,
                    reason: reason.to_string(),
                },
            )),
        }
    }

    /// Publishes a typed `ApprovalDecision` ledger event for a resolved
    /// approval, when it is linked to a receipts-tracked action (план 08-4
    /// acceptance: "approval approve/reject/expiry"). Cancellation already
    /// collapses into `granted = false` at the call site — a cancelled
    /// approval and a denied one both land as `Rejected` here, matching the
    /// existing `approval.decision` audit record's own `granted` field.
    /// A no-op when `approval_id` isn't a receipts approval intent (e.g. a
    /// pure workflow-node or routing approval) — those aren't
    /// receipts-tracked actions and get no `ExecutionEventV1` here.
    pub(crate) async fn record_ledger_approval_decision(&self, approval_id: &str, granted: bool) {
        let database = self.journal.database().lock().await;
        let linked: Option<(String, String, String)> = database
            .connection()
            .query_row(
                "SELECT action_id, task_id, run_id FROM receipt_approval_intents
                   WHERE approval_id = ?1",
                [approval_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap_or(None);
        let Some((action_id, task_id, run_id)) = linked else {
            return;
        };
        let state_after = if granted {
            execution_ledger::ActionState::Running
        } else {
            execution_ledger::ActionState::Denied
        };
        let event = execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: uuid::Uuid::now_v7().to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id,
            session_id: Some(task_id.clone()),
            task_id,
            created_at_ms: now_ms(),
            state_after: Some(state_after),
            action_id: Some(action_id),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id: approval_id.to_string(),
                decision: if granted {
                    execution_ledger::ApprovalOutcome::Approved
                } else {
                    execution_ledger::ApprovalOutcome::Rejected
                },
                snapshot_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        };
        if let Err(error) = database.append_ledger_event(&event) {
            tracing::warn!(
                event = "ledger.approval_decision_publish_failed",
                approval_id,
                error = %error,
                "typed ledger event failed to publish"
            );
        }
    }

    pub(crate) fn manager_for(journal: &EventJournal) -> Arc<ReceiptKeyManager> {
        let data_dir = journal
            .database_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        Arc::new(ReceiptKeyManager::new(data_dir))
    }
    /// Записывает лимиты каталога в локальную базу. Это подсказка для
    /// планировщика контекста, а не условие работы: провайдер может не сообщить
    /// окно, а база — быть занята другим писателем, и ни то, ни другое не повод
    /// проваливать запрос каталога.
    pub(crate) async fn remember_model_limits(
        &self,
        provider: &str,
        entries: &[evohime_model_gateway::ModelCatalogEntry],
    ) {
        if entries.is_empty() {
            return;
        }
        let records = entries
            .iter()
            .map(
                |entry| evohime_local_storage::model_limit_store::ModelLimitRecord {
                    model: entry.id.clone(),
                    provider: provider.to_string(),
                    context_tokens: entry.context_tokens,
                    max_output_tokens: entry.max_output_tokens,
                },
            )
            .collect::<Vec<_>>();
        let database = self.journal.database().lock().await;
        if let Err(error) = evohime_local_storage::model_limit_store::ModelLimitStoreSql::upsert_all(
            database.connection(),
            &records,
        ) {
            tracing::warn!(target: "model.catalog", %error, "model context limits were not stored");
        }
    }

    pub fn new(journal: EventJournal) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: None,
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            tool_simulation: Arc::new(tokio::sync::Mutex::new(
                crate::tool_simulation_runtime::ToolSimulationRuntime::default(),
            )),
            external_agents: Arc::new(tokio::sync::Mutex::new(Default::default())),
            role_profiles: Arc::new(tokio::sync::Mutex::new(Default::default())),
            conversation_subscription: Arc::new(tokio::sync::Mutex::new(None)),
            team_sop: Arc::new(tokio::sync::Mutex::new(Default::default())),
            human_work_items: Arc::new(tokio::sync::Mutex::new(Default::default())),
            browser_backends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            tool_simulation: Arc::new(tokio::sync::Mutex::new(
                crate::tool_simulation_runtime::ToolSimulationRuntime::default(),
            )),
            external_agents: Arc::new(tokio::sync::Mutex::new(Default::default())),
            role_profiles: Arc::new(tokio::sync::Mutex::new(Default::default())),
            conversation_subscription: Arc::new(tokio::sync::Mutex::new(None)),
            team_sop: Arc::new(tokio::sync::Mutex::new(Default::default())),
            human_work_items: Arc::new(tokio::sync::Mutex::new(Default::default())),
            browser_backends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn with_coordinator_and_approvals(
        journal: EventJournal,
        coordinator: TaskCoordinator,
        approvals: ApprovalCoordinator,
        tools: Arc<ToolRegistry>,
        model_config: Option<ModelConfigSnapshot>,
        gateway_config: Option<ModelGatewayConfig>,
    ) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: Some(approvals),
            tools: Some(tools),
            model_config,
            gateway_config,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            tool_simulation: Arc::new(tokio::sync::Mutex::new(
                crate::tool_simulation_runtime::ToolSimulationRuntime::default(),
            )),
            external_agents: Arc::new(tokio::sync::Mutex::new(Default::default())),
            role_profiles: Arc::new(tokio::sync::Mutex::new(Default::default())),
            conversation_subscription: Arc::new(tokio::sync::Mutex::new(None)),
            team_sop: Arc::new(tokio::sync::Mutex::new(Default::default())),
            human_work_items: Arc::new(tokio::sync::Mutex::new(Default::default())),
            browser_backends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Разделяемый реестр состояния слушания.
    pub fn ambient(&self) -> crate::ambient::AmbientListeningRegistry {
        self.ambient.clone()
    }

    pub fn voice_commands(&self) -> Arc<crate::voice_command::VoiceCommandRegistry> {
        self.voice_commands.clone()
    }

    /// Подключает готовый реестр: `main.rs` создаёт его до моста, чтобы
    /// endpoint листенера и мост говорили об одном и том же состоянии.
    pub fn with_ambient(mut self, ambient: crate::ambient::AmbientListeningRegistry) -> Self {
        self.ambient = ambient;
        self
    }

    /// Каталог политики и намерения слушания.
    pub fn with_ambient_data_dir(mut self, directory: std::path::PathBuf) -> Self {
        self.ambient_data_dir = Some(directory);
        self
    }

    /// Разделяемый реестр проактивности.
    pub fn proactivity(&self) -> crate::ambient::AmbientProactivityRegistry {
        self.proactivity.clone()
    }

    /// Подключает готовый реестр проактивности: `main.rs` создаёт его до
    /// агента и до моста, чтобы обе стороны считали один и тот же потолок.
    pub fn with_proactivity(
        mut self,
        proactivity: crate::ambient::AmbientProactivityRegistry,
    ) -> Self {
        self.proactivity = proactivity;
        self
    }

    pub(crate) fn ambient_data_dir(&self) -> std::path::PathBuf {
        self.ambient_data_dir
            .clone()
            .unwrap_or_else(crate::ambient::data_dir)
    }

    /// Пишет ambient-событие в durable journal и будит push к оболочке.
    ///
    /// Без второго шага запись легла бы в базу, но открытое окно узнало бы о
    /// ней только со следующим событием задачи.
    pub async fn publish_ambient(
        &self,
        event: &evohime_listener_contract::AmbientLogEvent,
    ) -> Result<i64, evohime_listener_contract::AmbientErrorCode> {
        let sequence = self.journal.append_ambient_event(event).await?;
        if let Some(coordinator) = &self.coordinator {
            coordinator.notify_journalled(sequence.max(0) as u64);
        }
        Ok(sequence)
    }

    /// Отдаёт закрытый эпизод в ambient-извлечение (04.6).
    ///
    /// Мост здесь только курьер: решают `EVOHIME_AMBIENT_MEMORY`, общий режим
    /// извлечения и ambient-бюджеты, и все три проверяются в Core, а не тут.
    /// Без координатора вызов молча ничего не делает — извлекателя в этой
    /// сборке просто нет.
    pub async fn request_ambient_extraction(&self, episode_id: &str) {
        let Some(coordinator) = &self.coordinator else {
            return;
        };
        let _ = coordinator
            .dispatch(CoreCommand::ExtractAmbientMemory {
                episode_id: episode_id.to_owned(),
            })
            .await;
    }

}
