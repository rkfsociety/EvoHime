use evohime_listener::{
    backoff, data_dir, engine::EngineUnavailable, tools_dir, EngineNotice, ListenerRuntime,
    NullEngine, SpeechEngine,
};
use evohime_listener_contract::{AmbientLimits, AmbientPolicy, ListeningReason, ListeningState};
use evohime_listener_ipc::{envelope, generated, read_frame, write_frame};
use tokio::time::sleep;

/// Кадр 16 кГц моно, который получает VAD.
#[cfg(windows)]
const FRAME_SAMPLES: usize = (AmbientLimits::DEFAULT.frame_ms as usize) * 16_000 / 1000;

/// Сколько кадров держится в очереди между callback устройства и обработкой.
///
/// Очередь bounded специально: если распознавание отстаёт, лишний звук надо
/// выбросить, а не копить, — иначе процесс растёт в памяти ровно там, где
/// хранится сырая речь.
#[cfg(windows)]
const FRAME_QUEUE: usize = 64;

#[cfg(windows)]
#[tokio::main]
async fn main() {
    evohime_listener::harden_process();
    let (tx, _rx) = tokio::sync::watch::channel(ListeningState::PausedByPolicy);
    // Движок выбирается один раз на процесс: набор рантайма не меняется, пока
    // оболочка не скачает новый и не перезапустит листенер.
    let (engine, engine_error) = open_engine();
    let mut runtime = ListenerRuntime::new(
        AmbientPolicy {
            paused: true,
            ..Default::default()
        },
        engine,
        tx,
    );
    let mut attempt = 0;
    loop {
        match run_connection(&mut runtime, engine_error).await {
            Ok(()) => attempt = 0,
            Err(error) => {
                log_error(&error.to_string());
                sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

/// Открывает движок по проверенному каталогу инструментов.
///
/// Отказ не останавливает процесс: листенер поднимается с `NullEngine`,
/// сообщает Core причину и остаётся управляемым. Молчащий процесс без
/// объяснения был бы неотличим от сломанного.
#[cfg(windows)]
fn open_engine() -> (Box<dyn SpeechEngine>, Option<EngineUnavailable>) {
    match tools_dir::resolve(&tools_dir::ProcessEnv) {
        Ok(runtime) => {
            match evohime_listener::engine::whisper_dll::WhisperDllEngine::open(&runtime) {
                Ok(engine) => (Box::new(engine), None),
                Err(reason) => (Box::new(NullEngine::new(reason)), Some(reason)),
            }
        }
        Err(reason) => (Box::new(NullEngine::new(reason)), Some(reason)),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("evohime-listener is a Windows capture process");
}

/// Живой поток захвата. Хранится, чтобы пауза закрывала устройство, а не
/// фильтровала кадры уже после чтения микрофона.
#[cfg(windows)]
struct Capture {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl Capture {
    /// Поднимает поток захвата: `cpal::Stream` не `Send`, поэтому живёт в
    /// собственном потоке и закрывается вместе с ним.
    fn start(
        frames: tokio::sync::mpsc::Sender<Vec<f32>>,
    ) -> Result<Self, evohime_listener_audio::AudioError> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            let opened = evohime_listener_audio::open_default_capture(|format| {
                let mut pending: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES * 4);
                move |data: &[f32]| {
                    let mono = evohime_listener_audio::downmix_to_mono(data, format.channels);
                    let Ok(resampled) =
                        evohime_listener_audio::resample_to_16khz(&mono, format.sample_rate)
                    else {
                        // Частота, которую нельзя децимировать без интерполяции:
                        // лучше не отдать кадр, чем отдать искажённый.
                        return;
                    };
                    pending.extend_from_slice(&resampled);
                    while pending.len() >= FRAME_SAMPLES {
                        let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
                        // Переполненная очередь означает, что распознавание не
                        // успевает: кадр выбрасывается здесь, а не копится.
                        let _ = frames.try_send(frame);
                    }
                }
            });
            match opened {
                Ok((stream, actual)) => {
                    let _ = ready_tx.send(Ok(actual));
                    while !thread_stop.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    drop(stream);
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            }
        });
        match ready_rx.recv() {
            Ok(Ok(_)) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(evohime_listener_audio::AudioError::DeviceUnavailable(
                "capture thread stopped before it opened a device".into(),
            )),
        }
    }
}

#[cfg(windows)]
impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(windows)]
async fn run_connection(
    runtime: &mut ListenerRuntime,
    engine_error: Option<EngineUnavailable>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let context_path = std::env::var("EVOHIME_LAUNCH_CONTEXT")?;
    let context =
        evohime_desktop_ipc::session::read_launch_context(std::path::Path::new(&context_path))?;
    let pipe = std::env::var("EVOHIME_LISTENER_PIPE")?;
    let mut stream = ClientOptions::new().open(&pipe)?;
    let hello = generated::Hello {
        protocol_major: 1,
        client_id: format!("listener-{}", std::process::id()),
        role: "listener".into(),
    };
    let client_id = hello.client_id.clone();
    write_frame(
        &mut stream,
        &envelope(generated::envelope::Payload::Hello(hello)),
    )
    .await?;

    let (frames_tx, mut frames_rx) = tokio::sync::mpsc::channel::<Vec<f32>>(FRAME_QUEUE);
    let mut capture: Option<Capture> = None;
    let mut announced = false;

    loop {
        tokio::select! {
            message = read_frame(&mut stream) => {
                let message = message?;
                match message.payload {
                    Some(generated::envelope::Payload::Handshake(challenge)) => {
                        let proof = context
                            .secret
                            .proof("listener", &client_id, &challenge.nonce);
                        write_frame(
                            &mut stream,
                            &envelope(generated::envelope::Payload::Handshake(
                                generated::Handshake {
                                    nonce: challenge.nonce,
                                    proof,
                                },
                            )),
                        )
                        .await?;
                    }
                    Some(generated::envelope::Payload::Policy(policy)) => {
                        runtime.policy.paused = policy.paused;
                        runtime.policy.process_blocklist = policy.process_blocklist;
                        runtime.policy.window_title_blocklist = policy.window_title_blocklist;
                        if !announced {
                            announced = true;
                            report_engine(&mut stream, runtime, engine_error).await?;
                        }
                        apply_policy(&mut stream, runtime, &mut capture, &frames_tx, engine_error)
                            .await?;
                    }
                    Some(generated::envelope::Payload::Command(command)) => {
                        match command.command {
                            Some(generated::local_command::Command::ResetBuffers(true)) => {
                                runtime.reset_buffers();
                            }
                            Some(generated::local_command::Command::Pause(paused)) => {
                                runtime.policy.paused = paused;
                                apply_policy(
                                    &mut stream,
                                    runtime,
                                    &mut capture,
                                    &frames_tx,
                                    engine_error,
                                )
                                .await?;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            frame = frames_rx.recv() => {
                let Some(frame) = frame else { continue };
                for utterance in runtime.process_frame(&frame, now_ms()) {
                    write_frame(
                        &mut stream,
                        &envelope(generated::envelope::Payload::Utterance(
                            generated::UtteranceRecognized {
                                episode_id: utterance.episode_id,
                                text: utterance.text,
                                started_at_ms: utterance.started_at_ms,
                                continued: utterance.continued,
                                sequence: utterance.sequence,
                                duration_ms: utterance.duration_ms,
                                language: utterance.language,
                            },
                        )),
                    )
                    .await?;
                }
                for notice in runtime.take_notices() {
                    publish_notice(&mut stream, runtime, notice, &mut capture).await?;
                }
            }
        }
    }
}

/// Открывает или закрывает устройство по текущей политике.
///
/// Пауза именно закрывает поток: `ListeningState::is_capturing` обещает, что
/// микрофон читается только в `Listening`, и фильтрация кадров после чтения
/// этого обещания не выполняет.
#[cfg(windows)]
async fn apply_policy(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &mut ListenerRuntime,
    capture: &mut Option<Capture>,
    frames: &tokio::sync::mpsc::Sender<Vec<f32>>,
    engine_error: Option<EngineUnavailable>,
) -> Result<(), Box<dyn std::error::Error>> {
    if engine_error.is_some() {
        // Без движка микрофон не открывается вовсе: записывать нечего, а
        // держать устройство занятым — значит мешать другим приложениям.
        *capture = None;
        set_state(
            stream,
            runtime,
            ListeningState::EngineUnavailable,
            ListeningReason::EngineUnavailable,
        )
        .await?;
        return Ok(());
    }
    if runtime.policy.paused {
        *capture = None;
        set_state(
            stream,
            runtime,
            ListeningState::PausedByPolicy,
            ListeningReason::QuietHours,
        )
        .await?;
        return Ok(());
    }
    if capture.is_none() {
        set_state(
            stream,
            runtime,
            ListeningState::Starting,
            ListeningReason::UserRequest,
        )
        .await?;
        match Capture::start(frames.clone()) {
            Ok(started) => {
                *capture = Some(started);
                set_state(
                    stream,
                    runtime,
                    ListeningState::Listening,
                    ListeningReason::UserRequest,
                )
                .await?;
            }
            Err(_) => {
                set_state(
                    stream,
                    runtime,
                    ListeningState::DeviceDisconnected,
                    ListeningReason::DeviceDisconnected,
                )
                .await?;
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn publish_notice(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &mut ListenerRuntime,
    notice: EngineNotice,
    capture: &mut Option<Capture>,
) -> Result<(), Box<dyn std::error::Error>> {
    match notice {
        EngineNotice::RungChanged(_) => {
            write_frame(
                stream,
                &envelope(generated::envelope::Payload::Engine(
                    generated::EngineStatus {
                        status: "approved".into(),
                        version: runtime.engine_version().to_owned(),
                        code: String::new(),
                    },
                )),
            )
            .await?;
        }
        EngineNotice::Degraded => {
            // Лестница исчерпана: слушание остановлено, устройство закрыто.
            *capture = None;
            let _ = runtime.set_state(ListeningState::PausedByPolicy);
            write_frame(
                stream,
                &envelope(generated::envelope::Payload::State(
                    generated::StateChanged {
                        state: "paused_by_policy".into(),
                        reason: "engine_degraded".into(),
                        device_id: String::new(),
                    },
                )),
            )
            .await?;
        }
        EngineNotice::Unavailable(reason) => {
            *capture = None;
            write_frame(
                stream,
                &envelope(generated::envelope::Payload::Engine(
                    generated::EngineStatus {
                        status: "failed".into(),
                        version: runtime.engine_version().to_owned(),
                        code: reason.as_str().into(),
                    },
                )),
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn report_engine(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &ListenerRuntime,
    engine_error: Option<EngineUnavailable>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (status, code) = match engine_error {
        Some(reason) => ("failed", reason.as_str()),
        None => ("approved", ""),
    };
    write_frame(
        stream,
        &envelope(generated::envelope::Payload::Engine(
            generated::EngineStatus {
                status: status.into(),
                version: runtime.engine_version().to_owned(),
                code: code.into(),
            },
        )),
    )
    .await?;
    Ok(())
}

#[cfg(windows)]
async fn set_state(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &mut ListenerRuntime,
    state: ListeningState,
    reason: ListeningReason,
) -> Result<(), Box<dyn std::error::Error>> {
    if runtime.state == state {
        // Повтор состояния не является изменением: контракт запрещает
        // самопереход, и публиковать его нельзя.
        return Ok(());
    }
    if runtime.set_state(state).is_err() {
        return Ok(());
    }
    let state = serde_json::to_value(state)?;
    let reason = serde_json::to_value(reason)?;
    write_frame(
        stream,
        &envelope(generated::envelope::Payload::State(
            generated::StateChanged {
                state: state.as_str().unwrap_or_default().to_owned(),
                reason: reason.as_str().unwrap_or_default().to_owned(),
                device_id: String::new(),
            },
        )),
    )
    .await?;
    Ok(())
}

#[cfg(windows)]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

fn log_error(error: &str) {
    let path = data_dir().join("logs").join("listener.jsonl");
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let line = serde_json::json!({"event":"listener.connection_failed","code":"core_unavailable","error":error});
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, format!("{}\n", line).as_bytes())
        });
}
