use evohime_desktop_ipc::{generated, transport, FrameError};
use prost::Message;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{ApprovalCoordinator, CoreCommand, EventJournal, TaskCoordinator};
use evohime_model_gateway::ModelGatewayConfig;
use evohime_tool_runtime::ToolRegistry;
use evohime_permissions::{Permission, PermissionMode};
use std::sync::Arc;

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
}

impl IpcBridge {
    pub fn new(journal: EventJournal) -> Self {
        Self {
            journal,
            coordinator: None,
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        Self {
            journal,
            coordinator: Some(coordinator),
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
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
        Self {
            journal,
            coordinator: Some(coordinator),
            approvals: Some(approvals),
            tools: Some(tools),
            model_config,
            gateway_config,
        }
    }

    pub async fn process_once<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let payload = transport::read_frame(reader).await?;
        let command = generated::CommandEnvelope::decode(payload.as_slice())?;
        match command.command {
            Some(generated::command_envelope::Command::Handshake(_)) => {
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "core.ready".into(),
                    payload: Vec::new(),
                    core_instance_id: String::new(),
                    session_epoch: 0,
                    event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                        protocol: Some(protocol()),
                        core_version: env!("CARGO_PKG_VERSION").into(),
                    })),
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                let mut last_sequence = replay.after_sequence;
                for record in self
                    .journal
                    .replay(replay.after_sequence as i64, 1_000)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?
                {
                    last_sequence = record.sequence_id as u64;
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: record.sequence_id as u64,
                        task_id: record.task_id,
                        event_type: record.event_type,
                        payload: record.payload,
                        core_instance_id: String::new(),
                        session_epoch: 0,
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
                    core_instance_id: String::new(),
                    session_epoch: 0,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ModelConfig(_)) => {
                let payload = serde_json::to_vec(&self.model_config).unwrap_or_else(|_| b"null".to_vec());
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.config".into(),
                    payload,
                    core_instance_id: String::new(),
                    session_epoch: 0,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                let mode = if request.mode == "paid" { "paid" } else { "free" };
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
                    core_instance_id: String::new(),
                    session_epoch: 0,
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
}
