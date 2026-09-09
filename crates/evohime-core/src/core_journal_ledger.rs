use super::*;

impl EventJournal {
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
            CoreEvent::EventPersistenceFailed { .. } => "event-persistence",
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
            CoreEvent::CodeReviewLane { review_id, .. } => review_id,
            CoreEvent::StaticAnalysisPacks { pack_id, .. } => pack_id,
            CoreEvent::ContextLoadouts { profile_id, .. } => profile_id,
            CoreEvent::SkillSourceLifecycle { installation_id, .. } => installation_id,
            CoreEvent::KernelCapabilityFacade { record_id, .. } => record_id,
            CoreEvent::AuthorizedSecurityAssessment { assessment_id, .. } => assessment_id,
            CoreEvent::RuntimeServiceGraph { graph_id, .. } => graph_id,
            CoreEvent::AgentProgramOptimizer { program_id, .. } => program_id,
            CoreEvent::ProjectKnowledgeNotebook { notebook_id, .. } => notebook_id,
            CoreEvent::GitRemotePublicationProtocol { protocol_id, .. } => protocol_id,
            CoreEvent::VoiceInputDictation { profile_id, .. } => profile_id,
            CoreEvent::OfflineExperienceConsolidation { cycle_id, .. } => cycle_id,
            CoreEvent::DeterministicReviewExecutionPlan { plan_id, .. } => plan_id,
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
            CoreEvent::ExecutionEnvironmentProfile { profile_id, .. } => profile_id,
            CoreEvent::ContextNamespace { namespace_id, .. } => namespace_id,
            CoreEvent::DurableBackgroundExecution { run_id, .. } => run_id,
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
            CoreEvent::EventPersistenceFailed { .. } => "event.persistence_failed",
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
            CoreEvent::CodeReviewLane { .. } => "code_review_lane.result",
            CoreEvent::StaticAnalysisPacks { .. } => "static_analysis_packs.result",
            CoreEvent::ContextLoadouts { .. } => "context_loadouts.result",
            CoreEvent::SkillSourceLifecycle { .. } => "skill_source_lifecycle.result",
            CoreEvent::KernelCapabilityFacade { .. } => "kernel_capability_facade.result",
            CoreEvent::AuthorizedSecurityAssessment { .. } => "authorized_security_assessment.result",
            CoreEvent::RuntimeServiceGraph { .. } => "runtime_service_graph.result",
            CoreEvent::AgentProgramOptimizer { .. } => "agent_program_optimizer.result",
            CoreEvent::ProjectKnowledgeNotebook { .. } => "project_knowledge_notebook.result",
            CoreEvent::GitRemotePublicationProtocol { .. } => "git_remote_publication_protocol.result",
            CoreEvent::VoiceInputDictation { .. } => "voice_input_dictation.result",
            CoreEvent::OfflineExperienceConsolidation { .. } => "offline_experience_consolidation.result",
            CoreEvent::DeterministicReviewExecutionPlan { .. } => "deterministic_review_execution_plan.result",
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
            CoreEvent::ExecutionEnvironmentProfile { .. } => "execution_environment_profile.result",
            CoreEvent::ContextNamespace { .. } => "context_namespace.result",
            CoreEvent::DurableBackgroundExecution { .. } => "background_execution.result",
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
        let database_path = self.database_path.clone();
        let task_id = task_id.to_owned();
        let event_type = event_type.to_owned();
        let event_type_for_sql = event_type.clone();
        let sql_started = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || -> Result<i64, StorageError> {
            let database = LocalDatabase::open(database_path.as_ref())?;
            let mut last_sequence =
                database.append_event(&task_id, &event_type_for_sql, &payload)?;
            if let Some((conversation_id, client_message_id, workspace_id)) =
                evohime_local_storage::domains::audit::task_binding(
                    database.connection(),
                    &task_id,
                )?
            {
                for draft in projected {
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
                            causation_id: Some(&client_message_id),
                            task_id: Some(&task_id),
                            run_id: Some(&task_id),
                            turn_id: Some(&task_id),
                            client_message_id: Some(&client_message_id),
                            persistence_class: &draft.persistence_class,
                            sensitivity: &draft.sensitivity,
                            timestamp_ms: task_memory::now_millis() as i64,
                        },
                    )?;
                    let renderer = crate::conversation_event_log::renderer_event(&stored)
                        .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
                    last_sequence = database.append_event(
                        &task_id,
                        "conversation.event",
                        &serde_json::to_vec(&renderer)?,
                    )?;
                }
            }
            Ok(last_sequence)
        })
        .await
        .map_err(|error| StorageError::InvalidInput(format!("journal worker failed: {error}")))?;
        tracing::debug!(
            sql_ms = sql_started.elapsed().as_secs_f64() * 1000.0,
            event_type,
            "core event SQL write completed"
        );
        result
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
    ) -> Result<Vec<evohime_local_storage::domains::memory::MemoryRecord>, StorageError> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::search_lessons(
            database.connection(),
            evohime_local_storage::domains::memory::MemoryScope::Project,
            scope_id,
            query,
            now,
            limit,
        )
        .map_err(|error| StorageError::InvalidRecovery(error.to_string()))
    }

    pub async fn record_lesson(
        &self,
        record: &evohime_local_storage::domains::memory::MemoryRecord,
    ) -> Result<evohime_local_storage::domains::memory::MemoryRecord, StorageError> {
        crate::memory_governance::MemoryWriteGate::validate(record)
            .map_err(|error| StorageError::InvalidRecovery(error.to_string()))?;
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::upsert_lesson(
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
        let path = path.as_ref().to_owned();
        tokio::task::spawn_blocking(move || LocalDatabase::preview_backup(path))
            .await
            .map_err(|error| {
                StorageError::InvalidInput(format!("backup preview worker failed: {error}"))
            })?
    }

    pub async fn create_database_backup(
        &self,
        path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress) + Send + 'static,
    ) -> Result<BackupResult, StorageError> {
        let path = path.as_ref().to_owned();
        let app_version = app_version.to_owned();
        let database = Arc::clone(&self.database).lock_owned().await;
        tokio::task::spawn_blocking(move || database.create_backup(path, &app_version, progress))
            .await
            .map_err(|error| StorageError::InvalidInput(format!("backup worker failed: {error}")))?
    }

    pub async fn create_database_backup_with_cancel(
        &self,
        path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress) + Send + 'static,
        cancelled: impl FnMut() -> bool + Send + 'static,
    ) -> Result<BackupResult, StorageError> {
        let path = path.as_ref().to_owned();
        let app_version = app_version.to_owned();
        let database = Arc::clone(&self.database).lock_owned().await;
        tokio::task::spawn_blocking(move || {
            database.create_backup_with_cancel(path, &app_version, progress, cancelled)
        })
        .await
        .map_err(|error| StorageError::InvalidInput(format!("backup worker failed: {error}")))?
    }

    pub async fn restore_database(
        &self,
        backup_path: impl AsRef<std::path::Path>,
        safety_path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress) + Send + 'static,
    ) -> Result<RestoreResult, StorageError> {
        let backup_path = backup_path.as_ref().to_owned();
        let safety_path = safety_path.as_ref().to_owned();
        let app_version = app_version.to_owned();
        let mut database = Arc::clone(&self.database).lock_owned().await;
        tokio::task::spawn_blocking(move || {
            database.restore_backup(backup_path, safety_path, &app_version, progress)
        })
        .await
        .map_err(|error| StorageError::InvalidInput(format!("restore worker failed: {error}")))?
    }

    pub async fn restore_database_with_cancel(
        &self,
        backup_path: impl AsRef<std::path::Path>,
        safety_path: impl AsRef<std::path::Path>,
        app_version: &str,
        progress: impl FnMut(BackupProgress) + Send + 'static,
        cancelled: impl FnMut() -> bool + Send + 'static,
    ) -> Result<RestoreResult, StorageError> {
        let backup_path = backup_path.as_ref().to_owned();
        let safety_path = safety_path.as_ref().to_owned();
        let app_version = app_version.to_owned();
        let mut database = Arc::clone(&self.database).lock_owned().await;
        tokio::task::spawn_blocking(move || {
            database.restore_backup_with_cancel(
                backup_path,
                safety_path,
                &app_version,
                progress,
                cancelled,
            )
        })
        .await
        .map_err(|error| StorageError::InvalidInput(format!("restore worker failed: {error}")))?
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
}
