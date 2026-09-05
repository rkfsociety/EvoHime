//! Узкий отдельный endpoint листенера. Он не делит одно соединение shell:
//! desktop pipe обслуживает по одному клиенту, поэтому listener получает
//! собственный pipe и тот же owner-only ACL.
//!
//! Здесь же живёт трансляция состояния листенера в реестр Core и в durable
//! journal: состояние слушания имеет ровно один источник истины, и это не
//! Electron, а `AmbientListeningRegistry`.

use crate::{
    ambient::{AmbientDeviceInfo, AmbientUtteranceInput, ListenerControl},
    IpcBridge, StructuredLogger,
};
use evohime_listener_contract::{
    AmbientLogEvent, EngineStatus, EngineVersion, EpisodeId, ListeningReason, ListeningState,
};
use evohime_listener_ipc::{envelope, generated, read_frame, write_frame};
use std::sync::Arc;
use tokio::net::windows::named_pipe::ServerOptions;

/// Сколько команд листенеру помещается в очередь до того, как отправитель
/// начнёт ждать. Команд здесь единицы — это защита от утечки, а не буфер.
const CONTROL_QUEUE: usize = 16;

/// Сколько тишины считается концом эпизода.
///
/// Сообщения «эпизод кончился» в протоколе листенера нет: он присылает
/// высказывания и флаг продолжения. Границу поэтому проводит Core — по началу
/// нового эпизода, по этой паузе и по разрыву связи. Без такой границы
/// закрытие эпизода никогда бы не наступило, а вместе с ним не наступило бы и
/// ambient-извлечение, для которого оно и есть триггер.
const EPISODE_IDLE_MS: u64 = 60_000;

/// Как часто проверяется тишина. Ветка срабатывает только тогда, когда за это
/// время не пришло ни кадра, — то есть ровно в тишине.
const IDLE_POLL_SECONDS: u64 = 20;

/// Услышанная команда: разбор, карточка или запуск.
///
/// Запуск здесь возможен только при явно включённом автозапуске; во всех
/// прочих случаях появляется карточка, и приложение открывает клик. Это то же
/// правило, по которому проактивность 04.7 не имеет права вызвать инструмент:
/// микрофон не является подтверждением.
async fn handle_voice_command(
    bridge: &Arc<IpcBridge>,
    logger: &Arc<StructuredLogger>,
    policy: &evohime_listener_contract::AmbientPolicy,
    text: &str,
) {
    use crate::voice_command::{decide, Decision};
    use evohime_listener_contract::VoiceCommandState;

    let registry = bridge.voice_commands();
    let now_ms = now_ms();
    // Истёкшие карточки снимаются здесь же: панель узнаёт об этом событием, а
    // не тем, что карточка перестала приходить в списке.
    for expired in registry.expire(now_ms) {
        publish_voice_command(bridge, &expired, VoiceCommandState::Expired).await;
    }
    let command_id = uuid::Uuid::new_v4().to_string();
    let decision = decide(&registry, policy, text, now_ms, command_id);
    let (command, state) = match decision {
        Decision::Ignore => return,
        Decision::Confirm(command) => (command, VoiceCommandState::Pending),
        Decision::Autorun(command) => {
            let launch_registry = registry.clone();
            let launch_command = command.clone();
            let launched = match tokio::task::spawn_blocking(move || {
                crate::voice_command::launch(&launch_registry, &launch_command, now_ms)
            })
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    tracing::error!(%error, "listener voice command launch task failed");
                    Err("voice command launch task failed".to_owned())
                }
            };
            match launched {
                Ok(_) => (command, VoiceCommandState::Launched),
                Err(error) => {
                    let _ = logger.write(
                        "warn",
                        "listener.voice_command_failed",
                        serde_json::json!({"app_id": command.app_id, "error": error}),
                    );
                    (command, VoiceCommandState::Failed)
                }
            }
        }
    };
    let _ = logger.write(
        "info",
        "listener.voice_command",
        serde_json::json!({"app_id": command.app_id, "state": state.as_str()}),
    );
    publish_voice_command(bridge, &command, state).await;
}

