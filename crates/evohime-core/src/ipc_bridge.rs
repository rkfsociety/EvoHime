use evohime_desktop_ipc::{generated, transport, FrameError};
use prost::Message;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{ApprovalCoordinator, CoreCommand, EventJournal, TaskCoordinator};
use evohime_local_storage::WorkItemRecord;
use evohime_model_gateway::ModelGatewayConfig;
use evohime_permissions::{Permission, PermissionMode};
use evohime_tool_runtime::ToolRegistry;
use std::sync::Arc;
use tokio::sync::oneshot;

const PROTOCOL_MAJOR: u32 = 1;
const PROTOCOL_MINOR: u32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum IpcBridgeError {
    #[error("IPC frame failed: {0}")]
    Frame(#[from] FrameError),
    #[error("protobuf message failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigSnapshot {
    pub provider: String,
    pub route: String,
    pub model: String,
    pub configured: bool,
}

pub struct IpcBridge {
    journal: EventJournal,
    coordinator: Option<TaskCoordinator>,
    approvals: Option<ApprovalCoordinator>,
    tools: Option<Arc<ToolRegistry>>,
    model_config: Option<ModelConfigSnapshot>,
    gateway_config: Option<ModelGatewayConfig>,
    core_instance_id: String,
    session_epoch: u64,
}

fn runtime_identity() -> (String, u64) {
    (
        uuid::Uuid::new_v4().to_string(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or_default(),
    )
}

impl IpcBridge {
    pub fn new(journal: EventJournal) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        Self {
            journal,
            coordinator: None,
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            core_instance_id,
            session_epoch,
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        Self {
            journal,
            coordinator: Some(coordinator),
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            core_instance_id,
            session_epoch,
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
        Self {
            journal,
            coordinator: Some(coordinator),
            approvals: Some(approvals),
            tools: Some(tools),
            model_config,
            gateway_config,
            core_instance_id,
            session_epoch,
        }
    }

    pub async fn process_once<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let payload = transport::read_frame(reader).await?;
        let command = generated::CommandEnvelope::decode(payload.as_slice())?;
        let request_id = command.request_id.clone();
        let client_id = command.client_id.clone();
        let command_hash = hex_encode(&payload);
        match command.command {
            Some(generated::command_envelope::Command::Handshake(_)) => {
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "core.ready".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                        protocol: Some(protocol()),
                        core_version: env!("CARGO_PKG_VERSION").into(),
                    })),
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ResyncRequest(request)) => {
                evohime_desktop_ipc::validate_resync_request(&request)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let limit = if request.max_events == 0 {
                    evohime_desktop_ipc::DEFAULT_RESYNC_MAX_EVENTS
                } else {
                    request.max_events
                } as usize;
                let batch = self
                    .journal
                    .replay_bounded(request.after_sequence as i64, limit)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let last_sequence = batch
                    .events
                    .last()
                    .map(|record| record.sequence_id as u64)
                    .unwrap_or(request.after_sequence);
                if batch.gap_detected {
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "after_sequence": request.after_sequence,
                        "first_available_sequence": batch.first_available_sequence,
                        "reason": "replay_gap",
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "replay.gap", payload).await?;
                }
                if request.include_full_snapshot {
                    let snapshot_json = serde_json::to_vec(&serde_json::json!({
                        "after_sequence": request.after_sequence,
                        "last_sequence": last_sequence,
                        "events": batch.events.iter().map(|record| serde_json::json!({
                            "sequence_id": record.sequence_id,
                            "task_id": record.task_id,
                            "event_type": record.event_type,
                            "payload": record.payload,
                            "created_at": record.created_at,
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let snapshot = generated::FullSnapshot {
                        sequence_id: last_sequence,
                        snapshot_json,
                    };
                    evohime_desktop_ipc::validate_full_snapshot(&snapshot)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "replay.full_snapshot".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: Some(generated::event_envelope::Event::FullSnapshot(snapshot)),
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                } else {
                    for record in batch.events {
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: record.sequence_id as u64,
                            task_id: record.task_id,
                            event_type: record.event_type,
                            payload: record.payload,
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: None,
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    }
                }
                let end = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: last_sequence,
                    task_id: String::new(),
                    event_type: "resync.end".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                let batch = self
                    .journal
                    .replay_bounded(replay.after_sequence as i64, 1_000)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let mut last_sequence = batch.last_sequence as u64;
                if batch.gap_detected {
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "after_sequence": replay.after_sequence,
                        "first_available_sequence": batch.first_available_sequence,
                        "reason": "replay_gap",
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "replay.gap", payload).await?;
                }
                for record in batch.events {
                    last_sequence = record.sequence_id as u64;
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: record.sequence_id as u64,
                        task_id: record.task_id,
                        event_type: record.event_type,
                        payload: record.payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                let end = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: last_sequence,
                    task_id: String::new(),
                    event_type: "replay.end".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ModelConfig(_)) => {
                let payload =
                    serde_json::to_vec(&self.model_config).unwrap_or_else(|_| b"null".to_vec());
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.config".into(),
                    payload,
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                let mode = if request.mode == "paid" {
                    "paid"
                } else {
                    "free"
                };
                let result = self
                    .gateway_config
                    .as_ref()
                    .and_then(|config| config.routes.get(&config.default_route))
                    .map(|route| async move {
                        evohime_model_gateway::fetch_available_models(route)
                            .await
                            .map(|models| {
                                models
                                    .into_iter()
                                    .filter(|model| {
                                        if mode == "free" {
                                            model.ends_with(":free")
                                        } else {
                                            !model.ends_with(":free")
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                    });
                let (models, error) = match result {
                    Some(request) => request.await,
                    None => Err(evohime_model_gateway::providers::ProviderError::Config(
                        "provider is not configured".into(),
                    )),
                }
                .map_or_else(
                    |error| (Vec::new(), Some(error.to_string())),
                    |models| (models, None),
                );
                let payload = serde_json::json!({
                    "mode": mode,
                    "models": models,
                    "error": error,
                });
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.catalog".into(),
                    payload: serde_json::to_vec(&payload).unwrap_or_default(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::PermissionMode(request)) => {
                if let Some(tools) = &self.tools {
                    let mode = match request.mode.as_str() {
                        "full" => PermissionMode::Allow,
                        "read_only" => PermissionMode::Deny,
                        _ => PermissionMode::Ask,
                    };
                    tools.permissions().set_all_modes(mode).await;
                    if request.mode == "read_only" {
                        tools
                            .permissions()
                            .set_mode(Permission::FilesystemRead, PermissionMode::Allow)
                            .await;
                        tools
                            .permissions()
                            .set_mode(Permission::GitRead, PermissionMode::Allow)
                            .await;
                    }
                }
            }
            Some(generated::command_envelope::Command::CreateProject(request)) => {
                let result = self
                    .dispatch_create_project(client_id, request_id, command_hash, request)
                    .await?;
                self.write_response(writer, "project.created", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateTask(request)) => {
                let item = WorkItemRecord {
                    id: request.task_id,
                    project_id: request.project_id,
                    parent_id: (!request.parent_id.is_empty()).then_some(request.parent_id),
                    title: request.title,
                    description: request.description,
                    source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                    acceptance_criteria: request.acceptance_criteria,
                    non_goals: request.non_goals,
                    status: if request.status.is_empty() {
                        "backlog".into()
                    } else {
                        request.status
                    },
                    priority: request.priority,
                    estimate: (request.estimate != 0).then_some(request.estimate),
                    complexity: (!request.complexity.is_empty()).then_some(request.complexity),
                    attempt_count: 0,
                    version: 1,
                };
                let result = self
                    .dispatch_create_task(client_id, request_id, command_hash, item)
                    .await?;
                self.write_response(writer, "task.created", result).await?;
            }
            Some(generated::command_envelope::Command::UpdateTaskStatus(request)) => {
                let result = self
                    .dispatch_update_status(
                        client_id,
                        request_id,
                        command_hash,
                        request.task_id,
                        request.expected_version,
                        request.status,
                    )
                    .await?;
                self.write_response(writer, "task.status_updated", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::AddTaskEdge(request)) => {
                let result = self
                    .dispatch_add_edge(
                        client_id,
                        request_id,
                        command_hash,
                        request.from_task_id,
                        request.to_task_id,
                        request.kind,
                    )
                    .await?;
                self.write_response(writer, "task.edge_added", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetTaskGraph(request)) => {
                let result = self.dispatch_get_task_graph(request.project_id).await?;
                self.write_response(writer, "task.graph", result).await?;
            }
            Some(generated::command_envelope::Command::NextReadyTask(request)) => {
                let result = self.dispatch_next_ready_task(request.project_id).await?;
                self.write_response(writer, "task.next_ready", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ImportPrd(request)) => {
                let result = self
                    .dispatch_import_prd(client_id, request_id, command_hash, request)
                    .await?;
                self.write_response(writer, "prd.imported", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskHistory(request)) => {
                let result = self
                    .dispatch_get_task_history(request.task_id, request.limit as usize)
                    .await?;
                self.write_response(writer, "task.history", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskContext(request)) => {
                let result = self
                    .dispatch_get_task_context(
                        request.project_id,
                        request.task_id,
                        request.max_chars as usize,
                    )
                    .await?;
                self.write_response(writer, "task.context", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskPlanSpec(request)) => {
                let result = self
                    .dispatch_get_task_plan_spec(
                        request.project_id,
                        request.task_id,
                        request.max_chars as usize,
                    )
                    .await?;
                self.write_response(writer, "task.plan_spec", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ApplyApprovedBuild(request)) => {
                let result = self
                    .dispatch_apply_approved_build(
                        request.project_id,
                        request.run_id,
                        request.task_id,
                        request.approved_build_json,
                    )
                    .await?;
                self.write_response(writer, "build.applied", result).await?;
            }
            Some(generated::command_envelope::Command::PrepareBuild(request)) => {
                let result = self
                    .dispatch_prepare_build(request.project_id, request.proposal_json)
                    .await?;
                self.write_response(writer, "build.prepared", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetTaskSnapshot(request)) => {
                let result = self
                    .dispatch_get_task_snapshot(request.project_id, request.task_id)
                    .await?;
                self.write_response(writer, "task.snapshot", result).await?;
            }
            Some(generated::command_envelope::Command::RestoreTaskSnapshot(request)) => {
                let result = self
                    .dispatch_restore_task_snapshot(
                        request.project_id,
                        request.task_id,
                        request.snapshot_id,
                    )
                    .await?;
                self.write_response(writer, "snapshot.restored", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetBuildPolicy(request)) => {
                let result = self.dispatch_get_build_policy(request.project_id).await?;
                self.write_response(writer, "build.policy", result).await?;
            }
            Some(generated::command_envelope::Command::SaveBuildPolicy(request)) => {
                let result = self
                    .dispatch_save_build_policy(
                        request.project_id,
                        request.policy_json,
                        request.expected_version,
                    )
                    .await?;
                self.write_response(writer, "build.policy.saved", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::StartTask(start)) => {
                if let Some(coordinator) = &self.coordinator {
                    coordinator
                        .dispatch(CoreCommand::StartTask {
                            task_id: start.task_id,
                            prompt: start.prompt,
                            workspace_root: (!start.workspace_path.is_empty())
                                .then(|| std::path::PathBuf::from(start.workspace_path)),
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                }
            }
            Some(generated::command_envelope::Command::StopTask(stop)) => {
                if let Some(coordinator) = &self.coordinator {
                    coordinator
                        .dispatch(CoreCommand::StopTask {
                            task_id: stop.task_id,
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                }
            }
            Some(generated::command_envelope::Command::RunDoctor(request)) => {
                let result = self
                    .dispatch_run_doctor(request.project_id, command.protocol.clone())
                    .await?;
                self.write_response(writer, "doctor.report", result).await?;
            }
            Some(generated::command_envelope::Command::SaveResearchEvidence(request)) => {
                let result = self.dispatch_save_research_evidence(request).await?;
                self.write_response(writer, "research.evidence.saved", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListResearchEvidence(request)) => {
                let result = self
                    .dispatch_list_research_evidence(request.work_item_id)
                    .await?;
                self.write_response(writer, "research.evidence.list", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateMemory(request)) => {
                let result = self.dispatch_create_memory(request).await?;
                self.write_response(writer, "memory.created", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListMemory(request)) => {
                let result = self.dispatch_list_memory(request).await?;
                self.write_response(writer, "memory.list", result).await?;
            }
            Some(generated::command_envelope::Command::SearchMemory(request)) => {
                let result = self.dispatch_search_memory(request).await?;
                self.write_response(writer, "memory.search", result).await?;
            }
            Some(generated::command_envelope::Command::ArchiveMemory(request)) => {
                let result = self.dispatch_archive_memory(request).await?;
                self.write_response(writer, "memory.archived", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ForgetMemory(request)) => {
                let result = self.dispatch_forget_memory(request).await?;
                self.write_response(writer, "memory.forgotten", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::InstallCapability(request)) => {
                let result = self.dispatch_install_capability(request).await?;
                self.write_response(writer, "capability.installed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListCapabilities(request)) => {
                let result = self.dispatch_list_capabilities(request).await?;
                self.write_response(writer, "capability.list", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::MatchCapabilities(request)) => {
                let result = self.dispatch_match_capabilities(request).await?;
                self.write_response(writer, "capability.match", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RemoveCapability(request)) => {
                let result = self.dispatch_remove_capability(request).await?;
                self.write_response(writer, "capability.removed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RequestChildHandoff(request)) => {
                let result = self.dispatch_request_child_handoff(request).await?;
                self.write_response(writer, "child.handoff.requested", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListChildHandoffs(request)) => {
                let result = self.dispatch_list_child_handoffs(request).await?;
                self.write_response(writer, "child.handoff.list", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SubmitChildRequest(request)) => {
                let result = self.dispatch_submit_child_request(request).await?;
                self.write_response(writer, "child.request.submitted", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SubmitChildReport(request)) => {
                let result = self.dispatch_submit_child_report(request).await?;
                self.write_response(writer, "child.report.accepted", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ResolveApproval(resolve)) => {
                let approval_id = uuid::Uuid::parse_str(&resolve.approval_id)
                    .map_err(|error| FrameError::Io(format!("invalid approval id: {error}")))?;
                if let (Some(approvals), Some(tools)) = (&self.approvals, &self.tools) {
                    if tools
                        .permissions()
                        .resolve(approval_id, resolve.granted)
                        .await
                        .is_some()
                    {
                        let _ = approvals.resolve(approval_id, resolve.granted).await;
                    }
                }
            }
            None => {}
        }
        Ok(())
    }

    async fn dispatch_create_project(
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

    async fn dispatch_create_task(
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

    async fn dispatch_update_status(
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

    async fn dispatch_add_edge(
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

    async fn dispatch_get_task_graph(&self, project_id: String) -> Result<Vec<u8>, IpcBridgeError> {
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

    async fn dispatch_next_ready_task(
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

    async fn dispatch_import_prd(
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

    async fn dispatch_get_task_history(
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

    async fn dispatch_get_task_context(
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

    async fn dispatch_get_task_plan_spec(
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

    async fn dispatch_apply_approved_build(
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

    async fn dispatch_prepare_build(
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

    async fn dispatch_get_task_snapshot(
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

    async fn dispatch_restore_task_snapshot(
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

    async fn dispatch_get_build_policy(
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

    async fn dispatch_save_build_policy(
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

    async fn dispatch_run_doctor(
        &self,
        project_id: String,
        protocol: Option<generated::ProtocolVersion>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let approval_required = match &self.tools {
            Some(tools) => !matches!(
                tools.permissions().mode(Permission::FilesystemWrite).await,
                PermissionMode::Allow
            ),
            None => true,
        };
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunDoctor {
                project_id,
                protocol_major: protocol.map(|version| version.major),
                expected_protocol_major: PROTOCOL_MAJOR,
                provider: self.provider_probe(),
                approval_required,
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

    async fn dispatch_save_research_evidence(
        &self,
        request: generated::SaveResearchEvidence,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveResearchEvidence {
                work_item_id: request.work_item_id,
                source_kind: request.source_kind,
                source_ref: request.source_ref,
                title: request.title,
                publisher: request.publisher,
                content_type: request.content_type,
                raw_excerpt: request.raw_excerpt,
                retrieved_at_ms: request.retrieved_at_ms,
                ttl_ms: request.ttl_ms,
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

    async fn dispatch_list_research_evidence(
        &self,
        work_item_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListResearchEvidence {
                work_item_id,
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

    async fn dispatch_create_memory(
        &self,
        request: generated::CreateMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                title: request.title,
                content: request.content,
                provenance_kind: request.provenance_kind,
                provenance_id: request.provenance_id,
                provenance_locator: request.provenance_locator,
                privacy: request.privacy,
                ttl_ms: request.ttl_ms,
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

    async fn dispatch_list_memory(
        &self,
        request: generated::ListMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                include_archived: request.include_archived,
                limit: request.limit,
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

    async fn dispatch_search_memory(
        &self,
        request: generated::SearchMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SearchMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                query: request.query,
                limit: request.limit,
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

    async fn dispatch_archive_memory(
        &self,
        request: generated::ArchiveMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ArchiveMemory {
                id: request.id,
                approval_id: request.approval_id,
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

    async fn dispatch_forget_memory(
        &self,
        request: generated::ForgetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ForgetMemory {
                id: request.id,
                approval_id: request.approval_id,
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

    async fn dispatch_install_capability(
        &self,
        request: generated::InstallCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::InstallCapability {
                manifest_json: request.manifest_json,
                install_source: request.install_source,
                source_path: request.source_path,
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

    async fn dispatch_list_capabilities(
        &self,
        request: generated::ListCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListCapabilities {
                limit: request.limit,
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

    async fn dispatch_match_capabilities(
        &self,
        request: generated::MatchCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::MatchCapabilities {
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
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

    async fn dispatch_remove_capability(
        &self,
        request: generated::RemoveCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RemoveCapability {
                id: request.id,
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

    async fn dispatch_request_child_handoff(
        &self,
        request: generated::RequestChildHandoff,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RequestChildHandoff {
                handoff_id: request.handoff_id,
                task_id: request.task_id,
                kind: request.kind,
                from_role: request.from_role,
                from_name: request.from_name,
                to_role: request.to_role,
                to_name: request.to_name,
                purpose: request.purpose,
                payload: request.payload,
                sequence: request.sequence,
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

    async fn dispatch_list_child_handoffs(
        &self,
        request: generated::ListChildHandoffs,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListChildHandoffs {
                task_id: request.task_id,
                limit: request.limit,
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

    async fn dispatch_submit_child_request(
        &self,
        request: generated::SubmitChildRequest,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildRequest {
                child_task_id: request.child_task_id,
                parent_task_id: request.parent_task_id,
                role: request.role,
                kind: request.kind,
                reduced_context: request.reduced_context,
                max_output_bytes: request.max_output_bytes,
                requested_capabilities: request.requested_capabilities,
                parent_is_child: request.parent_is_child,
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

    async fn dispatch_submit_child_report(
        &self,
        request: generated::SubmitChildReport,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildReport {
                child_task_id: request.child_task_id,
                status: request.status,
                summary: request.summary,
                findings: request.findings,
                sources: request.sources,
                confidence_percent: request.confidence_percent,
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

    /// Builds a Core Doctor provider probe from already-loaded, secret-free
    /// gateway configuration. Never exposes an API key value, only whether
    /// one is present.
    fn provider_probe(&self) -> crate::doctor::ProviderProbe {
        let (provider_id, model_id, configured) = match &self.model_config {
            Some(config) => (
                config.provider.clone(),
                config.model.clone(),
                config.configured,
            ),
            None => (String::new(), String::new(), false),
        };
        let key_present = self
            .gateway_config
            .as_ref()
            .and_then(|config| config.routes.get(&config.default_route))
            .map(|route| route.configured())
            .unwrap_or(false);
        let metadata_valid = !provider_id.is_empty() && !model_id.is_empty();
        crate::doctor::ProviderProbe {
            provider_id,
            model_id,
            configured,
            key_present,
            metadata_valid,
        }
    }

    async fn write_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        event_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: event_type.into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: None,
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn protocol() -> generated::ProtocolVersion {
    generated::ProtocolVersion {
        major: PROTOCOL_MAJOR,
        minor: PROTOCOL_MINOR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreEvent;
    use tokio::io::duplex;

    #[tokio::test]
    async fn serves_replay_command_over_framed_transport() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-ipc".into(),
                final_message: "replayed".into(),
            })
            .await
            .expect("event records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "request-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 0 },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("bridge serves replay");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.sequence_id, 1);
        assert_eq!(event.task_id, "task-ipc");
        assert_eq!(event.event_type, "task.completed");
        assert!(String::from_utf8(event.payload)
            .expect("payload utf8")
            .contains("replayed"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn handshake_exposes_runtime_identity() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-handshake-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "handshake".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 9,
            command: Some(generated::command_envelope::Command::Handshake(
                generated::Handshake {
                    protocol: Some(protocol()),
                    client_id: "client".into(),
                    session_id: "session".into(),
                    session_epoch: 9,
                    last_event_sequence: 0,
                    capabilities: vec!["task.crud".into()],
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("handshake writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("handshake serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert!(!event.core_instance_id.is_empty());
        assert!(event.session_epoch > 0);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn malformed_command_is_rejected_without_crashing_bridge() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-malformed-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &[0xff, 0x00, 0x01])
            .await
            .expect("malformed frame writes");
        assert!(matches!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await,
            Err(IpcBridgeError::Protobuf(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reconnect_replays_only_events_after_last_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-reconnect-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-reconnect".into(),
                prompt: "one".into(),
            })
            .await
            .expect("first event");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-reconnect".into(),
                final_message: "two".into(),
            })
            .await
            .expect("second event");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "reconnect".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 2,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents {
                    after_sequence: first as u64,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("reconnect writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("reconnect serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("event reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "task.completed");
        assert_eq!(event.sequence_id, first as u64 + 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_task_crud_and_replays_deduplicated_create() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-task-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "create-project-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-1".into(),
                    title: "Demo".into(),
                    workspace_path: "C:\\Projects\\demo".into(),
                    source_ref: "plan:0a".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let first = transport::read_frame(&mut client)
            .await
            .expect("first response");

        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("duplicate writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("duplicate replays");
        let second = transport::read_frame(&mut client)
            .await
            .expect("second response");
        assert_eq!(first, second);

        let mut conflict = command.clone();
        if let Some(generated::command_envelope::Command::CreateProject(project)) =
            &mut conflict.command
        {
            project.title = "Different".into();
        }
        transport::write_frame(&mut client, &conflict.encode_to_vec())
            .await
            .expect("conflicting writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn imports_prd_without_touching_workspace_and_rejects_duplicate_import() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-prd-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(32 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let project = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "project-prd".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-prd".into(),
                    title: "PRD".into(),
                    workspace_path: "C:\\Projects\\prd".into(),
                    source_ref: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &project.encode_to_vec())
            .await
            .expect("project writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("project response");

        let import = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "import-prd-1".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ImportPrd(
                generated::ImportPrd {
                    import_id: "import-1".into(),
                    project_id: "project-prd".into(),
                    origin: "prd.md".into(),
                    version: "v1".into(),
                    source_text: "# Plan\n\n## Task\n- [ ] Pass\n".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &import.encode_to_vec())
            .await
            .expect("import writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("import succeeds");
        let response = transport::read_frame(&mut client)
            .await
            .expect("import response");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "prd.imported");
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );

        let mut duplicate = import;
        if let Some(generated::command_envelope::Command::ImportPrd(request)) =
            &mut duplicate.command
        {
            request.source_text.push_str("\n## Another");
            duplicate.request_id = "import-prd-2".into();
        }
        transport::write_frame(&mut client, &duplicate.encode_to_vec())
            .await
            .expect("duplicate writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_run_doctor_with_real_storage_and_pipe_state() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-doctor-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "doctor-1".into(),
            client_id: "doctor-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunDoctor(
                generated::RunDoctor {
                    project_id: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("doctor serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "doctor.report");
        let report: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("doctor report is valid json");
        assert_eq!(report["bounded"], serde_json::json!(true));
        let checks = report["checks"].as_array().expect("checks array");
        assert_eq!(checks.len(), 5);
        let storage_check = checks
            .iter()
            .find(|check| check["id"] == "storage")
            .expect("storage check present");
        // A freshly-opened journal exists, is writable, and is on the
        // current schema version, so this reflects real (not fabricated)
        // storage state.
        assert_eq!(storage_check["status"], serde_json::json!("OK"));
        let permissions_check = checks
            .iter()
            .find(|check| check["id"] == "permissions")
            .expect("permissions check present");
        // No project_id was supplied, so the permissions probe is honestly
        // fail-closed rather than fabricated as healthy.
        assert_ne!(permissions_check["status"], serde_json::json!("OK"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn saves_and_lists_research_evidence_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-research-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let save = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-save-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SaveResearchEvidence(
                generated::SaveResearchEvidence {
                    work_item_id: "task-42".into(),
                    source_kind: "url".into(),
                    source_ref: "https://example.test/article".into(),
                    title: "Example Article".into(),
                    publisher: "Example Org".into(),
                    content_type: "text/html".into(),
                    raw_excerpt: "Useful finding sk-secret alice@example.test".into(),
                    retrieved_at_ms: 1_700_000_000_000,
                    ttl_ms: 3_600_000,
                },
            )),
        };
        transport::write_frame(&mut client, &save.encode_to_vec())
            .await
            .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("save response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.saved");
        let saved: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("save payload is valid json");
        assert_eq!(saved["work_item_id"], serde_json::json!("task-42"));
        let evidence_id = saved["id"].as_str().expect("id present").to_owned();
        assert_eq!(
            saved["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-list-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListResearchEvidence(
                generated::ListResearchEvidence {
                    work_item_id: "task-42".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("list response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], serde_json::json!(evidence_id));
        assert_eq!(records[0]["source_kind"], serde_json::json!("url"));
        assert_eq!(
            records[0]["redacted_excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        assert_eq!(records[0]["provenance_link"], serde_json::json!("task-42"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_create_list_search_archive_forget_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-memory-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Create two memories in the same task scope, one containing a
        // secret that must come back redacted.
        let create_one = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-1",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Rust build notes".into(),
                content: "Rust build cache lives in target/".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-1".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_one.event_type, "memory.created");
        let created_one: serde_json::Value =
            serde_json::from_slice(&create_one.payload).expect("create payload is valid json");
        let memory_one_id = created_one["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        let create_two = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-2",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Deployment secret".into(),
                content: "Token is sk-secret, keep it safe".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-2".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_two.event_type, "memory.created");
        let created_two: serde_json::Value =
            serde_json::from_slice(&create_two.payload).expect("create payload is valid json");
        assert_eq!(
            created_two["record"]["content"],
            serde_json::json!("Token is [REDACTED] keep it safe")
        );
        let memory_two_id = created_two["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        // List returns both, newest first.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-1",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "memory.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["id"], serde_json::json!(memory_two_id));
        assert_eq!(records[1]["id"], serde_json::json!(memory_one_id));
        assert_eq!(records[0]["project_id"], serde_json::json!("proj-1"));
        assert_eq!(records[0]["secondary_id"], serde_json::json!("task-1"));

        // Search only matches the record with "rust" in it.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-1",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                query: "rust".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(search.event_type, "memory.search");
        let searched: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("search payload is valid json");
        let hits = searched["records"].as_array().expect("records array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["id"], serde_json::json!(memory_one_id));

        // Archive without an approval token is rejected.
        let archive_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-archive-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ArchiveMemory(
                generated::ArchiveMemory {
                    id: memory_one_id.clone(),
                    approval_id: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &archive_envelope.encode_to_vec())
            .await
            .expect("archive request writes");
        let denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(denied.is_err(), "archive without approval must fail");

        // Archive with an approval token succeeds and is audited.
        let archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-archive-1",
            generated::command_envelope::Command::ArchiveMemory(generated::ArchiveMemory {
                id: memory_one_id.clone(),
                approval_id: "approval-1".into(),
            }),
        )
        .await;
        assert_eq!(archive.event_type, "memory.archived");
        let archived: serde_json::Value =
            serde_json::from_slice(&archive.payload).expect("archive payload is valid json");
        assert_eq!(archived["archived"], serde_json::json!(true));

        // Archived record is hidden from default listing.
        let list_after_archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-2",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value = serde_json::from_slice(&list_after_archive.payload)
            .expect("list payload is valid json");
        let records_after = listed_after["records"].as_array().expect("records array");
        assert_eq!(records_after.len(), 1);
        assert_eq!(records_after[0]["id"], serde_json::json!(memory_two_id));

        // Forget with an approval token erases title/content.
        let forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-forget-1",
            generated::command_envelope::Command::ForgetMemory(generated::ForgetMemory {
                id: memory_two_id.clone(),
                approval_id: "approval-2".into(),
            }),
        )
        .await;
        assert_eq!(forget.event_type, "memory.forgotten");
        let forgotten: serde_json::Value =
            serde_json::from_slice(&forget.payload).expect("forget payload is valid json");
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));

        let list_after_forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-3",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: true,
                limit: 10,
            }),
        )
        .await;
        let listed_final: serde_json::Value =
            serde_json::from_slice(&list_after_forget.payload).expect("list payload is valid json");
        let records_final = listed_final["records"].as_array().expect("records array");
        // Forgotten records are excluded even with include_archived=true.
        assert!(records_final
            .iter()
            .all(|record| record["id"] != serde_json::json!(memory_two_id)));

        let _ = std::fs::remove_file(path);
    }

    fn capability_manifest_json(name: &str, version: &str, risk_class: &str) -> String {
        serde_json::json!({
            "name": name,
            "version": version,
            "content_hash": "0123456789abcdef0123456789abcdef",
            "signature": "sig:v1:trusted",
            "roles": [{
                "name": "reviewer",
                "version": "1",
                "content_hash": "abcdef0123456789abcdef0123456789"
            }],
            "skills": [],
            "allowed_tools": ["filesystem.read", "git.diff"],
            "allowed_domains": ["docs.example.com"],
            "protected_paths": ["src"],
            "risk_class": risk_class,
            "install": {
                "source": "local_archive",
                "allow_install_scripts": false,
                "allow_update": true,
                "rollback_on_failure": true
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn capability_install_list_match_remove_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-capability-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Installing a manifest that declares an unsupported https_archive
        // install source must be rejected: that installer is out of scope
        // for this pass.
        let https_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-https".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                    install_source: "https_archive".into(),
                    source_path: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &https_envelope.encode_to_vec())
            .await
            .expect("https install request writes");
        let https_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            https_denied.is_err(),
            "https_archive install source must be rejected in this pass"
        );

        // Installing a manifest with a malformed content_hash must be
        // rejected via the real RegistryError::InvalidHash path.
        let bad_hash_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-hash".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-hash", "1.0.0", "medium")
                        .replace("0123456789abcdef0123456789abcdef", "not-a-hex-hash"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_hash_envelope.encode_to_vec())
            .await
            .expect("bad hash install request writes");
        let bad_hash_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_hash_denied.is_err(),
            "manifest with a malformed content_hash must be rejected"
        );

        // Installing a manifest with an invalid risk_class must be rejected
        // before it ever reaches storage.
        let bad_risk_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-risk".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-risk", "1.0.0", "extreme"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_risk_envelope.encode_to_vec())
            .await
            .expect("bad risk install request writes");
        let bad_risk_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_risk_denied.is_err(),
            "manifest with an invalid risk_class must be rejected"
        );

        // A valid local-archive manifest installs successfully.
        let install = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-install-1",
            generated::command_envelope::Command::InstallCapability(generated::InstallCapability {
                manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                install_source: "local_archive".into(),
                source_path: "C:/archives/reviewer.zip".into(),
            }),
        )
        .await;
        assert_eq!(install.event_type, "capability.installed");
        let installed: serde_json::Value =
            serde_json::from_slice(&install.payload).expect("install payload is valid json");
        assert_eq!(installed["manifest"]["name"], serde_json::json!("reviewer"));

        // List returns the installed manifest.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-1",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "capability.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let manifests = listed["manifests"].as_array().expect("manifests array");
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0]["name"], serde_json::json!("reviewer"));

        // Match selects the installed manifest for a fitting query.
        let matched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-match-1",
            generated::command_envelope::Command::MatchCapabilities(generated::MatchCapabilities {
                intent: "review reviewer".into(),
                required_tools: vec!["git.diff".into()],
                required_domains: vec!["docs.example.com".into()],
                requested_risk: "low".into(),
            }),
        )
        .await;
        assert_eq!(matched.event_type, "capability.match");
        let matches: serde_json::Value =
            serde_json::from_slice(&matched.payload).expect("match payload is valid json");
        let hits = matches["matches"].as_array().expect("matches array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["manifest_name"], serde_json::json!("reviewer"));

        // Remove deletes the manifest.
        let removed = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-remove-1",
            generated::command_envelope::Command::RemoveCapability(generated::RemoveCapability {
                id: "reviewer".into(),
            }),
        )
        .await;
        assert_eq!(removed.event_type, "capability.removed");
        let removed_payload: serde_json::Value =
            serde_json::from_slice(&removed.payload).expect("remove payload is valid json");
        assert_eq!(removed_payload["removed"], serde_json::json!(true));

        // Removing again is rejected: the manifest is already gone.
        let remove_again_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-remove-2".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RemoveCapability(
                generated::RemoveCapability {
                    id: "reviewer".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &remove_again_envelope.encode_to_vec())
            .await
            .expect("second remove request writes");
        let remove_again = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            remove_again.is_err(),
            "removing a manifest that no longer exists must fail"
        );

        let list_after_remove = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-2",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value =
            serde_json::from_slice(&list_after_remove.payload).expect("list payload is valid json");
        assert!(listed_after["manifests"]
            .as_array()
            .expect("manifests array")
            .is_empty());

        let _ = std::fs::remove_file(path);
    }

    /// Proves the read-only child delegation boundary holds end-to-end
    /// through the real IPC command path, not just at the pure-function
    /// level (`child_runtime::ChildTaskRequest::validate` /
    /// `child_runtime::accept_report` unit tests): a request naming a
    /// non-read-only capability is rejected, a nested-child request is
    /// rejected, a report with secret-like content is rejected, and a
    /// valid read-only request plus matching valid report round-trips
    /// through save -> submit -> list successfully. This test does not
    /// spawn or execute any child agent; it only proves the
    /// request/report validation and persistence boundary.
    #[tokio::test]
    async fn child_handoff_request_report_security_boundary_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-child-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "child-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // (a) A request naming a non-read-only capability (workspace.write)
        // must be rejected end-to-end, not just by the pure function.
        let write_capability_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-write-capability".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-write".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.write".into()],
                    parent_is_child: false,
                },
            )),
        };
        transport::write_frame(&mut client, &write_capability_envelope.encode_to_vec())
            .await
            .expect("write-capability request writes");
        let write_capability_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            write_capability_denied.is_err(),
            "a request naming a non-read-only capability must be rejected"
        );

        // (b) A nested child (parent_is_child = true) must be rejected.
        let nested_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-nested".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-nested".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into()],
                    parent_is_child: true,
                },
            )),
        };
        transport::write_frame(&mut client, &nested_envelope.encode_to_vec())
            .await
            .expect("nested request writes");
        let nested_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            nested_denied.is_err(),
            "a nested child request (parent_is_child = true) must be rejected"
        );

        // A valid read-only request submits successfully.
        let submitted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-request-valid",
            generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-1".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into(), "git.diff".into()],
                    parent_is_child: false,
                },
            ),
        )
        .await;
        assert_eq!(submitted.event_type, "child.request.submitted");
        let submitted_payload: serde_json::Value =
            serde_json::from_slice(&submitted.payload).expect("submit payload is valid json");
        assert_eq!(
            submitted_payload["request"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // (c) A report containing secret-like content must be rejected,
        // even though it matches a valid, already-persisted request.
        let secret_report_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-report-secret".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildReport(
                generated::SubmitChildReport {
                    child_task_id: "child-1".into(),
                    status: "complete".into(),
                    summary: "api_key=do-not-leak".into(),
                    findings: vec!["module is bounded".into()],
                    sources: vec!["src/lib.rs:10".into()],
                    confidence_percent: 90,
                },
            )),
        };
        transport::write_frame(&mut client, &secret_report_envelope.encode_to_vec())
            .await
            .expect("secret report writes");
        let secret_report_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            secret_report_denied.is_err(),
            "a report containing secret-like content must be rejected"
        );

        // (d) A matching, valid report round-trips through
        // save -> submit -> list successfully.
        let accepted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-report-valid",
            generated::command_envelope::Command::SubmitChildReport(generated::SubmitChildReport {
                child_task_id: "child-1".into(),
                status: "complete".into(),
                summary: "found one relevant module".into(),
                findings: vec!["module is bounded".into()],
                sources: vec!["src/lib.rs:10".into()],
                confidence_percent: 90,
            }),
        )
        .await;
        assert_eq!(accepted.event_type, "child.report.accepted");
        let accepted_payload: serde_json::Value =
            serde_json::from_slice(&accepted.payload).expect("report payload is valid json");
        assert_eq!(
            accepted_payload["report"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // A separately requested handoff persists and lists back through
        // the real command path too.
        let handoff = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-valid",
            generated::command_envelope::Command::RequestChildHandoff(
                generated::RequestChildHandoff {
                    handoff_id: "handoff-1".into(),
                    task_id: "task-1".into(),
                    kind: "delegate".into(),
                    from_role: "coordinator".into(),
                    from_name: String::new(),
                    to_role: "researcher".into(),
                    to_name: String::new(),
                    purpose: "investigate module bounds".into(),
                    payload: std::collections::HashMap::new(),
                    sequence: 1,
                },
            ),
        )
        .await;
        assert_eq!(handoff.event_type, "child.handoff.requested");

        let listed_handoffs = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-list",
            generated::command_envelope::Command::ListChildHandoffs(generated::ListChildHandoffs {
                task_id: "task-1".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(listed_handoffs.event_type, "child.handoff.list");
        let listed_handoffs_payload: serde_json::Value =
            serde_json::from_slice(&listed_handoffs.payload)
                .expect("handoff list payload is valid json");
        let handoffs = listed_handoffs_payload["handoffs"]
            .as_array()
            .expect("handoffs array");
        assert_eq!(handoffs.len(), 1);
        assert_eq!(handoffs[0]["handoff_id"], serde_json::json!("handoff-1"));

        let _ = std::fs::remove_file(path);
    }
}
