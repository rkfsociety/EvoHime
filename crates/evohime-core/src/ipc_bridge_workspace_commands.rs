use super::*;

impl IpcBridge {
    pub(crate) async fn publish_voice_command(
        &self,
        command: &crate::voice_command::PendingCommand,
        state: evohime_listener_contract::VoiceCommandState,
    ) {
        let (Ok(command_id), Ok(app_id)) = (
            evohime_listener_contract::CommandId::new(command.command_id.clone()),
            evohime_listener_contract::AppId::new(command.app_id.clone()),
        ) else {
            return;
        };
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::VoiceCommand {
                command_id,
                kind: command.kind,
                app_id,
                command_state: state,
            })
            .await;
    }

    pub(crate) async fn dispatch_create_project(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        request: generated::CreateProject,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateProject {
                client_id,
                request_id,
                command_hash,
                project_id: request.project_id,
                title: request.title,
                workspace_path: request.workspace_path,
                source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_plan_artifact(
        &self,
        operation: String,
        request: generated::PlanArtifactCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PlanArtifact {
                operation,
                artifact_json: request.artifact_json,
                artifact_id: request.artifact_id,
                expected_version: request.expected_version,
                status: request.status,
                policy_snapshot_hash: request.policy_snapshot_hash,
                task_id: (!request.task_id.is_empty()).then_some(request.task_id),
                workflow_run_id: (!request.workflow_run_id.is_empty())
                    .then_some(request.workflow_run_id),
                correlation_id: request.correlation_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_workspace_state_checkpoint(
        &self,
        operation: String,
        request: generated::WorkspaceStateCheckpointCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::WorkspaceStateCheckpoint {
                operation,
                project_id: request.project_id,
                task_id: (!request.task_id.is_empty()).then_some(request.task_id),
                checkpoint_id: (!request.checkpoint_id.is_empty()).then_some(request.checkpoint_id),
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_incremental_change_protocol(
        &self,
        operation: String,
        request: generated::IncrementalChangeProtocolCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::IncrementalChangeProtocol {
                operation,
                run_id: request.run_id,
                payload: request.payload,
                expected_version: request.expected_version,
                observed_fingerprint: request.observed_fingerprint,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_revision_safe_workspace_files(
        &self,
        operation: String,
        request: generated::RevisionSafeWorkspaceFilesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RevisionSafeWorkspaceFiles {
                operation,
                project_id: request.owner_scope,
                logical_path: request.logical_path,
                content: request.content,
                expected_hash: request.expected_hash,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_task_worktree_isolation(
        &self,
        operation: String,
        request: generated::TaskWorktreeIsolationCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TaskWorktreeIsolation {
                operation,
                project_id: request.owner_scope,
                task_id: request.task_id,
                worktree_id: request.worktree_id,
                branch: request.branch,
                base_commit: request.base_commit,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_team_resource_budget(
        &self,
        operation: String,
        request: generated::TeamResourceBudgetCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TeamResourceBudget {
                operation,
                owner_scope: request.owner_scope,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_composable_termination_conditions(
        &self,
        operation: String,
        request: generated::ComposableTerminationConditionsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ComposableTerminationConditions {
                operation,
                owner_scope: request.owner_scope,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_workspace_bootstrap_manifest(
        &self,
        operation: String,
        request: generated::WorkspaceBootstrapManifestCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::WorkspaceBootstrapManifest {
                operation,
                project_id: request.project_id,
                workspace_id: request.workspace_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_team_coordination_policies(
        &self,
        operation: String,
        request: generated::TeamCoordinationPoliciesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TeamCoordinationPolicies {
                operation,
                team_id: request.team_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_typed_agent_handoff_contract(
        &self,
        operation: String,
        request: generated::TypedAgentHandoffContractCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TypedAgentHandoffContract {
                operation,
                handoff_id: request.handoff_id,
                packet_json: request.packet_json,
                actor: request.actor,
                reason: request.reason,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_schema_driven_agent_configuration(
        &self,
        operation: String,
        request: generated::SchemaDrivenAgentConfigurationCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SchemaDrivenAgentConfiguration {
                operation,
                scope: request.scope,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_runtime_intervention_pipeline(
        &self,
        operation: String,
        request: generated::RuntimeInterventionPipelineCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RuntimeInterventionPipeline {
                operation,
                run_id: request.run_id,
                payload: request.payload,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_code_diagnostics_feedback_loop(
        &self,
        operation: String,
        request: generated::CodeDiagnosticsFeedbackLoopCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CodeDiagnosticsFeedbackLoop {
                operation,
                workspace_root_id: request.workspace_root_id,
                payload: request.payload,
                baseline_snapshot_id: request.baseline_snapshot_id,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_code_review_lane(
        &self,
        request: generated::CodeReviewLaneCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CodeReviewLane {
                operation: request.operation,
                review_id: request.review_id,
                target_id: request.target_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_static_analysis_packs(
        &self,
        request: generated::StaticAnalysisPacksCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::StaticAnalysisPacks {
                operation: request.operation,
                pack_id: request.pack_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_context_loadouts(
        &self,
        request: generated::ContextLoadoutsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ContextLoadouts {
                operation: request.operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_skill_source_lifecycle(
        &self,
        request: generated::SkillSourceLifecycleCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SkillSourceLifecycle {
                operation: request.operation,
                installation_id: request.installation_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_kernel_capability_facade(
        &self,
        request: generated::KernelCapabilityFacadeCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::KernelCapabilityFacade {
                operation: request.operation,
                record_id: request.record_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_authorized_security_assessment(
        &self,
        request: generated::AuthorizedSecurityAssessmentCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AuthorizedSecurityAssessment {
                operation: request.operation,
                assessment_id: request.assessment_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_runtime_service_graph(
        &self,
        request: generated::RuntimeServiceGraphCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RuntimeServiceGraph {
                operation: request.operation,
                graph_id: request.graph_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_agent_program_optimizer(
        &self,
        request: generated::AgentProgramOptimizerCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AgentProgramOptimizer {
                operation: request.operation,
                program_id: request.program_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_project_knowledge_notebook(
        &self,
        request: generated::ProjectKnowledgeNotebookCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ProjectKnowledgeNotebook {
                operation: request.operation,
                notebook_id: request.notebook_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_git_remote_publication_protocol(
        &self,
        request: generated::GitRemotePublicationProtocolCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GitRemotePublicationProtocol {
                operation: request.operation,
                protocol_id: request.protocol_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_voice_input_dictation(
        &self,
        request: generated::VoiceInputDictationCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::VoiceInputDictation {
                operation: request.operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_offline_experience_consolidation(
        &self,
        request: generated::OfflineExperienceConsolidationCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::OfflineExperienceConsolidation {
                operation: request.operation,
                cycle_id: request.cycle_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_deterministic_review_execution_plan(
        &self,
        request: generated::DeterministicReviewExecutionPlanCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DeterministicReviewExecutionPlan {
                operation: request.operation,
                plan_id: request.plan_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_workflow_optimization_lab(
        &self,
        operation: String,
        request: generated::WorkflowOptimizationLabCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::WorkflowOptimizationLab {
                operation,
                run_id: request.run_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_core_topic_subscription_event_bus(
        &self,
        operation: String,
        request: generated::CoreTopicSubscriptionEventBusCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CoreTopicSubscriptionEventBus {
                operation,
                payload: request.payload,
                capability: request.capability,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_dependency_aware_task_graph(
        &self,
        operation: String,
        request: generated::DependencyAwareTaskGraphCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DependencyAwareTaskGraph {
                operation,
                graph_id: request.graph_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                grants: request.grants,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_safe_ui_extension_framework(
        &self,
        operation: String,
        request: generated::SafeUiExtensionFrameworkCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SafeUiExtensionFramework {
                operation,
                extension_id: request.extension_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_typed_context_references(
        &self,
        operation: String,
        request: generated::TypedContextReferencesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TypedContextReferences {
                operation,
                ref_id: request.ref_id,
                payload: request.payload,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_capability_workbench(
        &self,
        operation: String,
        request: generated::CapabilityWorkbenchCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CapabilityWorkbench {
                operation,
                instance_id: request.instance_id,
                owner_id: request.owner_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                grants: request.grants,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_team_coordinator(
        &self,
        operation: String,
        request: generated::TeamCoordinatorCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TeamCoordinator {
                operation,
                work_item_id: request.work_item_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_project_instruction_stack(
        &self,
        operation: String,
        request: generated::ProjectInstructionStackCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ProjectInstructionStack {
                operation,
                workspace_root: request.workspace_root,
                payload: request.payload,
                relevant_paths: request.relevant_paths,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_workspace_sets(
        &self,
        operation: String,
        request: generated::WorkspaceSetsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::WorkspaceSets {
                operation,
                set_id: request.set_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_knowledge_source_registry(
        &self,
        operation: String,
        request: generated::KnowledgeSourceRegistryProjectRoleCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        let generated::KnowledgeSourceRegistryProjectRoleCommand {
            source_id,
            payload,
            expected_version,
            idempotency_key,
            ..
        } = request;
        if operation == "research_session_run" {
            coordinator
                .dispatch(CoreCommand::RunGroundedResearchSession { payload, reply })
                .await
                .map_err(|error| FrameError::Io(error.to_string()))?;
            return response
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from);
        }
        coordinator
            .dispatch(CoreCommand::KnowledgeSourceRegistryProjectRole {
                operation,
                source_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_agent_git_change_sets(
        &self,
        operation: String,
        request: generated::AgentGitChangeSetsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AgentGitChangeSets {
                operation,
                change_set_id: request.change_set_id,
                workspace_root: request.workspace_root,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_architect_editor_pipeline(
        &self,
        operation: String,
        request: generated::ArchitectEditorModelPipelineCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ArchitectEditorModelPipeline {
                operation,
                pipeline_id: request.pipeline_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_event_visualizer_registry(
        &self,
        operation: String,
        request: generated::EventVisualizerRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::EventVisualizerRegistry {
                operation,
                visualizer_id: request.visualizer_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_reasoning_operator_library(
        &self,
        operation: String,
        request: generated::ReasoningOperatorLibraryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReasoningOperatorLibrary {
                operation,
                operator_id: request.operator_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_output_guardrail_pipeline(
        &self,
        operation: String,
        request: generated::OutputGuardrailPipelineCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::OutputGuardrailPipeline {
                operation,
                pipeline_id: request.pipeline_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_customization_inventory(
        &self,
        operation: String,
        request: generated::CustomizationInventoryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CustomizationInventory {
                operation,
                item_id: request.item_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_standing_approval_profiles(
        &self,
        operation: String,
        request: generated::StandingApprovalProfilesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::StandingApprovalProfiles {
                operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_approval_policy_profiles(
        &self,
        operation: String,
        request: generated::ApprovalPolicyProfilesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ApprovalPolicyProfiles {
                operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_checkpoint_forking(
        &self,
        operation: String,
        request: generated::CheckpointForkingCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::CheckpointForking {
            operation,
            fork_run_id: request.fork_run_id,
            payload: request.payload,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_privacy_telemetry_governance(
        &self,
        operation: String,
        request: generated::PrivacyTelemetryGovernanceCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::PrivacyTelemetryGovernance {
            operation,
            category: request.category,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_conversation_bridge_adapters(
        &self,
        operation: String,
        request: generated::ConversationBridgeAdaptersCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ConversationBridgeAdapters {
            operation,
            bridge_id: request.bridge_id,
            payload: request.payload,
            expected_revision: request.expected_revision,
            idempotency_key: request.idempotency_key,
            correlation_id: request.correlation_id,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_remote_conversation_channels(
        &self,
        operation: String,
        request: generated::RemoteConversationChannelsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::RemoteConversationChannels {
            operation,
            connection_id: request.connection_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_prompt_cache_planner(
        &self,
        operation: String,
        request: generated::PromptCachePlannerCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::PromptCachePlanner {
            operation,
            plan_id: request.plan_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_declarative_runtime_components(
        &self,
        operation: String,
        request: generated::DeclarativeRuntimeComponentsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::DeclarativeRuntimeComponents {
            operation,
            component_id: request.component_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_guided_calibration_sessions(
        &self,
        operation: String,
        request: generated::GuidedCalibrationSessionsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::GuidedCalibrationSessions {
            operation,
            session_id: request.session_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_extension_conformance_kit(
        &self,
        operation: String,
        request: generated::ExtensionConformanceKitCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ExtensionConformanceKit {
            operation,
            subject_id: request.subject_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_durable_remote_task_bridge(
        &self,
        operation: String,
        request: generated::DurableRemoteTaskBridgeCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::DurableRemoteTaskBridge {
            operation,
            remote_task_id: request.remote_task_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_message_intervention_policies(
        &self,
        operation: String,
        request: generated::MessageInterventionPoliciesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::MessageInterventionPolicies {
            operation,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_batch_invocation_runtime(
        &self,
        operation: String,
        request: generated::BatchInvocationRuntimeCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::BatchInvocationRuntime {
            operation,
            batch_id: request.batch_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_policy_aware_tool_result_cache(
        &self,
        operation: String,
        request: generated::PolicyAwareToolResultCacheCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::PolicyAwareToolResultCache {
            operation,
            cache_key: request.cache_key,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_code_anchored_intent_markers(
        &self,
        operation: String,
        request: generated::CodeAnchoredIntentMarkersCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::CodeAnchoredIntentMarkers {
            operation,
            file_path: request.file_path,
            revision: request.revision,
            payload: request.payload,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_model_purpose_routing(
        &self,
        operation: String,
        request: generated::ModelPurposeRoutingCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ModelPurposeRouting {
            operation,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_local_model_runtime_manager(
        &self,
        operation: String,
        request: generated::LocalModelRuntimeManagerCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::LocalModelRuntimeManager {
            operation,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_architecture_snapshot(
        &self,
        operation: String,
        request: generated::ArchitectureSnapshotCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ArchitectureSnapshot {
            operation,
            snapshot_id: request.snapshot_id,
            workspace_root: request.workspace_root,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_persistent_agent_organization_registry(
        &self,
        operation: String,
        request: generated::PersistentAgentOrganizationRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PersistentAgentOrganizationRegistry {
                operation,
                agent_id: request.agent_id,
                owner_scope: request.owner_scope,
                // Renderer input cannot select a privileged actor. Core-only
                // recovery uses the internal EventJournal API directly.
                actor: "user".into(),
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_execution_environment_profile(
        &self,
        operation: String,
        request: generated::ExecutionEnvironmentProfileCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExecutionEnvironmentProfile {
                operation,
                profile_id: request.profile_id,
                owner_scope: request.owner_scope,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_context_namespace(
        &self,
        operation: String,
        request: generated::ContextNamespaceCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        if request.schema_version != crate::context_namespace::SCHEMA_VERSION {
            return Err(FrameError::Io("unsupported context namespace schema".into()).into());
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ContextNamespace {
                operation,
                namespace_id: request.namespace_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_durable_background_execution(
        &self,
        operation: String,
        request: generated::BackgroundExecutionCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        if request.schema_version != crate::durable_background_execution::SCHEMA_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.payload.len() > crate::durable_background_execution::MAX_SNAPSHOT_BYTES
        {
            return Err(FrameError::Io("invalid background execution request".into()).into());
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DurableBackgroundExecution {
                operation,
                run_id: request.run_id,
                owner_scope: request.owner_scope,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_memory_views_and_adaptive_recall(
        &self,
        operation: String,
        request: generated::MemoryViewsAndAdaptiveRecallCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::MemoryViewsAndAdaptiveRecall {
            operation,
            view_id: request.view_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_model_edit_protocol_registry(
        &self,
        operation: String,
        request: generated::ModelEditProtocolRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ModelEditProtocolRegistry {
            operation,
            protocol_id: request.protocol_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_declarative_agent_component_registry(
        &self,
        operation: String,
        request: generated::DeclarativeAgentComponentRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DeclarativeAgentComponentRegistry {
                operation,
                registry_id: request.registry_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_experience_replay_library(
        &self,
        operation: String,
        request: generated::ExperienceReplayLibraryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExperienceReplayLibrary {
                operation,
                scope: request.scope,
                scope_id: request.scope_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_create_task(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        item: WorkItemRecord,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateTask {
                client_id,
                request_id,
                command_hash,
                item,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_update_status(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        task_id: String,
        expected_version: i64,
        status: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::UpdateTaskStatus {
                client_id,
                request_id,
                command_hash,
                task_id,
                expected_version,
                status,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_add_edge(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        from_task_id: String,
        to_task_id: String,
        kind: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AddTaskEdge {
                client_id,
                request_id,
                command_hash,
                from_task_id,
                to_task_id,
                kind,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_task_graph(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskGraph { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_next_ready_task(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::NextReadyTask { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_import_prd(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        request: generated::ImportPrd,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ImportPrd {
                client_id,
                request_id,
                command_hash,
                import_id: request.import_id,
                project_id: request.project_id,
                origin: request.origin,
                version: request.version,
                source_text: request.source_text,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_task_history(
        &self,
        task_id: String,
        limit: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskHistory {
                task_id,
                limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_task_context(
        &self,
        project_id: String,
        task_id: String,
        max_chars: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskContext {
                project_id,
                task_id,
                max_chars,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_task_plan_spec(
        &self,
        project_id: String,
        task_id: String,
        max_chars: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskPlanSpec {
                project_id,
                task_id,
                max_chars,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_apply_approved_build(
        &self,
        project_id: String,
        run_id: String,
        task_id: String,
        approved_build_json: Vec<u8>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ApplyApprovedBuild {
                project_id,
                run_id,
                task_id,
                approved_build_json,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_prepare_build(
        &self,
        project_id: String,
        proposal_json: Vec<u8>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PrepareBuild {
                project_id,
                proposal_json,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_task_snapshot(
        &self,
        project_id: String,
        task_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskSnapshot {
                project_id,
                task_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_restore_task_snapshot(
        &self,
        project_id: String,
        task_id: String,
        snapshot_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RestoreTaskSnapshot {
                project_id,
                task_id,
                snapshot_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_build_policy(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetBuildPolicy { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_save_build_policy(
        &self,
        project_id: String,
        policy_json: Vec<u8>,
        expected_version: i64,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveBuildPolicy {
                project_id,
                policy_json,
                expected_version,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
}

#[path = "ipc_bridge_workspace_operations.rs"]
mod operations;
