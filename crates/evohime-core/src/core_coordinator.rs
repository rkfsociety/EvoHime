use super::*;

#[derive(Clone)]
pub struct TaskCoordinator {
    commands: mpsc::Sender<CoreCommand>,
    state: Arc<Mutex<CoordinatorState>>,
    journalled: tokio::sync::watch::Receiver<u64>,
    /// Тот же канал, по которому координатор сообщает о записанном событии.
    ///
    /// Нужен производителям, которые пишут в журнал напрямую (ambient-путь):
    /// pipe-сервер сбрасывает хвост журнала только по этому сигналу, и без
    /// него запись легла бы в базу, но не дошла бы до открытого окна.
    journalled_tx: Arc<tokio::sync::watch::Sender<u64>>,
}

pub(crate) struct CoordinatorState {
    command_tx: mpsc::Sender<CoreCommand>,
    marker_gate: crate::code_anchored_intent_markers::MarkerGate,
    tasks: HashMap<String, ActiveTask>,
    workspace_index_cancellations: HashMap<String, CancellationToken>,
    backup_cancellations: HashMap<String, CancellationToken>,
    backup_approvals: HashMap<String, String>,
    routing_decisions: HashMap<String, bool>,
    routing_approvals: RoutingApprovalRegistry,
    events: EventSink,
    notifications: broadcast::Sender<CoreEvent>,
    executor: Option<Arc<dyn TaskExecutor>>,
    journal: Option<EventJournal>,
    audit: crate::audit::AuditTrail,
    retained_children: crate::retained_child::RetainedRegistry,
    background_tasks: Arc<crate::bounded_tasks::BoundedTaskGroup>,
    host_telemetry: crate::host_resource_telemetry::HostTelemetryService,
    persistence_error: Option<String>,
}

struct ActiveTask {
    cancellation: CancellationToken,
}

#[path = "core_coordinator_build.rs"]
mod build;
#[path = "core_coordinator_build_runtime.rs"]
mod build_runtime;
#[path = "core_coordinator_capabilities.rs"]
mod capabilities;
#[path = "core_coordinator_capabilities_runtime.rs"]
mod capabilities_runtime;
#[path = "core_coordinator_context_namespace.rs"]
mod context_namespace;
#[path = "core_coordinator_durable_background_execution.rs"]
mod durable_background_execution;
#[path = "core_coordinator_memory.rs"]
mod memory;
#[path = "core_coordinator_memory_runtime.rs"]
mod memory_runtime;
#[path = "core_coordinator_models.rs"]
mod models;
#[path = "core_coordinator_models_observability.rs"]
mod models_observability;
#[path = "core_coordinator_models_runtime.rs"]
mod models_runtime;
#[path = "core_coordinator_models_tail.rs"]
mod models_tail;
#[path = "core_coordinator_orchestration.rs"]
mod orchestration;
#[path = "core_coordinator_orchestration_runtime.rs"]
mod orchestration_runtime;
#[path = "core_coordinator_routing.rs"]
mod routing;
#[path = "core_coordinator_tasks.rs"]
mod tasks;
#[path = "core_coordinator_workflow.rs"]
mod workflow;
#[path = "core_coordinator_workflow_capabilities.rs"]
mod workflow_capabilities;
#[path = "core_coordinator_workflow_runtime.rs"]
mod workflow_runtime;
#[path = "core_coordinator_workflow_subsystems.rs"]
mod workflow_subsystems;
#[path = "core_coordinator_workspace.rs"]
mod workspace;
#[path = "core_coordinator_workspace_children.rs"]
mod workspace_children;
#[path = "core_coordinator_workspace_context.rs"]
mod workspace_context;

