use super::auth::{challenge_nonce, ready_generation, validate_event_generation};
use evohime_desktop_ipc::{generated, session, transport};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

const MAX_SNAPSHOT_INTERLEAVED_EVENTS: usize = 128;

pub struct CoreClient<S> {
    pub(crate) stream: S,
    pub(crate) sequence: u64,
    pub(crate) client_id: String,
    pub(crate) core_instance_id: String,
    pub(crate) session_epoch: u64,
}

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn connect(
        stream: S,
        context: &session::LaunchContext,
        after_sequence: u64,
    ) -> Result<Self, String> {
        context
            .validate()
            .map_err(|_| "authentication_failed: invalid launch context".to_string())?;
        let client_id = format!("cli-{}", uuid::Uuid::new_v4());
        let mut client = Self {
            stream,
            sequence: after_sequence,
            client_id: client_id.clone(),
            core_instance_id: String::new(),
            session_epoch: 0,
        };
        let challenge = client.read_event().await?;
        let nonce = challenge_nonce(&challenge)?;
        let proof = context.secret.proof("cli", &client_id, &nonce);
        client
            .write(generated::CommandEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                request_id: uuid::Uuid::new_v4().to_string(),
                client_id: client_id.clone(),
                core_instance_id: String::new(),
                session_epoch: 0,
                command: Some(generated::command_envelope::Command::Handshake(
                    generated::Handshake {
                        protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                        client_id: client_id.clone(),
                        session_id: client_id,
                        session_epoch: 0,
                        last_event_sequence: after_sequence,
                        capabilities: vec!["headless-cli".into(), "replay".into(), "resync".into()],
                        client_role: "cli".into(),
                        nonce,
                        proof,
                    },
                )),
            })
            .await?;
        let ready = client.read_event().await?;
        (client.core_instance_id, client.session_epoch) = ready_generation(&ready)?;
        Ok(client)
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) async fn write(
        &mut self,
        command: generated::CommandEnvelope,
    ) -> Result<(), String> {
        transport::write_frame(&mut self.stream, &command.encode_to_vec())
            .await
            .map_err(|error| error.to_string())
    }

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
