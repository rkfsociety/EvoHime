use evohime_desktop_ipc::{generated, transport, FrameError};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{CoreCommand, EventJournal, TaskCoordinator};

const PROTOCOL_MAJOR: u32 = 1;
const PROTOCOL_MINOR: u32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum IpcBridgeError {
    #[error("IPC frame failed: {0}")]
    Frame(#[from] FrameError),
    #[error("protobuf message failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
}

pub struct IpcBridge {
    journal: EventJournal,
    coordinator: Option<TaskCoordinator>,
}

impl IpcBridge {
    pub fn new(journal: EventJournal) -> Self {
        Self {
            journal,
            coordinator: None,
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        Self {
            journal,
            coordinator: Some(coordinator),
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
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::StartTask(start)) => {
                if let Some(coordinator) = &self.coordinator {
                    coordinator
                        .dispatch(CoreCommand::StartTask {
                            task_id: start.task_id,
                            prompt: start.prompt,
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