async fn publish_voice_command(
    bridge: &Arc<IpcBridge>,
    command: &crate::voice_command::PendingCommand,
    state: evohime_listener_contract::VoiceCommandState,
) {
    let (Ok(command_id), Ok(app_id)) = (
        evohime_listener_contract::CommandId::new(command.command_id.clone()),
        evohime_listener_contract::AppId::new(command.app_id.clone()),
    ) else {
        return;
    };
    let _ = bridge
        .publish_ambient(&AmbientLogEvent::VoiceCommand {
            command_id,
            kind: command.kind,
            app_id,
            command_state: state,
        })
        .await;
}

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
        let data_dir = crate::ambient::data_dir();
        let ambient_policy = crate::ambient::load_policy(&data_dir);
        let control = crate::ambient::load_control(&data_dir);
        write_frame(
            &mut server,
            &envelope(generated::envelope::Payload::Policy(policy_update(
                &ambient_policy,
                &control,
            ))),
        )
        .await?;

        // Канал команд живёт ровно столько, сколько соединение: три точки
        // входа отправляют `SetAmbientListening`, ветка `ipc_bridge` кладёт
        // сюда одну команду, и другого пути к микрофону нет.
        let (control_tx, mut control_rx) = tokio::sync::mpsc::channel(CONTROL_QUEUE);
        let registry = bridge.ambient();
        registry.attach_control(control_tx).await;

        // Версия движка приходит от листенера и до неё эпизод открывать
        // нечем: `engine_version` эпизода — это то, чем он реально распознан,
        // а не заглушка.
        let mut engine_version = String::new();
        // Эпизод -> время последнего высказывания: по нему и определяется
        // тишина, закрывающая эпизод.
        let mut open_episodes: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        loop {
            tokio::select! {
                message = read_frame(&mut server) => {
                    let Ok(message) = message else { break };
                    match message.payload {
                        Some(generated::envelope::Payload::Engine(engine)) => {
                            if !engine.version.is_empty() {
                                engine_version = engine.version.clone();
                            }
                            let status = match engine.status.as_str() {
                                "approved" => EngineStatus::Approved,
                                "downloading" => EngineStatus::Downloading,
                                "verifying" => EngineStatus::Verifying,
                                "failed" => EngineStatus::Failed,
                                _ => EngineStatus::Idle,
                            };
                            registry
                                .set_engine(engine.version.clone(), status == EngineStatus::Approved)
                                .await;
                            let _ = bridge
                                .publish_ambient(&AmbientLogEvent::Engine {
                                    status,
                                    engine_version: EngineVersion::new(engine.version.clone()).ok(),
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
                            let parsed = parse_state(&state.state);
                            let reason = parse_reason(&state.reason);
                            let changed = registry
                                .set_state(parsed, reason, Some(state.device_id.clone()))
                                .await;
                            if changed {
                                publish_state(&bridge, parsed, reason, &state.device_id).await;
                            }
                            let _ = logger.write(
                                "info",
                                "listener.state_changed",
                                serde_json::json!({"state": state.state, "reason": state.reason}),
                            );
                        }
                        Some(generated::envelope::Payload::Devices(list)) => {
                            registry
                                .set_devices(
                                    list.devices
                                        .into_iter()
                                        .map(|device| AmbientDeviceInfo {
                                            device_id: device.device_id,
                                            display_name: device.display_name,
                                            is_default: device.is_default,
                                            is_active: false,
                                        })
                                        .collect(),
                                    list.active_device_id,
                                    list.watching,
                                )
                                .await;
                            // Список устройств не меняет состояние слушания,
                            // но панель обязана перечитать снимок: событие
                            // публикуется с текущим состоянием, а не с новым.
                            let snapshot = registry.snapshot().await;
                            publish_state(
                                &bridge,
                                snapshot.state,
                                snapshot.reason,
                                &snapshot.active_device_id,
                            )
                            .await;
                        }
                        Some(generated::envelope::Payload::Utterance(utterance)) => {
                            let policy = crate::ambient::load_policy(&data_dir);
                            let journal = bridge.journal();
                            // Разбор команды идёт до сохранения: дальше по
                            // конвейеру текст уходит в хранилище по значению,
                            // а команда обязана быть услышана в тот же момент,
                            // что и сказана.
                            handle_voice_command(&bridge, &logger, &policy, &utterance.text).await;
                            if !utterance.continued
                                && !open_episodes.contains_key(&utterance.episode_id)
                            {
                                // Начался новый эпизод — значит прежние
                                // кончились. Закрываются они здесь, а не
                                // молча забываются: закрытие и есть триггер
                                // ambient-извлечения.
                                let previous =
                                    open_episodes.keys().cloned().collect::<Vec<_>>();
                                for episode_id in previous {
                                    open_episodes.remove(&episode_id);
                                    close_episode(&bridge, &episode_id, utterance.started_at_ms)
                                        .await;
                                }
                                let _ = journal
                                    .open_ambient_episode(
                                        &utterance.episode_id,
                                        &engine_version,
                                        &engine_version,
                                        evohime_listener_contract::ExtractionState::Disabled,
                                        utterance.started_at_ms,
                                    )
                                    .await;
                                open_episodes.insert(
                                    utterance.episode_id.clone(),
                                    utterance.started_at_ms,
                                );
                            }
                            if let Some(last) = open_episodes.get_mut(&utterance.episode_id) {
                                *last = utterance.started_at_ms;
                            }
                            // Идентификатор высказывания строится из эпизода и
                            // его порядкового номера: время старта одного кадра
                            // может совпасть у двух высказываний, а пара
                            // «эпизод + номер» — нет.
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
                                    evohime_listener_contract::AmbientLimits::DEFAULT
                                        .dedup_window_ms,
                                )
                                .await;
                            if let (Ok(true), Ok(episode_id)) = (
                                stored,
                                EpisodeId::new(utterance.episode_id.clone()),
                            ) {
                                let _ = bridge
                                    .publish_ambient(&AmbientLogEvent::Transcript {
                                        episode_id,
                                        started_at_ms: utterance.started_at_ms,
                                        utterance_count: utterance.sequence.saturating_add(1),
                                        extraction_state:
                                            evohime_listener_contract::ExtractionState::Disabled,
                                    })
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
                command = control_rx.recv() => {
                    let Some(command) = command else { break };
                    if write_frame(&mut server, &control_frame(command)).await.is_err() {
                        break;
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(IDLE_POLL_SECONDS)),
                    if !open_episodes.is_empty() => {
                    let now = now_ms();
                    let idle = open_episodes
                        .iter()
                        .filter(|(_, last)| now.saturating_sub(**last) >= EPISODE_IDLE_MS)
                        .map(|(episode_id, _)| episode_id.clone())
                        .collect::<Vec<_>>();
                    for episode_id in idle {
                        open_episodes.remove(&episode_id);
                        close_episode(&bridge, &episode_id, now).await;
                    }
                }
            }
        }

        // Связь потеряна: продолжения у открытых эпизодов не будет, и
        // оставлять их незакрытыми значило бы потерять их для извлечения.
        let now = now_ms();
        for episode_id in open_episodes.keys().cloned().collect::<Vec<_>>() {
            open_episodes.remove(&episode_id);
            close_episode(&bridge, &episode_id, now).await;
        }

        // Связь потеряна. Реестр не имеет права остаться на «слушаю»: это
        // ровно тот случай, когда индикатор обязан сказать, что состояние
        // неизвестно, а не изображать работающий микрофон.
        registry.detach_control().await;
        let snapshot = registry.snapshot().await;
        publish_state(
            &bridge,
            snapshot.state,
            snapshot.reason,
            &snapshot.active_device_id,
        )
        .await;
        let _ = logger.write(
            "warn",
            "listener.disconnected",
            serde_json::json!({"role":"listener"}),
        );
    }
}

/// Закрывает эпизод и отдаёт его в ambient-извлечение (04.6).
///
/// Порядок важен: сначала эпизод получает `ended_at`, и только потом его
/// разбирают. Решает ли Core вообще что-то извлекать — вопрос режимов и
/// бюджетов, и он решается там, а не здесь.
async fn close_episode(bridge: &IpcBridge, episode_id: &str, now_ms: u64) {
    let _ = bridge
        .journal()
        .close_ambient_episode(episode_id, now_ms)
        .await;
    bridge.request_ambient_extraction(episode_id).await;
}

/// Собирает `PolicyUpdate` из политики 04.1 и намерения пользователя.
fn policy_update(
    policy: &evohime_listener_contract::AmbientPolicy,
    control: &crate::ambient::AmbientControl,
) -> generated::PolicyUpdate {
    generated::PolicyUpdate {
        paused: policy.paused,
        process_blocklist: policy.process_blocklist.clone(),
        window_title_blocklist: policy.window_title_blocklist.clone(),
        quiet_start: policy
            .quiet_hours
            .iter()
            .map(|window| window.start_minute)
            .collect(),
        quiet_end: policy
            .quiet_hours
            .iter()
            .map(|window| window.end_minute)
            .collect(),
        enabled: control.enabled,
        device_id: control.device_id.clone(),
    }
}

fn control_frame(command: ListenerControl) -> generated::Envelope {
    use generated::local_command::Command;
    let command = match command {
        ListenerControl::Enabled(enabled) => Command::Enabled(enabled),
        ListenerControl::Paused(paused) => Command::Pause(paused),
        ListenerControl::SelectDevice(device_id) => Command::SelectDevice(device_id),
        ListenerControl::ResetBuffers => Command::ResetBuffers(true),
        ListenerControl::Policy(update) => {
            let (policy, control) = *update;
            return envelope(generated::envelope::Payload::Policy(policy_update(
                &policy, &control,
            )));
        }
    };
    envelope(generated::envelope::Payload::Command(
        generated::LocalCommand {
            command: Some(command),
        },
    ))
}

/// Публикует `ambient.state` в durable journal и будит push к оболочке.
async fn publish_state(
    bridge: &IpcBridge,
    state: ListeningState,
    reason: ListeningReason,
    device_id: &str,
) {
    let _ = bridge
        .publish_ambient(&AmbientLogEvent::State {
            state,
            reason,
            active_device_id: evohime_listener_contract::DeviceId::new(device_id.to_owned()).ok(),
        })
        .await;
}

/// Разбирает состояние листенера. Неизвестное значение не превращается в
/// «выключено»: неизвестность — это отказ связи, и она называется своим
/// состоянием.
fn parse_state(value: &str) -> ListeningState {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .unwrap_or(ListeningState::EngineUnavailable)
}

fn parse_reason(value: &str) -> ListeningReason {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .unwrap_or(ListeningReason::Unknown)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}
