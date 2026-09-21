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

#[path = "ipc_bridge_workspace_commands_extensions.rs"]
mod extensions;
