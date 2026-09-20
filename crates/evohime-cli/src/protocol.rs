//! Platform-neutral Core protocol client used by the Windows transport.
//!
//! Keeping framing, authentication and command envelopes independent from the
//! operating-system endpoint lets Linux CI exercise the real CLI/Core
//! contract without pretending that the Windows named pipe is portable.

use evohime_desktop_ipc::{generated, session, transport};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

const MAX_SNAPSHOT_INTERLEAVED_EVENTS: usize = 128;

pub struct CoreClient<S> {
    stream: S,
    sequence: u64,
    client_id: String,
    core_instance_id: String,
    session_epoch: u64,
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

    async fn write(&mut self, command: generated::CommandEnvelope) -> Result<(), String> {
        transport::write_frame(&mut self.stream, &command.encode_to_vec())
            .await
            .map_err(|error| error.to_string())
    }

    async fn read_event(&mut self) -> Result<generated::EventEnvelope, String> {
        let payload = transport::read_frame(&mut self.stream)
            .await
            .map_err(|error| error.to_string())?;
        let event = generated::EventEnvelope::decode(payload.as_slice())
            .map_err(|error| format!("protocol_error: {error}"))?;
        self.sequence = self.sequence.max(event.sequence_id);
        Ok(event)
    }

    pub async fn start(
        &mut self,
        task_id: String,
        prompt: String,
        workspace: String,
    ) -> Result<(), String> {
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::StartTask(
                generated::StartTask {
                    task_id,
                    prompt,
                    workspace_path: workspace,
                    preferred_route_hint: String::new(),
                    execution_kind: "agent".into(),
                    conversation_id: String::new(),
                    client_message_id: String::new(),
                },
            )),
        })
        .await
    }

    pub async fn start_workflow(
        &mut self,
        task_id: String,
        template_id: String,
        workspace: String,
    ) -> Result<(), String> {
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::StartWorkflow(
                generated::StartWorkflow {
                    template_id,
                    task_id,
                    workspace_path: workspace,
                    inputs: Vec::new(),
                    idempotency_key: uuid::Uuid::new_v4().to_string(),
                },
            )),
        })
        .await
    }

    pub async fn stop(&mut self, task_id: String) -> Result<(), String> {
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::StopTask(
                generated::StopTask { task_id },
            )),
        })
        .await
    }

    pub async fn snapshot(&mut self, task_id: String) -> Result<generated::EventEnvelope, String> {
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
            if event.event_type == "task.snapshot" {
                return Ok(event);
            }
        }
        Err("protocol_error: task snapshot response missing".into())
    }

    pub async fn next(&mut self) -> Result<generated::EventEnvelope, String> {
        self.read_event().await
    }
}

fn ready_generation(event: &generated::EventEnvelope) -> Result<(String, u64), String> {
    if !matches!(
        &event.event,
        Some(generated::event_envelope::Event::Ready(_))
    ) {
        return Err("authentication_failed: Core did not become ready".into());
    }
    if event.core_instance_id.is_empty() || event.session_epoch == 0 {
        return Err("authentication_failed: Core generation is invalid".into());
    }
    Ok((event.core_instance_id.clone(), event.session_epoch))
}

