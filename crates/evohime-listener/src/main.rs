use evohime_listener::{
    backoff, data_dir, engine::EngineUnavailable, tools_dir, EngineNotice, ListenerRuntime,
    NullEngine, SpeechEngine,
};
use evohime_listener_contract::{
    AmbientLimits, AmbientPolicy, ListeningReason, ListeningState, QuietHours,
};
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

/// Как часто пересматривается состояние без внешнего повода.
///
/// Тихие часы наступают по календарю, а не по команде: без этого тика
/// слушание вошло бы в окно тишины и осталось бы в нём открытым до
/// следующего сообщения от Core.
#[cfg(windows)]
const POLICY_TICK_SECONDS: u64 = 20;

#[cfg(windows)]
#[tokio::main]
async fn main() {
    evohime_listener::harden_process();
    let (tx, _rx) = tokio::sync::watch::channel(ListeningState::Stopped);
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
    ///
    /// Пустой `device_id` означает «устройство системы по умолчанию».
    /// Непустой открывается именно им: подмена на умолчание при пропаже
    /// выбранного микрофона означала бы слушать не тем устройством, которое
    /// выбрал пользователь.
    fn start(
        device_id: String,
        frames: tokio::sync::mpsc::Sender<Vec<f32>>,
    ) -> Result<Self, evohime_listener_audio::AudioError> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            let opened = evohime_listener_audio::open_capture(&device_id, |format| {
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

    // Подписка на приход и уход устройств. Отказ подписки не выдумывает
    // «список неизменен»: он уезжает в `watching=false`, и панель говорит,
    // что список надо обновить вручную.
    let (device_tx, mut device_rx) = tokio::sync::mpsc::channel::<()>(1);
    let watcher = evohime_listener_audio::DeviceWatcher::start(move || {
        let _ = device_tx.try_send(());
    });
    let watching = watcher.is_ok();
    let _watcher = watcher.ok();
    let mut devices = evohime_listener_audio::list_capture_devices().unwrap_or_default();

    let mut tick = tokio::time::interval(std::time::Duration::from_secs(POLICY_TICK_SECONDS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

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
                        apply_policy_update(runtime, policy);
                        if !announced {
                            announced = true;
                            report_engine(&mut stream, runtime, engine_error).await?;
                            publish_devices(&mut stream, runtime, &mut devices, watching).await?;
                            announce_state(&mut stream, runtime).await?;
                        }
                        reconcile(&mut stream, runtime, &mut capture, &frames_tx, engine_error, &devices)
                            .await?;
                    }
                    Some(generated::envelope::Payload::Command(command)) => {
                        match command.command {
                            Some(generated::local_command::Command::ResetBuffers(true)) => {
                                runtime.reset_buffers();
                            }
                            Some(generated::local_command::Command::Pause(paused)) => {
                                runtime.policy.paused = paused;
                            }
                            Some(generated::local_command::Command::Enabled(enabled)) => {
                                runtime.enabled = enabled;
                            }
                            Some(generated::local_command::Command::SelectDevice(device_id))
                                if device_id != runtime.device_id =>
                            {
                                runtime.device_id = device_id;
                                // Устройство меняется без перезапуска
                                // процесса: прежний поток закрывается, новый
                                // открывается в `reconcile`.
                                capture = None;
                            }
                            _ => {}
                        }
                        reconcile(&mut stream, runtime, &mut capture, &frames_tx, engine_error, &devices)
                            .await?;
                    }
                    _ => {}
                }
            }
            Some(()) = device_rx.recv() => {
                publish_devices(&mut stream, runtime, &mut devices, watching).await?;
                reconcile(&mut stream, runtime, &mut capture, &frames_tx, engine_error, &devices)
                    .await?;
            }
            _ = tick.tick() => {
                reconcile(&mut stream, runtime, &mut capture, &frames_tx, engine_error, &devices)
                    .await?;
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

/// Переносит присланную политику в рантайм.
///
/// Валидация уже прошла в Core: сюда политика приезжает только сохранённой,
/// поэтому нечисловые окна отбрасываются молча, а не роняют соединение.
#[cfg(windows)]
fn apply_policy_update(runtime: &mut ListenerRuntime, policy: generated::PolicyUpdate) {
    runtime.policy.paused = policy.paused;
    runtime.policy.process_blocklist = policy.process_blocklist;
    runtime.policy.window_title_blocklist = policy.window_title_blocklist;
    runtime.policy.quiet_hours = policy
        .quiet_start
        .iter()
        .zip(policy.quiet_end.iter())
        .filter_map(|(start, end)| QuietHours::new(*start, *end).ok())
        .collect();
    runtime.enabled = policy.enabled;
    runtime.device_id = policy.device_id;
}

/// Перечисляет устройства заново и отправляет снимок в Core.
#[cfg(windows)]
async fn publish_devices(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &ListenerRuntime,
    devices: &mut Vec<evohime_listener_audio::CaptureDevice>,
    watching: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    *devices = evohime_listener_audio::list_capture_devices().unwrap_or_default();
    write_frame(
        stream,
        &envelope(generated::envelope::Payload::Devices(
            generated::DeviceList {
                devices: devices
                    .iter()
                    .map(|device| generated::CaptureDevice {
                        device_id: device.id.clone(),
                        display_name: device.display_name.clone(),
                        is_default: device.is_default,
                    })
                    .collect(),
                active_device_id: runtime.device_id.clone(),
                watching,
            },
        )),
    )
    .await?;
    Ok(())
}

/// Приводит поток захвата в соответствие с желаемым состоянием.
///
/// Пауза, тихие часы и выключение именно закрывают устройство:
/// `ListeningState::is_capturing` обещает, что микрофон читается только в
/// `Listening`, и фильтрация кадров после чтения этого обещания не выполняет.
#[cfg(windows)]
async fn reconcile(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &mut ListenerRuntime,
    capture: &mut Option<Capture>,
    frames: &tokio::sync::mpsc::Sender<Vec<f32>>,
    engine_error: Option<EngineUnavailable>,
    devices: &[evohime_listener_audio::CaptureDevice],
) -> Result<(), Box<dyn std::error::Error>> {
    let (target, reason) = runtime.desired_state(minute_of_day(), engine_error.is_none());
    if target != ListeningState::Listening {
        *capture = None;
        set_state(stream, runtime, target, reason).await?;
        return Ok(());
    }
    if capture.is_some() {
        return Ok(());
    }
    // `Listening` достижимо только через `Starting`, и отказ устройства тоже
    // объявляется из него: это единственное состояние, из которого контракт
    // разрешает уйти в `DeviceDisconnected`.
    set_state(stream, runtime, ListeningState::Starting, reason).await?;
    let selected_is_gone = !runtime.device_id.is_empty()
        && !devices.iter().any(|device| device.id == runtime.device_id);
    if selected_is_gone || (runtime.device_id.is_empty() && devices.is_empty()) {
        set_state(
            stream,
            runtime,
            ListeningState::DeviceDisconnected,
            ListeningReason::DeviceDisconnected,
        )
        .await?;
        return Ok(());
    }
    match Capture::start(runtime.device_id.clone(), frames.clone()) {
        Ok(started) => {
            *capture = Some(started);
            set_state(stream, runtime, ListeningState::Listening, reason).await?;
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
            runtime.last_reason = ListeningReason::EngineDegraded;
            write_frame(
                stream,
                &envelope(generated::envelope::Payload::State(
                    generated::StateChanged {
                        state: "paused_by_policy".into(),
                        reason: "engine_degraded".into(),
                        device_id: runtime.device_id.clone(),
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

/// Объявляет Core текущее состояние при подключении.
///
/// Это не переход, а снимок: у Core состояние по умолчанию —
/// `EngineUnavailable` («листенер не на связи»), и без такого объявления оно
/// остаётся таким до первой настоящей смены состояния. Выключенный листенер с
/// открытым движком не менял бы состояние никогда, и панель показывала бы
/// «нет связи» при живом соединении. `set_state` здесь не годится: повтор
/// текущего состояния он подавляет по контракту.
#[cfg(windows)]
async fn announce_state(
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: &ListenerRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = serde_json::to_value(runtime.state)?;
    let reason = serde_json::to_value(runtime.last_reason)?;
    write_frame(
        stream,
        &envelope(generated::envelope::Payload::State(
            generated::StateChanged {
                state: state.as_str().unwrap_or_default().to_owned(),
                reason: reason.as_str().unwrap_or_default().to_owned(),
                device_id: runtime.device_id.clone(),
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
    runtime.last_reason = reason;
    let device_id = runtime.device_id.clone();
    let state = serde_json::to_value(state)?;
    let reason = serde_json::to_value(reason)?;
    write_frame(
        stream,
        &envelope(generated::envelope::Payload::State(
            generated::StateChanged {
                state: state.as_str().unwrap_or_default().to_owned(),
                reason: reason.as_str().unwrap_or_default().to_owned(),
                device_id,
            },
        )),
    )
    .await?;
    Ok(())
}

/// Локальная минута суток для тихих часов.
///
/// Часовой пояс берётся системный: пользователь задаёт тишину «с 23 до 7» по
/// своим часам, а не по UTC.
#[cfg(windows)]
fn minute_of_day() -> u32 {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut now = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut now) };
    u32::from(now.wHour) * 60 + u32::from(now.wMinute)
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
