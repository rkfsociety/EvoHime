use super::auth::{challenge_nonce, ready_generation};
use evohime_desktop_ipc::{generated, session, transport};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

/// Authenticated client bound to one Core session and ordered event stream.
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
    /// Authenticates the CLI role over an already-connected async stream.
    ///
    /// `context` must be the supervisor-issued launch context. Events at or
    /// before `after_sequence` are not replayed by this client.
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

    /// Returns the last event sequence consumed by this session.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn command_envelope(
        &self,
        command: generated::command_envelope::Command,
    ) -> generated::CommandEnvelope {
        generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(command),
        }
    }

    pub(crate) async fn write(
        &mut self,
        command: generated::CommandEnvelope,
    ) -> Result<(), String> {
        transport::write_frame(&mut self.stream, &command.encode_to_vec())
            .await
            .map_err(|error| error.to_string())
    }
}
