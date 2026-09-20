use super::auth::{challenge_nonce, ready_generation, validate_event_generation};
use super::CoreClient;
use evohime_desktop_ipc::{generated, session, transport};
use prost::Message;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::duplex;

const TEST_NONCE: &str = "abababababababababababababababababababababababababababababababab";

fn test_expiry() -> u64 {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_millis();
    u64::try_from(now_ms)
        .expect("test clock fits in u64")
        .saturating_add(30_000)
}

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
                expires_at_ms: test_expiry(),
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
fn rejects_events_from_another_core_generation() {
    let mut event = ready_event(43);
    event.core_instance_id = "other-core".into();
    assert_eq!(
        validate_event_generation(&event, "core-test", 8).unwrap_err(),
        "protocol_error: Core generation changed"
    );

    let mut event = ready_event(43);
    event.session_epoch = 9;
    assert_eq!(
        validate_event_generation(&event, "core-test", 8).unwrap_err(),
        "protocol_error: Core generation changed"
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

#[test]
fn rejects_expired_auth_challenge() {
    let mut event = event_with_auth_challenge(1);
    if let Some(generated::event_envelope::Event::AuthChallenge(challenge)) = &mut event.event {
        challenge.expires_at_ms = 1;
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
    let context =
        session::LaunchContext::generate(String::new(), String::new(), 1).expect("test context");
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
                task_id: "other-task".into(),
                event_type: "task.snapshot".into(),
                payload: Vec::new(),
                core_instance_id: "core-test".into(),
                session_epoch: 8,
                event: None,
            }
            .encode_to_vec(),
        )
        .await
        .expect("write unrelated snapshot");
        transport::write_frame(
            &mut writer,
            &generated::EventEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                sequence_id: 45,
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
                sequence_id: 46,
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
    assert_eq!(client.sequence(), 46);
    server.await.expect("server task");
}

#[tokio::test(flavor = "current_thread")]
async fn rejects_invalid_launch_context_before_transport_use() {
    let mut context =
        session::LaunchContext::generate(String::new(), String::new(), 1).expect("test context");
    context.pipe_name = "invalid".into();
    let (client_stream, _server_stream) = duplex(1024);

    let result = CoreClient::connect(client_stream, &context, 0).await;

    assert_eq!(
        result.err().as_deref(),
        Some("authentication_failed: invalid launch context")
    );
}
