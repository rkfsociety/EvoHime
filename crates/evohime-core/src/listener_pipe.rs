//! Узкий отдельный endpoint листенера. Он не делит одно соединение shell:
//! desktop pipe обслуживает по одному клиенту, поэтому listener получает
//! собственный pipe и тот же owner-only ACL.

use crate::{ambient::AmbientUtteranceInput, IpcBridge, StructuredLogger};
use evohime_listener_ipc::{envelope, generated, read_frame, write_frame};
use std::sync::Arc;
use tokio::net::windows::named_pipe::ServerOptions;

pub async fn run_windows_listener_pipe(
    context: evohime_desktop_ipc::session::LaunchContext,
    bridge: Arc<IpcBridge>,
    logger: Arc<StructuredLogger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pipe_name = format!("{}-listener", context.pipe_name);
    evohime_desktop_ipc::session::validate_pipe_name(&pipe_name)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let sid = evohime_desktop_ipc::windows_security::current_user_sid()?;
    loop {
        let mut security = evohime_desktop_ipc::windows_security::PipeSecurity::owner_only(&sid)?;
        let mut server = unsafe {
            ServerOptions::new().create_with_security_attributes_raw(&pipe_name, security.as_raw())
        }?;
        server.connect().await?;
        let _ = logger.write(
            "info",
            "listener.connected",
            serde_json::json!({"role":"listener"}),
        );
        let hello = read_frame(&mut server).await?;
        let Some(generated::envelope::Payload::Hello(hello)) = hello.payload else {
            continue;
        };
        let mut verifier = evohime_desktop_ipc::session::HandshakeVerifier::new(
            context.clone(),
            evohime_desktop_ipc::session::DEFAULT_NONCE_TTL_MS,
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        let nonce = verifier
            .issue_nonce(now_ms())
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        write_frame(
            &mut server,
            &envelope(generated::envelope::Payload::Handshake(
                generated::Handshake {
                    nonce: nonce.value.clone(),
                    proof: String::new(),
                },
            )),
        )
        .await?;
        let response = read_frame(&mut server).await?;
        let Some(generated::envelope::Payload::Handshake(response)) = response.payload else {
            continue;
        };
        let peer = evohime_desktop_ipc::session::PeerIdentity {
            user_sid: evohime_desktop_ipc::windows_security::current_user_sid()?,
            logon_session: evohime_desktop_ipc::windows_security::current_logon_session()?,
        };
        let request = evohime_desktop_ipc::session::HandshakeRequest {
            protocol_major: hello.protocol_major,
            client_id: hello.client_id,
            client_role: hello.role,
            nonce: response.nonce,
            proof: response.proof,
            capabilities: Vec::new(),
            peer,
        };
        if verifier.verify(&request, now_ms()).is_err() {
            continue;
        }
        let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|p| std::path::PathBuf::from(p).join("EvoHime"))
            })
            .unwrap_or_else(|| std::path::PathBuf::from(".evohime"));
        let ambient_policy = crate::ambient::load_policy(&data_dir);
        let policy = generated::PolicyUpdate {
            paused: ambient_policy.paused,
            process_blocklist: ambient_policy.process_blocklist,
            window_title_blocklist: ambient_policy.window_title_blocklist,
        };
        write_frame(
            &mut server,
            &envelope(generated::envelope::Payload::Policy(policy)),
        )
        .await?;

        // Версия движка приходит от листенера и до неё эпизод открывать
        // нечем: `engine_version` эпизода — это то, чем он реально распознан,
        // а не заглушка.
        let mut engine_version = String::new();
        let mut open_episodes: std::collections::HashSet<String> = std::collections::HashSet::new();
        while let Ok(message) = read_frame(&mut server).await {
            match message.payload {
                Some(generated::envelope::Payload::Engine(engine)) => {
                    if !engine.version.is_empty() {
                        engine_version = engine.version.clone();
                    }
                    let status = match engine.status.as_str() {
                        "approved" => evohime_listener_contract::EngineStatus::Approved,
                        "downloading" => evohime_listener_contract::EngineStatus::Downloading,
                        "verifying" => evohime_listener_contract::EngineStatus::Verifying,
                        "failed" => evohime_listener_contract::EngineStatus::Failed,
                        _ => evohime_listener_contract::EngineStatus::Idle,
                    };
                    let journal = bridge.journal();
                    let _ = journal
                        .append_ambient_event(&evohime_listener_contract::AmbientLogEvent::Engine {
                            status,
                            engine_version: evohime_listener_contract::EngineVersion::new(
                                engine.version.clone(),
                            )
                            .ok(),
                            progress_pct: None,
                        })
                        .await;
                    let _ = logger.write(
                        "info",
                        "listener.engine_status",
                        serde_json::json!({"status": engine.status, "code": engine.code}),
                    );
                }
                Some(generated::envelope::Payload::State(state)) => {
                    let _ = logger.write(
                        "info",
                        "listener.state_changed",
                        serde_json::json!({"state": state.state, "reason": state.reason}),
                    );
                }
                Some(generated::envelope::Payload::Utterance(utterance)) => {
                    let policy = crate::ambient::load_policy(&data_dir);
                    let journal = bridge.journal();
                    if !utterance.continued && open_episodes.insert(utterance.episode_id.clone()) {
                        let _ = journal
                            .open_ambient_episode(
                                &utterance.episode_id,
                                &engine_version,
                                &engine_version,
                                evohime_listener_contract::ExtractionState::Disabled,
                                utterance.started_at_ms,
                            )
                            .await;
                    }
                    // Идентификатор высказывания строится из эпизода и его
                    // порядкового номера: время старта одного кадра может
                    // совпасть у двух высказываний, а пара «эпизод + номер» —
                    // нет.
                    let stored = journal
                        .insert_ambient_utterance(
                            &AmbientUtteranceInput {
                                utterance_id: format!(
                                    "{}-{}",
                                    utterance.episode_id, utterance.sequence
                                ),
                                episode_id: utterance.episode_id.clone(),
                                sequence: i64::from(utterance.sequence),
                                started_at_ms: utterance.started_at_ms,
                                duration_ms: i64::from(utterance.duration_ms),
                                text: utterance.text,
                                language: if utterance.language.is_empty() {
                                    "und".into()
                                } else {
                                    utterance.language
                                },
                                avg_logprob: 0.0,
                                redacted: false,
                            },
                            policy.retention_days,
                            evohime_listener_contract::AmbientLimits::DEFAULT.dedup_window_ms,
                        )
                        .await;
                    if let (Ok(true), Ok(episode_id)) = (
                        stored,
                        evohime_listener_contract::EpisodeId::new(utterance.episode_id.clone()),
                    ) {
                        let _ = journal
                            .append_ambient_event(
                                &evohime_listener_contract::AmbientLogEvent::Transcript {
                                    episode_id,
                                    started_at_ms: utterance.started_at_ms,
                                    utterance_count: utterance.sequence.saturating_add(1),
                                    extraction_state:
                                        evohime_listener_contract::ExtractionState::Disabled,
                                },
                            )
                            .await;
                    }
                    let _ = logger.write(
                        "info",
                        "listener.utterance_received",
                        serde_json::json!({"episode_id": utterance.episode_id}),
                    );
                }
                _ => {}
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}