impl TaskCoordinator {
    pub fn new(buffer: usize) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, None, None)
    }

    /// Additional listener on the same event stream. Used by the pipe server to
    /// know when to flush the journal tail to a connected shell.
    pub async fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.state.lock().await.notifications.subscribe()
    }

    /// Fires after an event is durably recorded, carrying its sequence. The
    /// pipe server flushes the journal tail on this, so a shell never has to
    /// wait for the next event to see the previous one.
    pub fn journalled(&self) -> tokio::sync::watch::Receiver<u64> {
        self.journalled.clone()
    }

    /// Publishes an event produced outside the task executor.
    ///
    /// Recording straight into the journal is not enough: the pipe server
    /// flushes its tail only on the `journalled` signal, which the coordinator
    /// raises after it records an event taken from this broadcast. A producer
    /// that bypasses the broadcast lands in the database but never reaches a
    /// connected shell.
    pub async fn emit(&self, event: CoreEvent) {
        let events = self.state.lock().await.events.clone();
        let _ = events.send(event).await;
    }

    pub(crate) async fn emit_state_event(state: &Arc<Mutex<CoordinatorState>>, event: CoreEvent) {
        let events = state.lock().await.events.clone();
        let _ = events.send(event).await;
    }

    /// Сообщает, что в журнал легла запись, минуя broadcast координатора.
    ///
    /// Ambient-события пишутся прямо в журнал: у них нет варианта `CoreEvent`
    /// и не должно быть — иначе текстовые поля `CoreEvent` стали бы для них
    /// доступны. Сигнал остаётся общим, поэтому оболочка получает их так же
    /// быстро, как события задач.
    pub fn notify_journalled(&self, sequence: u64) {
        let _ = self.journalled_tx.send(sequence);
    }

    pub async fn attach_routing_approvals(&self, approvals: RoutingApprovalRegistry) {
        self.state.lock().await.routing_approvals = approvals;
    }

    /// Records a validated host snapshot in the Core-owned bounded ring.
    /// Collectors remain adapters and cannot publish a fabricated pressure
    /// value directly to the renderer or scheduler.
    pub async fn record_host_resource_snapshot(
        &self,
        snapshot: crate::host_resource_telemetry::HostResourceSnapshot,
        now_ms: i64,
    ) -> Result<crate::host_resource_telemetry::PressureLevel, crate::host_resource_telemetry::TelemetryError> {
        self.state.lock().await.host_telemetry.record(snapshot, now_ms)
    }

    pub fn new_with_executor(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, None)
    }

    pub fn new_with_journal(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: EventJournal,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, Some(journal))
    }

    pub(crate) fn build(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: Option<EventJournal>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        let (commands, mut command_rx) = mpsc::channel(buffer.max(1));
        let (notifications, notification_rx) = broadcast::channel(buffer.max(1));
        let (event_tx, mut event_rx) = mpsc::channel(buffer.max(1));
        let events = EventSink::new(event_tx);
        let background_tasks = Arc::new(crate::bounded_tasks::BoundedTaskGroup::new(
            crate::bounded_tasks::DEFAULT_CAPACITY,
        ));
        let state = Arc::new(Mutex::new(CoordinatorState {
            command_tx: commands.clone(),
            marker_gate: crate::code_anchored_intent_markers::MarkerGate::default(),
            tasks: HashMap::new(),
            workspace_index_cancellations: HashMap::new(),
            backup_cancellations: HashMap::new(),
            backup_approvals: HashMap::new(),
            routing_decisions: HashMap::new(),
            routing_approvals: RoutingApprovalRegistry::default(),
            events: events.clone(),
            notifications: notifications.clone(),
            executor,
            journal: journal.clone(),
            audit: crate::audit::AuditTrail::default(),
            retained_children: crate::retained_child::RetainedRegistry::default(),
            background_tasks,
            host_telemetry: crate::host_resource_telemetry::HostTelemetryService::new(crate::host_resource_telemetry::PressurePolicy::default()),
            persistence_error: None,
        }));
        // The shell is fed from the journal, so it must be told after a record
        // lands — not when the event was broadcast. Watching the broadcast
        // directly raced the writer and left the last event of a task unsent.
        let (journalled, journalled_rx) = tokio::sync::watch::channel(0_u64);
        let journalled = Arc::new(journalled);
        let audit_state = Arc::clone(&state);
        let journal_state = Arc::clone(&state);
        let notifications = notifications.clone();
        let journalled_for_worker = Arc::clone(&journalled);
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Some(journal) = journal_state.lock().await.journal.clone() {
                    match journal.record(&event).await {
                        Ok(sequence) => {
                            let _ = journalled_for_worker.send(sequence.max(0) as u64);
                        }
                        Err(error) => {
                            Self::report_persistence_error(
                                &journal_state,
                                "journal",
                                error.to_string(),
                            )
                            .await;
                        }
                    }
                }
                Self::record_audit_for_event(&audit_state, &event).await;
                let _ = notifications.send(event);
            }
        });
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::handle_command(Arc::clone(&worker_state), command).await;
            }
        });
        (
            Self {
                commands,
                state,
                journalled: journalled_rx,
                journalled_tx: journalled,
            },
            notification_rx,
        )
    }

    async fn report_persistence_error(
        state: &Arc<Mutex<CoordinatorState>>,
        source: &str,
        error: String,
    ) {
        let message = format!("{source}: {error}");
        let notifications = {
            let mut state_guard = state.lock().await;
            state_guard.persistence_error = Some(message);
            state_guard.notifications.clone()
        };
        let _ = notifications.send(CoreEvent::EventPersistenceFailed {
            source: source.to_owned(),
            error,
        });
    }

    /// Возвращает последнюю ошибку обязательной записи событий.
    pub async fn persistence_error(&self) -> Option<String> {
        self.state.lock().await.persistence_error.clone()
    }

    #[cfg(test)]
    pub(crate) async fn record_test_audit_failure(&self) {
        Self::record_audit(
            &self.state,
            crate::audit::AuditKind::Failure,
            "",
            "invalid",
            [],
        )
        .await;
    }

    // `SendError` по контракту tokio возвращает вызывающему саму неотправленную
    // команду, поэтому размер Err-варианта здесь неизбежен и боксировать его нельзя
    // без слома API диспетчеризации.
    #[allow(clippy::result_large_err)]
    pub async fn dispatch(
        &self,
        command: CoreCommand,
    ) -> Result<(), mpsc::error::SendError<CoreCommand>> {
        self.commands.send(command).await
    }

    /// Appends a bounded, durable audit record. Failures to append (bounds
    /// exceeded, invalid fields) are non-fatal to the caller: audit logging
    /// must never block or fail a live command.
    pub(super) async fn record_audit(
        state: &Arc<Mutex<CoordinatorState>>,
        kind: crate::audit::AuditKind,
        actor: impl Into<String>,
        event_id: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let mut state_guard = state.lock().await;
        let sequence = state_guard.audit.records().len() as u64;
        let record = match crate::audit::AuditRecord::new(sequence, event_id, kind, actor, fields) {
            Ok(record) => record,
            Err(error) => {
                drop(state_guard);
                Self::report_persistence_error(state, "audit", error.to_string()).await;
                return;
            }
        };
        let line = match record.to_json_line() {
            Ok(line) => line,
            Err(error) => {
                drop(state_guard);
                Self::report_persistence_error(state, "audit", error.to_string()).await;
                return;
            }
        };
        if let Err(error) = state_guard.audit.append(record) {
            drop(state_guard);
            Self::report_persistence_error(state, "audit", error.to_string()).await;
            return;
        }
        drop(state_guard);
        append_audit_line(&line);
    }

    /// Shared confirm/reject path. Both are approval-gated, batched and
    /// idempotent: each id reports the state the store actually holds after
    /// the call, so a replayed request produces the same answer instead of a
    /// second transition. Concurrent actions on one id are serialized by the
    /// storage transaction inside `transition_memory_state`.
    pub(super) async fn apply_memory_decision(
        state: &Arc<Mutex<CoordinatorState>>,
        ids: Vec<String>,
        approval_id: String,
        idempotency_key: String,
        operation: crate::memory_api::MemoryOperation,
        target: crate::memory_extraction::ConfirmationState,
        audit_event: &str,
    ) -> Result<Vec<u8>, String> {
        let journal = state.lock().await.journal.clone();
        let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
        crate::memory_api::Approval::new(approval_id.clone(), operation)
            .map_err(|error| error.to_string())?;
        validate_memory_idempotency_key(&idempotency_key)?;
        if ids.is_empty() {
            return Err("at least one memory id is required".to_string());
        }
        if ids.len() > MAX_MEMORY_BATCH {
            return Err(format!("batch is limited to {MAX_MEMORY_BATCH} memory ids"));
        }
        let mut results = Vec::with_capacity(ids.len());
        for id in &ids {
            // A contradictory decision on one id (rejecting an already
            // confirmed record, say) reports that id's real state instead of
            // aborting the rest of the batch.
            let actual = match journal.transition_memory_state(id, target.as_str()).await {
                Ok(state) => state,
                Err(error) => {
                    let current = journal
                        .get_memory(id)
                        .await
                        .ok()
                        .flatten()
                        .map(|record| record.extraction.confirmation_state);
                    match current {
                        Some(state) => state,
                        // No such record at all: that is a real failure.
                        None => return Err(error),
                    }
                }
            };
            results.push(serde_json::json!({
                "id": id,
                "state": actual,
                "applied": actual == target.as_str(),
            }));
            Self::record_audit(
                state,
                crate::audit::AuditKind::Approval,
                id.clone(),
                audit_event,
                [
                    ("memory_id".to_owned(), id.clone()),
                    ("state".to_owned(), actual),
                    ("approval_id".to_owned(), approval_id.clone()),
                    ("idempotency_key".to_owned(), idempotency_key.clone()),
                ],
            )
            .await;
        }
        serde_json::to_vec(&serde_json::json!({ "results": results }))
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn record_audit_for_event(
        state: &Arc<Mutex<CoordinatorState>>,
        event: &CoreEvent,
    ) {
        match event {
            CoreEvent::ApprovalRequired {
                task_id,
                approval_id,
                tool_name,
                permission,
                scope,
                ..
            } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::Approval,
                    task_id.to_string(),
                    "approval.required",
                    [
                        ("approval_id".to_owned(), approval_id.to_string()),
                        ("tool_name".to_owned(), tool_name.to_string()),
                        ("permission".to_owned(), permission.to_string()),
                        ("scope".to_owned(), scope.to_string()),
                    ],
                )
                .await;
            }
            CoreEvent::ToolStarted { task_id, tool_name } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::ToolCall,
                    task_id.to_string(),
                    "tool.started",
                    [("tool_name".to_owned(), tool_name.to_string())],
                )
                .await;
            }
            CoreEvent::TaskFailed { task_id, error } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::Failure,
                    task_id.to_string(),
                    "task.failed",
                    [("error".to_owned(), error.to_string())],
                )
                .await;
            }
            _ => {}
        }
    }

    /// Returns the current in-memory audit trail as JSONL, primarily for
    /// tests and diagnostics. The durable copy lives on disk at
    /// `<data_dir>/logs/audit.jsonl`.
    pub async fn audit_jsonl(&self) -> String {
        self.state.lock().await.audit.as_jsonl().unwrap_or_default()
    }

    /// Returns a snapshot of the current in-memory audit records, primarily
    /// for tests and diagnostics.
    pub async fn audit_records(&self) -> Vec<crate::audit::AuditRecord> {
        self.state.lock().await.audit.records().to_vec()
    }

    pub(crate) async fn handle_command(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
        match command {
            c @ CoreCommand::StartTask { .. } => routing::handle(state, c).await,
            c @ CoreCommand::ResolveRoutingDecision { .. } => routing::handle(state, c).await,
            c @ CoreCommand::ExtractAmbientMemory { .. } => routing::handle(state, c).await,
            c @ CoreCommand::StopTask { .. } => routing::handle(state, c).await,
            c @ CoreCommand::CreateProject { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::CreateTask { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::UpdateTaskStatus { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::AddTaskEdge { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::GetTaskGraph { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::NextReadyTask { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::ImportPrd { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::GetTaskHistory { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::GetTaskContext { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::GetTaskPlanSpec { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::PlanArtifact { .. } => tasks::handle(state, c).await,
            c @ CoreCommand::WorkspaceStateCheckpoint { .. } => workflow::handle(state, c).await,
            c @ CoreCommand::IncrementalChangeProtocol { .. } => workflow::handle(state, c).await,
            c @ CoreCommand::RevisionSafeWorkspaceFiles { .. } => workflow::handle(state, c).await,
            c @ CoreCommand::TaskWorktreeIsolation { .. } => workflow::handle(state, c).await,
            c @ CoreCommand::TeamResourceBudget { .. } => workflow::handle(state, c).await,
            c @ CoreCommand::ComposableTerminationConditions { .. } => {
                workflow::handle(state, c).await
            }
            c @ CoreCommand::WorkspaceBootstrapManifest { .. } => {
                workflow_runtime::handle(state, c).await
            }
            c @ CoreCommand::TeamCoordinationPolicies { .. } => {
                workflow_runtime::handle(state, c).await
            }
            c @ CoreCommand::TypedAgentHandoffContract { .. } => {
                workflow_runtime::handle(state, c).await
            }
            c @ CoreCommand::SchemaDrivenAgentConfiguration { .. } => {
                workflow_runtime::handle(state, c).await
            }
            c @ CoreCommand::ExperienceReplayLibrary { .. } => {
                workflow_runtime::handle(state, c).await
            }
            c @ CoreCommand::RuntimeInterventionPipeline { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::CodeDiagnosticsFeedbackLoop { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::CodeReviewLane { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::StaticAnalysisPacks { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::WorkflowOptimizationLab { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::CoreTopicSubscriptionEventBus { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::DependencyAwareTaskGraph { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::DeclarativeAgentComponentRegistry { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::TypedContextReferences { .. } => {
                workflow_subsystems::handle(state, c).await
            }
            c @ CoreCommand::SafeUiExtensionFramework { .. } => {
                workflow_capabilities::handle(state, c).await
            }
            c @ CoreCommand::CapabilityWorkbench { .. } => {
                workflow_capabilities::handle(state, c).await
            }
            c @ CoreCommand::TeamCoordinator { .. } => orchestration::handle(state, c).await,
            c @ CoreCommand::ProjectInstructionStack { .. } => {
                orchestration::handle(state, c).await
            }
            c @ CoreCommand::WorkspaceSets { .. } => orchestration::handle(state, c).await,
            c @ CoreCommand::KnowledgeSourceRegistryProjectRole { .. } => {
                orchestration_runtime::handle(state, c).await
            }
            c @ CoreCommand::DurableRemoteTaskBridge { .. } => {
                orchestration_runtime::handle(state, c).await
            }
            c @ CoreCommand::MessageInterventionPolicies { .. } => {
                orchestration_runtime::handle(state, c).await
            }
            c @ CoreCommand::BatchInvocationRuntime { .. } => {
                orchestration_runtime::handle(state, c).await
            }
            c @ CoreCommand::ArchitectureSnapshot { .. } => models::handle(state, c).await,
            c @ CoreCommand::LocalModelRuntimeManager { .. } => models::handle(state, c).await,
            c @ CoreCommand::ModelPurposeRouting { .. } => models_runtime::handle(state, c).await,
            c @ CoreCommand::CodeAnchoredIntentMarkers { .. } => {
                models_runtime::handle(state, c).await
            }
            c @ CoreCommand::AgentGitChangeSets { .. } => models_runtime::handle(state, c).await,
            c @ CoreCommand::PolicyAwareToolResultCache { .. } => {
                models_runtime::handle(state, c).await
            }
            c @ CoreCommand::ArchitectEditorModelPipeline { .. } => {
                models_runtime::handle(state, c).await
            }
            c @ CoreCommand::EventVisualizerRegistry { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::ReasoningOperatorLibrary { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::OutputGuardrailPipeline { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::CustomizationInventory { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::StandingApprovalProfiles { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::ApprovalPolicyProfiles { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::CheckpointForking { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::PrivacyTelemetryGovernance { .. } => {
                models_observability::handle(state, c).await
            }
            c @ CoreCommand::ConversationBridgeAdapters { .. } => {
                models_tail::handle(state, c).await
            }
            c @ CoreCommand::GetTaskSnapshot { .. } => models_tail::handle(state, c).await,
            c @ CoreCommand::RestoreTaskSnapshot { .. } => models_tail::handle(state, c).await,
            c @ CoreCommand::GetBuildPolicy { .. } => build::handle(state, c).await,
            c @ CoreCommand::SaveBuildPolicy { .. } => build::handle(state, c).await,
            c @ CoreCommand::ApplyApprovedBuild { .. } => build::handle(state, c).await,
            c @ CoreCommand::PrepareBuild { .. } => build::handle(state, c).await,
            c @ CoreCommand::RunDoctor { .. } => build::handle(state, c).await,
            c @ CoreCommand::CreateDiagnosticsSnapshot { .. } => {
                build_runtime::handle(state, c).await
            }
            c @ CoreCommand::ExportDoctorLogs { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::CreateDatabaseBackup { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::PrepareDatabaseRestore { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::RestoreDatabase { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::CancelDatabaseOperation { .. } => {
                build_runtime::handle(state, c).await
            }
            c @ CoreCommand::SaveResearchEvidence { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::ListResearchEvidence { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::RunResearchFetch { .. } => build_runtime::handle(state, c).await,
            c @ CoreCommand::RunGroundedResearchSession { .. } => {
                build_runtime::handle(state, c).await
            }
            c @ CoreCommand::CreateMemory { .. } => memory::handle(state, c).await,
            c @ CoreCommand::ListMemory { .. } => memory::handle(state, c).await,
            c @ CoreCommand::SearchMemory { .. } => memory::handle(state, c).await,
            c @ CoreCommand::ArchiveMemory { .. } => memory::handle(state, c).await,
            c @ CoreCommand::ForgetMemory { .. } => memory::handle(state, c).await,
            c @ CoreCommand::MemoryViewsAndAdaptiveRecall { .. } => memory::handle(state, c).await,
            c @ CoreCommand::ModelEditProtocolRegistry { .. } => memory::handle(state, c).await,
            c @ CoreCommand::RemoteConversationChannels { .. } => memory::handle(state, c).await,
            c @ CoreCommand::PromptCachePlanner { .. } => memory::handle(state, c).await,
            c @ CoreCommand::DeclarativeRuntimeComponents { .. } => memory::handle(state, c).await,
            c @ CoreCommand::GuidedCalibrationSessions { .. } => {
                memory_runtime::handle(state, c).await
            }
            c @ CoreCommand::ExtensionConformanceKit { .. } => {
                memory_runtime::handle(state, c).await
            }
            c @ CoreCommand::PersistentAgentOrganizationRegistry { .. } => {
                memory_runtime::handle(state, c).await
            }
            c @ CoreCommand::ExecutionEnvironmentProfile { .. } => {
                memory_runtime::handle(state, c).await
            }
            c @ CoreCommand::ContextNamespace { .. } => context_namespace::handle(state, c).await,
            c @ CoreCommand::DurableBackgroundExecution { .. } => {
                durable_background_execution::handle(state, c).await
            }
            c @ CoreCommand::GetMemory { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::ListMemoryPending { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::GetMemoryConflicts { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::ConfirmMemory { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::RejectMemory { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::ReviseMemoryCandidate { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::SupersedeMemory { .. } => memory_runtime::handle(state, c).await,
            c @ CoreCommand::InstallCapability { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::ListCapabilities { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::MatchCapabilities { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::RemoveCapability { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::GetCapabilitySelection { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::PinCapabilitySelection { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::ReplaceCapabilitySelection { .. } => {
                capabilities::handle(state, c).await
            }
            c @ CoreCommand::RequestChildHandoff { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::ListChildHandoffs { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::SubmitChildRequest { .. } => capabilities::handle(state, c).await,
            c @ CoreCommand::SubmitChildReport { .. } => {
                capabilities_runtime::handle(state, c).await
            }
            c @ CoreCommand::IndexWorkspace { .. } => capabilities_runtime::handle(state, c).await,
            c @ CoreCommand::RebuildIndex { .. } => capabilities_runtime::handle(state, c).await,
            c @ CoreCommand::CancelWorkspaceIndex { .. } => {
                capabilities_runtime::handle(state, c).await
            }
            c @ CoreCommand::SearchWorkspaceKnowledge { .. } => {
                capabilities_runtime::handle(state, c).await
            }
            c @ CoreCommand::GetIndexStatus { .. } => capabilities_runtime::handle(state, c).await,
            c @ CoreCommand::SubmitFeedback { .. } => workspace::handle(state, c).await,
            c @ CoreCommand::ListFeedback { .. } => workspace::handle(state, c).await,
            c @ CoreCommand::GetContextLedger { .. } => workspace::handle(state, c).await,
            c @ CoreCommand::ListTaskScratchpad { .. } => workspace::handle(state, c).await,
            c @ CoreCommand::ClearTaskScratchpad { .. } => {
                workspace_context::handle(state, c).await
            }
            c @ CoreCommand::SummarizeContextNow { .. } => {
                workspace_context::handle(state, c).await
            }
            c @ CoreCommand::PinContextItem { .. } => workspace_context::handle(state, c).await,
            c @ CoreCommand::ReadContextArtifact { .. } => {
                workspace_context::handle(state, c).await
            }
            c @ CoreCommand::RetainChild { .. } => workspace_children::handle(state, c).await,
            c @ CoreCommand::GetRetainedChild { .. } => workspace_children::handle(state, c).await,
            c @ CoreCommand::SendChildFollowUp { .. } => workspace_children::handle(state, c).await,
            c @ CoreCommand::ListRetainedChildren { .. } => {
                workspace_children::handle(state, c).await
            }
            c @ CoreCommand::DeleteRetainedChild { .. } => {
                workspace_children::handle(state, c).await
            }
        }
    }
}