fn challenge_nonce(event: &generated::EventEnvelope) -> Result<String, String> {
    let Some(generated::event_envelope::Event::AuthChallenge(challenge)) = &event.event else {
        return Err("authentication_failed: challenge missing".into());
    };
    if challenge.expires_at_ms == 0
        || challenge.nonce.len() != session::NONCE_BYTES * 2
        || !challenge.nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("authentication_failed: challenge invalid".into());
    }
    Ok(challenge.nonce.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    const TEST_NONCE: &str = "abababababababababababababababababababababababababababababababab";

    fn event_with_auth_challenge(sequence_id: u64) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            sequence_id,
            task_id: String::new(),
            event_type: "auth.challenge".into(),
            payload: Vec::new(),
            core_instance_id: String::new(),
            session_epoch: 7,
            event: Some(generated::event_envelope::Event::AuthChallenge(
                generated::AuthChallenge {
                    nonce: TEST_NONCE.into(),
                    expires_at_ms: 9_999,
                },
            )),
        }
    }

    fn ready_event(sequence_id: u64) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            sequence_id,
            task_id: String::new(),
            event_type: "core.ready".into(),
            payload: Vec::new(),
            core_instance_id: "core-test".into(),
            session_epoch: 8,
            event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                core_version: "test".into(),
                core_info: None,
            })),
        }
    }

    #[test]
    fn rejects_ready_event_without_generation_identity() {
        let mut event = ready_event(42);
        event.core_instance_id.clear();
        assert_eq!(
            ready_generation(&event).unwrap_err(),
            "authentication_failed: Core generation is invalid"
        );

        let mut event = ready_event(42);
        event.session_epoch = 0;
        assert_eq!(
            ready_generation(&event).unwrap_err(),
            "authentication_failed: Core generation is invalid"
        );
    }

    #[test]
    fn rejects_malformed_auth_challenge() {
        let mut event = event_with_auth_challenge(1);
        if let Some(generated::event_envelope::Event::AuthChallenge(challenge)) = &mut event.event {
            challenge.nonce = "nonce-1".into();
        } else {
            panic!("expected auth challenge");
        }
        assert_eq!(
            challenge_nonce(&event).unwrap_err(),
            "authentication_failed: challenge invalid"
        );

        if let Some(generated::event_envelope::Event::AuthChallenge(challenge)) = &mut event.event {
            challenge.nonce = TEST_NONCE.into();
            challenge.expires_at_ms = 0;
        } else {
            panic!("expected auth challenge");
        }
        assert_eq!(
            challenge_nonce(&event).unwrap_err(),
            "authentication_failed: challenge invalid"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn authenticates_and_sends_commands_over_duplex_transport() {
        let context = session::LaunchContext::generate(String::new(), String::new(), 1)
            .expect("test context");
        let expected_secret = context.secret.clone();
        let (client_stream, server_stream) = duplex(16 * 1024);
        let server = tokio::spawn(async move {
            let (mut reader, mut writer) = tokio::io::split(server_stream);
            transport::write_frame(&mut writer, &event_with_auth_challenge(41).encode_to_vec())
                .await
                .expect("write challenge");

            let handshake_payload = transport::read_frame(&mut reader)
                .await
                .expect("read handshake");
            let handshake = generated::CommandEnvelope::decode(handshake_payload.as_slice())
                .expect("decode handshake");
            let generated::command_envelope::Command::Handshake(handshake) =
                handshake.command.expect("handshake command")
            else {
                panic!("expected handshake command");
            };
            assert_eq!(handshake.client_role, "cli");
            assert_eq!(handshake.client_id, handshake.session_id);
            assert_eq!(handshake.last_event_sequence, 40);
            assert_eq!(handshake.capabilities, ["headless-cli", "replay", "resync"]);
            assert_eq!(
                handshake.proof,
                expected_secret.proof("cli", &handshake.client_id, TEST_NONCE)
            );

            transport::write_frame(&mut writer, &ready_event(42).encode_to_vec())
                .await
                .expect("write ready");

            let start_payload = transport::read_frame(&mut reader)
                .await
                .expect("read start");
            let start =
                generated::CommandEnvelope::decode(start_payload.as_slice()).expect("decode start");
            assert_eq!(start.client_id, handshake.client_id);
            assert_eq!(start.core_instance_id, "core-test");
            assert_eq!(start.session_epoch, 8);
            let generated::command_envelope::Command::StartTask(start) =
                start.command.expect("start command")
            else {
                panic!("expected start command");
            };
            assert_eq!(start.task_id, "task-1");
            assert_eq!(start.prompt, "hello");
            assert_eq!(start.workspace_path, "/workspace");
            assert_eq!(start.execution_kind, "agent");

            let workflow_payload = transport::read_frame(&mut reader)
                .await
                .expect("read workflow");
            let workflow = generated::CommandEnvelope::decode(workflow_payload.as_slice())
                .expect("decode workflow");
            assert_eq!(workflow.client_id, handshake.client_id);
            assert_eq!(workflow.core_instance_id, "core-test");
            assert_eq!(workflow.session_epoch, 8);
            let generated::command_envelope::Command::StartWorkflow(workflow) =
                workflow.command.expect("workflow command")
            else {
                panic!("expected workflow command");
            };
            assert_eq!(workflow.task_id, "workflow-task");
            assert_eq!(workflow.template_id, "template-1");
            assert_eq!(workflow.workspace_path, "/workspace");
            assert!(!workflow.idempotency_key.is_empty());

            let stop_payload = transport::read_frame(&mut reader).await.expect("read stop");
            let stop =
                generated::CommandEnvelope::decode(stop_payload.as_slice()).expect("decode stop");
            assert_eq!(stop.client_id, handshake.client_id);
            assert_eq!(stop.core_instance_id, "core-test");
            assert_eq!(stop.session_epoch, 8);
            let generated::command_envelope::Command::StopTask(stop) =
                stop.command.expect("stop command")
            else {
                panic!("expected stop command");
            };
            assert_eq!(stop.task_id, "workflow-task");

            let snapshot_payload = transport::read_frame(&mut reader)
                .await
                .expect("read snapshot");
            let snapshot = generated::CommandEnvelope::decode(snapshot_payload.as_slice())
                .expect("decode snapshot");
            assert_eq!(snapshot.client_id, handshake.client_id);
            assert_eq!(snapshot.core_instance_id, "core-test");
            assert_eq!(snapshot.session_epoch, 8);
            let generated::command_envelope::Command::GetTaskSnapshot(snapshot) =
                snapshot.command.expect("snapshot command")
            else {
                panic!("expected snapshot command");
            };
            assert_eq!(snapshot.project_id, "");
            assert_eq!(snapshot.task_id, "workflow-task");
            transport::write_frame(
                &mut writer,
                &generated::EventEnvelope {
                    protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                    sequence_id: 44,
                    task_id: "workflow-task".into(),
                    event_type: "task.progress".into(),
                    payload: Vec::new(),
                    core_instance_id: "core-test".into(),
                    session_epoch: 8,
                    event: None,
                }
                .encode_to_vec(),
            )
            .await
            .expect("write interleaved event");
            transport::write_frame(
                &mut writer,
                &generated::EventEnvelope {
                    protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                    sequence_id: 45,
                    task_id: "workflow-task".into(),
                    event_type: "task.snapshot".into(),
                    payload: Vec::new(),
                    core_instance_id: "core-test".into(),
                    session_epoch: 8,
                    event: None,
                }
                .encode_to_vec(),
            )
            .await
            .expect("write snapshot");
        });

        let mut client = CoreClient::connect(client_stream, &context, 40)
            .await
            .expect("client authentication");
        assert_eq!(client.sequence(), 42);
        client
            .start("task-1".into(), "hello".into(), "/workspace".into())
            .await
            .expect("start task");
        client
            .start_workflow(
                "workflow-task".into(),
                "template-1".into(),
                "/workspace".into(),
            )
            .await
            .expect("start workflow");
        client
            .stop("workflow-task".into())
            .await
            .expect("stop task");
        let snapshot = client
            .snapshot("workflow-task".into())
            .await
            .expect("snapshot task");
        assert_eq!(snapshot.event_type, "task.snapshot");
        assert_eq!(client.sequence(), 45);
        server.await.expect("server task");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_invalid_launch_context_before_transport_use() {
        let mut context = session::LaunchContext::generate(String::new(), String::new(), 1)
            .expect("test context");
        context.pipe_name = "invalid".into();
        let (client_stream, _server_stream) = duplex(1024);

        let result = CoreClient::connect(client_stream, &context, 0).await;

        assert_eq!(
            result.err().as_deref(),
            Some("authentication_failed: invalid launch context")
        );
    }
}
