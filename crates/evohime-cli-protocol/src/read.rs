use super::{auth::validate_event_generation, client::CoreClient};
use evohime_desktop_ipc::{generated, transport};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

const MAX_SNAPSHOT_INTERLEAVED_EVENTS: usize = 128;

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub(crate) async fn read_event(&mut self) -> Result<generated::EventEnvelope, String> {
        let payload = transport::read_frame(&mut self.stream)
            .await
            .map_err(|error| error.to_string())?;
        let event = generated::EventEnvelope::decode(payload.as_slice())
            .map_err(|error| format!("protocol_error: {error}"))?;
        validate_event_generation(&event, &self.core_instance_id, self.session_epoch)?;
        self.sequence = self.sequence.max(event.sequence_id);
        Ok(event)
    }

    pub async fn snapshot(&mut self, task_id: String) -> Result<generated::EventEnvelope, String> {
        let expected_task_id = task_id.clone();
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::GetTaskSnapshot(
                generated::GetTaskSnapshot {
                    project_id: String::new(),
                    task_id,
                },
            )),
        })
        .await?;
        for _ in 0..=MAX_SNAPSHOT_INTERLEAVED_EVENTS {
            let event = self.read_event().await?;
            if event.event_type == "task.snapshot" && event.task_id == expected_task_id {
                return Ok(event);
            }
        }
        Err("protocol_error: task snapshot response missing".into())
    }

    pub async fn next(&mut self) -> Result<generated::EventEnvelope, String> {
        self.read_event().await
    }
}
