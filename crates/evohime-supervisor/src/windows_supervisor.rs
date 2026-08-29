use crate::runtime_loop::{SupervisorRuntime, TickEvent};
use serde_json::json;
use std::{
    ffi::OsStr,
    fs, io,
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr,
    sync::Mutex,
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};

use std::path::Path;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::ServerOptions,
    process::Command,
    time::{sleep, Duration},
};

use evohime_desktop_ipc::session::LaunchContext;
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectCpuRateControlInformation,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
        JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    },
    System::Threading::{CreateEventW, CreateMutexW},
};

struct SingleInstance(HANDLE);

struct SupervisorLiveness(HANDLE);

impl Drop for SupervisorLiveness {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

struct SupervisorLogger(Mutex<std::io::BufWriter<std::fs::File>>);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Turns a bounded-contract tick event into the (event, fields) shape used by
/// `SupervisorLogger::write`, matching the `core.*` structured event style used
/// elsewhere in this crate.
fn tick_event_log(event: &TickEvent) -> (&'static str, serde_json::Value) {
    match event {
        TickEvent::LifecycleTransition { from, to } => (
            "runtime.lifecycle_transition",
            json!({"from": format!("{from:?}"), "to": format!("{to:?}")}),
        ),
        TickEvent::LeaseAcquired => ("runtime.lease_acquired", json!({})),
        TickEvent::LeaseRenewed => ("runtime.lease_renewed", json!({})),
        TickEvent::LeaseLost => ("runtime.lease_lost", json!({})),
        TickEvent::HeartbeatRecorded { sequence } => {
            ("runtime.heartbeat_recorded", json!({"sequence": sequence}))
        }
        TickEvent::RecoveryDecision(decision) => (
            "runtime.recovery_decision",
            json!({"decision": format!("{decision:?}")}),
        ),
        TickEvent::RetryScheduled {
            attempts,
            next_attempt_at_ms,
        } => (
            "runtime.retry_scheduled",
            json!({"attempts": attempts, "next_attempt_at_ms": next_attempt_at_ms}),
        ),
        TickEvent::RetryExhausted => ("runtime.retry_exhausted", json!({})),
        TickEvent::TriggerDecision(decision) => (
            "runtime.trigger_decision",
            json!({"decision": format!("{decision:?}")}),
        ),
        TickEvent::ScheduleCompleted { next_run_at_ms } => (
            "runtime.schedule_completed",
            json!({"next_run_at_ms": next_run_at_ms}),
        ),
        TickEvent::ScheduleFailed(decision) => (
            "runtime.schedule_failed",
            json!({"decision": format!("{decision:?}")}),
        ),
        TickEvent::ScheduleDeadLetter => ("runtime.schedule_dead_letter", json!({})),
        TickEvent::ScheduleRequeued => ("runtime.schedule_requeued", json!({})),
    }
}

fn log_runtime_events(logger: &SupervisorLogger, events: &[TickEvent]) {
    for event in events {
        let (name, fields) = tick_event_log(event);
        let _ = logger.write(name, fields);
    }
}

impl SupervisorLogger {
    fn open() -> io::Result<Self> {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".evohime"))
            .join("EvoHime/logs");
        std::fs::create_dir_all(&dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("supervisor.jsonl"))?;
        Ok(Self(Mutex::new(std::io::BufWriter::new(file))))
    }

    fn write(&self, event: &str, fields: serde_json::Value) -> io::Result<()> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut file = self
            .0
            .lock()
            .map_err(|_| io::Error::other("supervisor logger lock poisoned"))?;
        serde_json::to_writer(
            &mut *file,
            &json!({
                "timestamp_ms": timestamp_ms,
                "level": "info",
                "event": event,
                "fields": fields,
            }),
        )?;
        use std::io::Write;
        file.write_all(b"\n")?;
        file.flush()
    }
}

impl SingleInstance {
    fn acquire(name: &str) -> io::Result<Self> {
        let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(Some(0)).collect();
        let handle = unsafe { CreateMutexW(ptr::null(), 1, wide.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "EvoHime is already running",
            ));
        }
        Ok(Self(handle))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub(crate) struct JobObject(HANDLE);

// A Job Object handle is an owned kernel handle. Moving ownership between
// Tokio worker threads is safe; Drop remains the single close point.
unsafe impl Send for JobObject {}
unsafe impl Sync for JobObject {}

pub fn recover_pending_update(state_dir: &Path) -> io::Result<bool> {
    Ok(evohime_tx::UpdateTransaction::recover(state_dir)?.recovered)
}

fn is_deferred_update_recovery_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33))
}

fn update_state_dir() -> PathBuf {
    std::env::var_os("EVOHIME_UPDATE_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|path| path.join("EvoHime/update-state"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime/update-state"))
}

impl JobObject {
    pub(crate) fn create() -> io::Result<Self> {
        Self::create_with_limits(None, None)
    }

    pub(crate) fn create_with_limits(
        memory_bytes: Option<u64>,
        cpu_percent: Option<u8>,
    ) -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // `BREAKAWAY_OK` — не ослабление `KILL_ON_JOB_CLOSE`, а условие для
        // одного явного случая: приложение, которое Ева открыла по просьбе
        // пользователя, принадлежит пользователю и обязано пережить перезапуск
        // Core. Отвязка возможна только по явному флагу `CreateProcess`
        // (`SILENT_BREAKAWAY` здесь не выставлен), поэтому дерево процессов
        // самого Core остаётся в job и по-прежнему умирает вместе с ним.
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        if let Some(memory) = memory_bytes {
            limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            limits.ProcessMemoryLimit = memory as usize;
        }
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error());
        }
        if let Some(percent) = cpu_percent {
            let mut cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
                ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE
                    | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
                Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
                    CpuRate: (u32::from(percent) * 100).min(10_000),
                },
            };
            let configured = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectCpuRateControlInformation,
                    (&mut cpu as *mut JOBOBJECT_CPU_RATE_CONTROL_INFORMATION).cast(),
                    size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                )
            };
            if configured == 0 {
                unsafe { CloseHandle(handle) };
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self(handle))
    }

    pub(crate) fn assign(&self, child: &tokio::process::Child) -> io::Result<()> {
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("core process has no handle"))?;
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Protected launch context for one supervisor session.
///
/// The context file lives in a directory whose DACL grants only the owning
/// user, so Core and the shell can read it while another user or another logon
/// session cannot. It is removed when the supervisor stops.
struct SupervisorSession {
    context_path: PathBuf,
    launch_context: LaunchContext,
    _liveness: SupervisorLiveness,
}

impl SupervisorSession {
    fn establish() -> Result<Self, Box<dyn std::error::Error>> {
        use evohime_desktop_ipc::session::{write_launch_context, LaunchContext};
        use evohime_desktop_ipc::windows_security::{
            create_protected_directory, current_logon_session, current_user_sid,
        };

        let user_sid = current_user_sid()?;
        let logon_session = current_logon_session()?;
        let runtime_dir = core_data_dir().join("runtime");
        create_protected_directory(&runtime_dir, &user_sid)?;
        let context_path = runtime_dir.join("session.json");
        // A previous supervisor can leave a context file whose DACL is bound
        // to an old Windows logon session. The single-instance mutex is held
        // before this function runs, so removing that stale file cannot race
        // with a live supervisor and lets the new session self-heal.
        if context_path.exists() {
            fs::remove_file(&context_path)?;
        }

        let mut context = LaunchContext::generate(user_sid, logon_session, now_ms())?;
        context.supervisor_pipe_name = Some(
            evohime_desktop_ipc::session::generate_supervisor_pipe_name()
                .map_err(|error| io::Error::other(error.to_string()))?,
        );
        context.supervisor_secret = Some(
            evohime_desktop_ipc::session::SessionSecret::generate()
                .map_err(|error| io::Error::other(error.to_string()))?,
        );
        context.supervisor_pid = std::process::id();
        context.supervisor_liveness_event = format!(
            "Local\\EvoHime.Supervisor.Liveness.{}",
            context.pipe_name.rsplit('-').next().unwrap_or("session")
        );
        let event_name: Vec<u16> = OsStr::new(&context.supervisor_liveness_event)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let liveness = unsafe { CreateEventW(ptr::null(), 1, 1, event_name.as_ptr()) };
        if liveness.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        if let Err(error) = write_launch_context(&context_path, &context) {
            unsafe { CloseHandle(liveness) };
            return Err(error.into());
        }
        Ok(Self {
            context_path,
            launch_context: context,
            _liveness: SupervisorLiveness(liveness),
        })
    }
}

#[derive(Debug, serde::Deserialize)]
struct SupervisorHandshakeMessage {
    client_id: String,
    client_role: String,
    nonce: String,
    proof: String,
}

/// Authenticated, owner-only Core → supervisor lifecycle endpoint. The
/// command set is deliberately bounded; renderer data never reaches it.
async fn run_supervisor_command_channel(
    context: SupervisorSessionContext,
    logger: std::sync::Arc<SupervisorLogger>,
) -> io::Result<()> {
    use crate::local_provider::{LocalAdapterProcess, LocalProviderManager, ResourceLimits};
    use std::collections::BTreeMap;

    let mut provider_manager = LocalProviderManager::default();
    let mut adapter_processes: BTreeMap<String, LocalAdapterProcess> = BTreeMap::new();
    let mut verifier = evohime_desktop_ipc::session::HandshakeVerifier::new(
        context.launch_context.clone(),
        evohime_desktop_ipc::session::DEFAULT_NONCE_TTL_MS,
    )
    .map_err(|error| io::Error::other(error.to_string()))?;
    let user_sid = evohime_desktop_ipc::windows_security::current_user_sid()?;
    let logon_session = evohime_desktop_ipc::windows_security::current_logon_session()?;
    loop {
        let mut security =
            evohime_desktop_ipc::windows_security::PipeSecurity::owner_only(&user_sid)?;
        let server = unsafe {
            ServerOptions::new()
                .create_with_security_attributes_raw(context.pipe_name(), security.as_raw())
        }?;
        server.connect().await?;
        let mut channel = BufReader::new(server);
        let nonce = verifier
            .issue_nonce(now_ms())
            .map_err(|error| io::Error::other(error.to_string()))?;
        channel
            .get_mut()
            .write_all(
                serde_json::to_string(&json!({
                    "nonce": nonce.value,
                    "expires_at_ms": nonce.expires_at_ms
                }))
                .unwrap()
                .as_bytes(),
            )
            .await?;
        channel.get_mut().write_all(b"\n").await?;
        let mut line = Vec::new();
        if channel.read_until(b'\n', &mut line).await? > 16 * 1024 {
            continue;
        }
        let message: SupervisorHandshakeMessage = match serde_json::from_slice(&line) {
            Ok(message) => message,
            Err(_) => continue,
        };
        let request = evohime_desktop_ipc::session::HandshakeRequest {
            protocol_major: 1,
            client_id: message.client_id,
            client_role: message.client_role,
            nonce: message.nonce,
            proof: message.proof,
            capabilities: vec!["local-provider-lifecycle".into()],
            peer: evohime_desktop_ipc::session::PeerIdentity {
                user_sid: user_sid.clone(),
                logon_session: logon_session.clone(),
            },
        };
        if verifier.verify(&request, now_ms()).is_err() {
            let _ = logger.write("supervisor.command_auth_failed", json!({}));
            continue;
        }
        channel
            .get_mut()
            .write_all(b"{\"authenticated\":true}\n")
            .await?;
        let mut command = Vec::new();
        if channel.read_until(b'\n', &mut command).await? == 0 {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_slice(&command) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let op = value
            .get("op")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let response = match op {
            "launch" => {
                let model_id = value
                    .get("model_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let request_id = value
                    .get("request_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if model_id.len() > 128
                    || request_id.len() > 128
                    || model_id.trim().is_empty()
                    || request_id.trim().is_empty()
                {
                    json!({"accepted": false, "reason": "invalid_request"})
                } else if adapter_processes.contains_key(model_id) {
                    json!({"accepted": false, "reason": "already_running"})
                } else {
                    match provider_manager.launch(
                        model_id,
                        request_id,
                        now_ms(),
                        &[],
                        ResourceLimits::default(),
                    ) {
                        Ok((grant, _health)) => match LocalAdapterProcess::spawn_with_limits(
                            model_id,
                            grant.port,
                            ResourceLimits::default(),
                        )
                        .await
                        {
                            Ok(process) => {
                                adapter_processes.insert(model_id.to_owned(), process);
                                let _ = logger.write(
                                    "supervisor.local_provider_started",
                                    json!({"model_id": model_id, "port": grant.port}),
                                );
                                json!({"accepted": true, "request_id": grant.request_id, "expires_at_ms": grant.expires_at_ms, "port": grant.port, "token": grant.token})
                            }
                            Err(_) => {
                                let _ = provider_manager.stop(model_id, request_id, now_ms());
                                json!({"accepted": false, "reason": "process_start_failed"})
                            }
                        },
                        Err(error) => {
                            json!({"accepted": false, "reason": format!("{error:?}").to_ascii_lowercase()})
                        }
                    }
                }
            }
            "stop" => {
                let model_id = value
                    .get("model_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let request_id = value
                    .get("request_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let manager_result = provider_manager.stop(model_id, request_id, now_ms());
                if let Some(mut process) = adapter_processes.remove(model_id) {
                    let _ = process.stop().await;
                }
                match manager_result {
                    Ok(_) => {
                        let _ = logger.write(
                            "supervisor.local_provider_stopped",
                            json!({"model_id": model_id}),
                        );
                        json!({"accepted": true})
                    }
                    Err(error) => {
                        json!({"accepted": false, "reason": format!("{error:?}").to_ascii_lowercase()})
                    }
                }
            }
            "probe" => json!({"accepted": true, "processes": adapter_processes.len()}),
            _ => json!({"accepted": false, "reason": "unsupported_command"}),
        };
        channel
            .get_mut()
            .write_all(serde_json::to_string(&response).unwrap().as_bytes())
            .await?;
        channel.get_mut().write_all(b"\n").await?;
    }
}

/// Listener is deliberately outside the Core generation Job Object. Its own
/// bounded job lets it survive a Core crash while still guaranteeing cleanup
/// when this supervisor task ends.
async fn run_listener_supervision(
    listener_exe: PathBuf,
    pipe_name: String,
    context_path: PathBuf,
    logger: std::sync::Arc<SupervisorLogger>,
) {
    let max_restarts = std::env::var("EVOHIME_LISTENER_MAX_RESTARTS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(3);
    let mut restarts = 0;
    loop {
        let job = match JobObject::create_with_limits(Some(LISTENER_MEMORY_LIMIT_BYTES), Some(20)) {
            Ok(job) => job,
            Err(error) => {
                let _ = logger.write("listener.job_failed", json!({"error": error.to_string()}));
                return;
            }
        };
        let mut command = Command::new(&listener_exe);
        command
            .kill_on_drop(true)
            .env("EVOHIME_LISTENER_PIPE", &pipe_name)
            .env("EVOHIME_LAUNCH_CONTEXT", &context_path);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = logger.write("listener.spawn_failed", json!({"error": error.to_string()}));
                return;
            }
        };
        if let Err(error) = job.assign(&child) {
            let _ = logger.write(
                "listener.job_assign_failed",
                json!({"error": error.to_string()}),
            );
            let _ = child.kill().await;
            return;
        }
        let _ = logger.write("listener.spawned", json!({"restart": restarts}));
        let status = child.wait().await;
        drop(job);
        let _ = logger.write(
            "listener.exit",
            json!({"status": status.ok().and_then(|s| s.code())}),
        );
        if restarts >= max_restarts {
            let _ = logger.write(
                "listener.restart_budget_exhausted",
                json!({"max_restarts": max_restarts}),
            );
            return;
        }
        restarts += 1;
        tokio::time::sleep(std::time::Duration::from_millis(
            250u64.saturating_mul(2u64.saturating_pow(restarts.min(6))),
        ))
        .await;
    }
}

// The bundled `small` Whisper model is roughly 465 MiB on disk and its
// resident context needs additional memory. The previous 256 MiB cap caused
// Windows to fail-fast the listener while it loaded an otherwise valid engine.
const LISTENER_MEMORY_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone)]
struct SupervisorSessionContext {
    launch_context: LaunchContext,
}

impl SupervisorSessionContext {
    fn pipe_name(&self) -> &str {
        self.launch_context
            .supervisor_pipe_name
            .as_deref()
            .unwrap_or("")
    }
}

impl Drop for SupervisorSession {
    fn drop(&mut self) {
        // A stale secret must not outlive the session that issued it.
        let _ = fs::remove_file(&self.context_path);
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _instance = SingleInstance::acquire("Local\\EvoHime.Supervisor")?;
    let logger = std::sync::Arc::new(SupervisorLogger::open()?);
    let state_dir = update_state_dir();
    match recover_pending_update(&state_dir) {
        Ok(true) => {
            let _ = logger.write(
                "update.recovered",
                json!({"state_dir": state_dir.display().to_string()}),
            );
        }
        Ok(false) => {}
        Err(error) => {
            let event = if is_deferred_update_recovery_error(&error) {
                "update.recovery_deferred"
            } else {
                "update.recovery_failed"
            };
            let _ = logger.write(
                event,
                json!({"state_dir": state_dir.display().to_string(), "error": error.to_string()}),
            );
            if !is_deferred_update_recovery_error(&error) {
                return Err(error.into());
            }
        }
    }
    let core_exe = normalized_env_path("EVOHIME_CORE_EXE")
        .unwrap_or_else(|| PathBuf::from("evohime-core.exe"));

    // One launch context per supervisor session: an unpredictable pipe name, a
    // session secret and the identity the shell must run as. It survives Core
    // restarts so a connected shell keeps its credentials, and it is rotated
    // when this supervisor session ends.
    let session = match SupervisorSession::establish() {
        Ok(session) => {
            let _ = logger.write(
                "session.established",
                json!({"context": session.context_path.display().to_string()}),
            );
            Some(session)
        }
        Err(error) => {
            // Without a context Core falls back to the legacy pipe and reports
            // the connection as unauthenticated instead of refusing to start.
            let _ = logger.write("session.unavailable", json!({"error": error.to_string()}));
            None
        }
    };
    if let Some(session) = session.as_ref() {
        let context = SupervisorSessionContext {
            launch_context: session.launch_context.clone(),
        };
        let channel_logger = std::sync::Arc::clone(&logger);
        tokio::spawn(async move {
            if let Err(error) = run_supervisor_command_channel(context, channel_logger).await {
                // The Core lifecycle remains owned by the main supervisor loop;
                // a channel failure is logged and does not widen fallback paths.
                eprintln!("supervisor command channel stopped: {error}");
            }
        });
        let listener_exe = normalized_env_path("EVOHIME_LISTENER_EXE")
            .unwrap_or_else(|| PathBuf::from("evohime-listener.exe"));
        let listener_pipe = format!("{}-listener", session.launch_context.pipe_name);
        let listener_context_path = session.context_path.clone();
        let listener_logger = std::sync::Arc::clone(&logger);
        tokio::spawn(async move {
            run_listener_supervision(
                listener_exe,
                listener_pipe,
                listener_context_path,
                listener_logger,
            )
            .await;
        });
    }
    let max_restarts = std::env::var("EVOHIME_CORE_MAX_RESTARTS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(3);
    let healthy_uptime = healthy_uptime_threshold();
    let backoff_base = restart_backoff_base();
    let backoff_cap = restart_backoff_cap();
    let mut restarts = 0;
    let heartbeat_path = core_data_dir().join("core-heartbeat");
    let rotation_journal = core_data_dir()
        .join(evohime_receipts::key_lifecycle::KEY_DIR)
        .join(evohime_receipts::key_lifecycle::JOURNAL_FILE);
    if rotation_journal.exists() {
        match std::fs::read(&rotation_journal).ok().and_then(|bytes| {
            serde_json::from_slice::<evohime_receipts::key_lifecycle::RotationState>(&bytes).ok()
        }) {
            Some(state) => {
                let _ = logger.write(
                    "key.rotation_recovery_detected",
                    json!({"phase": state.phase, "rotation_id": state.rotation_id}),
                );
            }
            None => {
                let _ = logger.write(
                    "key.recovery_required",
                    json!({"error_code":"key.rotation_incomplete"}),
                );
                return Err("invalid receipt rotation journal".into());
            }
        }
    }

    // In-memory bounded scheduler/schedule contracts for this supervisor process's
    // lifetime (no persistence backing yet). They mirror the real spawn/health/exit
    // lifecycle of the core process; the restart decision below still owns
    // `max_restarts`, the contracts only report their own bounded view of it.
    let watchdog_max_attempts = max_restarts
        .saturating_add(2)
        .min(crate::schedule_contract::MAX_ATTEMPTS);
    let mut runtime = SupervisorRuntime::new(
        "evohime-core",
        "evohime-core-watchdog",
        "evohime-supervisor",
        StdDuration::from_secs(10),
        watchdog_max_attempts,
    )
    .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

    loop {
        let _ = logger.write("core.spawn", json!({"restart": restarts}));
        let generation_started = SystemTime::now();
        match runtime.start_generation(now_ms()) {
            Ok(events) => log_runtime_events(&logger, &events),
            Err(error) => {
                let _ = logger.write("runtime.error", json!({"error": error.to_string()}));
            }
        }
        let job = JobObject::create()?;
        let mut command = Command::new(&core_exe);
        command.kill_on_drop(true);
        if let Some(session) = session.as_ref() {
            command.env("EVOHIME_LAUNCH_CONTEXT", &session.context_path);
        }
        let mut child = command.spawn()?;
        job.assign(&child)?;
        let status = wait_for_core(&mut child, &heartbeat_path, &logger, &mut runtime).await?;
        let _ = logger.write(
            "core.exit",
            json!({"success": status.success(), "code": status.code()}),
        );
        match runtime.complete_generation(
            now_ms(),
            status.success(),
            format!("exit code {:?}", status.code()),
        ) {
            Ok(outcome) => log_runtime_events(&logger, &outcome.events),
            Err(error) => {
                let _ = logger.write("runtime.error", json!({"error": error.to_string()}));
            }
        }
        let healthy = generation_started
            .elapsed()
            .map(|elapsed| elapsed >= healthy_uptime)
            .unwrap_or(false);
        if should_reset_restart_budget(status.success(), healthy) {
            let previous_restarts = restarts;
            restarts = 0;
            let _ = logger.write(
                "core.restart_budget_reset",
                json!({
                    "reason": "healthy_generation_uptime",
                    "previous_restarts": previous_restarts,
                    "healthy_uptime_ms": healthy_uptime.as_millis()
                }),
            );
        }
        if status.success() || restarts >= max_restarts {
            return Ok(());
        }
        restarts += 1;
        let delay = restart_backoff(restarts, backoff_base, backoff_cap, now_ms());
        let _ = logger.write(
            "core.restart",
            json!({"restart": restarts, "backoff_ms": delay.as_millis()}),
        );
        sleep(delay).await;
    }
}

fn should_reset_restart_budget(success: bool, healthy: bool) -> bool {
    !success && healthy
}

fn healthy_uptime_threshold() -> StdDuration {
    let seconds = std::env::var("EVOHIME_CORE_HEALTHY_UPTIME_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(600)
        .clamp(1, 86_400);
    StdDuration::from_secs(seconds)
}

fn restart_backoff_base() -> StdDuration {
    let millis = std::env::var("EVOHIME_CORE_RESTART_BACKOFF_BASE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(250)
        .clamp(1, 10_000);
    StdDuration::from_millis(millis)
}

fn restart_backoff_cap() -> StdDuration {
    let millis = std::env::var("EVOHIME_CORE_RESTART_BACKOFF_MAX_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30_000)
        .clamp(1, 300_000);
    StdDuration::from_millis(millis)
}

fn restart_backoff(restart: u32, base: StdDuration, cap: StdDuration, entropy: u64) -> Duration {
    let exponent = restart.saturating_sub(1).min(20);
    let exponential_ms = base
        .as_millis()
        .saturating_mul(1u128 << exponent)
        .min(cap.as_millis());
    let jitter_bound = (base.as_millis() / 2).max(1);
    let jitter_ms = u128::from(entropy) % (jitter_bound + 1);
    let delay_ms = exponential_ms
        .saturating_add(jitter_ms)
        .min(cap.as_millis())
        .max(1);
    Duration::from_millis(delay_ms as u64)
}

fn core_data_dir() -> PathBuf {
    normalized_env_path("EVOHIME_DATA_DIR")
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|path| path.join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"))
}

fn normalized_env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn heartbeat_is_stale(path: &Path, max_age: StdDuration) -> bool {
    let modified = fs::metadata(path).and_then(|metadata| metadata.modified());
    modified
        .map(|time| SystemTime::now().duration_since(time).unwrap_or_default() > max_age)
        .unwrap_or(true)
}

fn heartbeat_is_current_generation(path: &Path, generation_started_at: SystemTime) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(|modified| modified >= generation_started_at)
        .unwrap_or(false)
}

fn heartbeat_is_stale_for_generation(
    path: &Path,
    generation_started_at: SystemTime,
    max_age: StdDuration,
) -> bool {
    if !heartbeat_is_current_generation(path, generation_started_at) {
        return true;
    }
    heartbeat_is_stale(path, max_age)
}

async fn wait_for_core(
    child: &mut tokio::process::Child,
    heartbeat_path: &Path,
    logger: &SupervisorLogger,
    runtime: &mut SupervisorRuntime,
) -> io::Result<std::process::ExitStatus> {
    let started = tokio::time::Instant::now();
    let generation_started_at = SystemTime::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        let heartbeat_fresh = !heartbeat_is_stale_for_generation(
            heartbeat_path,
            generation_started_at,
            StdDuration::from_secs(5),
        );
        match runtime.observe_tick(now_ms(), heartbeat_fresh) {
            Ok(events) => log_runtime_events(logger, &events),
            Err(error) => {
                let _ = logger.write("runtime.error", json!({"error": error.to_string()}));
            }
        }
        if started.elapsed() > Duration::from_secs(10)
            && heartbeat_is_stale_for_generation(
                heartbeat_path,
                generation_started_at,
                StdDuration::from_secs(5),
            )
        {
            let _ = logger.write(
                "core.health_timeout",
                json!({
                    "heartbeat": heartbeat_path.display().to_string(),
                    "generation_started_at_ms": generation_started_at
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis(),
                }),
            );
            child.kill().await?;
            return child.wait().await;
        }
        sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        heartbeat_is_current_generation, heartbeat_is_stale, heartbeat_is_stale_for_generation,
        is_deferred_update_recovery_error, recover_pending_update, restart_backoff,
        should_reset_restart_budget, JobObject, SupervisorSession,
    };
    use evohime_tx::UpdateTransaction;
    use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};
    use std::{fs, path::PathBuf};
    use tokio::process::Command;

    /// Proves the Job Object cleanup path actually works end to end: a real
    /// child process assigned to a `JobObject` must be killed by Windows when
    /// the job handle is closed (via `Drop`), thanks to
    /// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. This is what protects against
    /// orphaned core processes surviving supervisor restarts/crashes.
    #[tokio::test]
    async fn job_object_kill_on_close_terminates_child_process() {
        let mut child = Command::new("cmd")
            .args(["/C", "ping", "-n", "60", "127.0.0.1"])
            .spawn()
            .expect("spawn long-lived test child process");

        {
            let job = JobObject::create().expect("create job object");
            job.assign(&child).expect("assign child to job object");
            // `job` drops here, closing the handle. Combined with
            // KILL_ON_JOB_CLOSE this must terminate the child immediately.
        }

        // Give Windows a moment to tear the process down.
        tokio::time::sleep(StdDuration::from_millis(500)).await;

        let status = child
            .try_wait()
            .expect("try_wait should not error after job cleanup");
        assert!(
            status.is_some(),
            "child process should have been killed when its JobObject was dropped"
        );
    }

    #[test]
    fn recovers_pending_update_before_core_start() {
        let root = std::env::temp_dir().join(format!(
            "evohime-supervisor-recovery-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let install = root.join("install");
        let state = root.join("state");
        fs::create_dir_all(&install).unwrap();
        for component in UpdateTransaction::COMPONENTS {
            fs::write(install.join(component), format!("old:{component}")).unwrap();
        }
        let transaction = UpdateTransaction::prepare(&install, &state).unwrap();
        fs::write(install.join("EvoHime.exe"), "interrupted").unwrap();

        assert!(recover_pending_update(&state).unwrap());
        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "old:EvoHime.exe"
        );
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn defers_only_file_lock_recovery_errors() {
        assert!(is_deferred_update_recovery_error(
            &std::io::Error::from_raw_os_error(5)
        ));
        assert!(is_deferred_update_recovery_error(
            &std::io::Error::from_raw_os_error(32)
        ));
        assert!(is_deferred_update_recovery_error(
            &std::io::Error::from_raw_os_error(33)
        ));
        assert!(!is_deferred_update_recovery_error(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "broken transaction",
        )));
    }

    #[test]
    fn missing_core_heartbeat_is_stale() {
        let path =
            std::env::temp_dir().join(format!("evohime-missing-heartbeat-{}", std::process::id()));
        assert!(heartbeat_is_stale(&path, StdDuration::from_secs(5)));
    }

    #[test]
    fn normalized_env_path_removes_accidental_outer_whitespace() {
        std::env::set_var("EVOHIME_TEST_PATH", "  C:\\EvoHime\\data  ");
        assert_eq!(
            super::normalized_env_path("EVOHIME_TEST_PATH"),
            Some(PathBuf::from("C:\\EvoHime\\data"))
        );
        std::env::remove_var("EVOHIME_TEST_PATH");
    }

    #[test]
    fn listener_memory_limit_fits_the_bundled_small_model() {
        const { assert!(super::LISTENER_MEMORY_LIMIT_BYTES >= 1536 * 1024 * 1024) };
    }

    #[test]
    fn heartbeat_from_previous_generation_is_not_current() {
        let root = std::env::temp_dir().join(format!(
            "evohime-old-heartbeat-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("core-heartbeat");
        fs::write(&path, "old-generation").unwrap();

        let generation_started_at = SystemTime::now() + StdDuration::from_secs(1);
        assert!(!heartbeat_is_current_generation(
            &path,
            generation_started_at
        ));
        assert!(heartbeat_is_stale_for_generation(
            &path,
            generation_started_at,
            StdDuration::from_secs(5),
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn heartbeat_written_after_generation_is_current() {
        let root = std::env::temp_dir().join(format!(
            "evohime-current-heartbeat-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("core-heartbeat");
        let generation_started_at = SystemTime::now();
        std::thread::sleep(StdDuration::from_millis(20));
        fs::write(&path, "current-generation").unwrap();

        assert!(heartbeat_is_current_generation(
            &path,
            generation_started_at
        ));
        assert!(!heartbeat_is_stale_for_generation(
            &path,
            generation_started_at,
            StdDuration::from_secs(5),
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_backoff_is_exponential_bounded_and_nonzero() {
        let base = StdDuration::from_millis(250);
        let cap = StdDuration::from_millis(1_000);
        let first = restart_backoff(1, base, cap, 0);
        let second = restart_backoff(2, base, cap, 0);
        let capped = restart_backoff(10, base, cap, u64::MAX);

        assert!(first >= StdDuration::from_millis(250));
        assert!(second >= first);
        assert!(capped > StdDuration::ZERO);
        assert!(capped <= cap);
    }

    #[test]
    fn healthy_failed_generation_resets_budget_before_max_restart_guard() {
        assert!(should_reset_restart_budget(false, true));
        assert!(!should_reset_restart_budget(true, true));
        assert!(!should_reset_restart_budget(false, false));
    }

    /// The supervisor session must hand Core a usable, validated launch
    /// context bound to this user, and must not leave the secret behind when
    /// the session ends.
    #[test]
    fn session_writes_a_protected_launch_context_and_removes_it_on_drop() {
        let data_dir =
            std::env::temp_dir().join(format!("evohime-session-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        std::env::set_var("EVOHIME_DATA_DIR", &data_dir);
        let stale_path = data_dir.join("runtime/session.json");
        fs::create_dir_all(stale_path.parent().expect("runtime parent"))
            .expect("runtime directory");
        fs::write(&stale_path, b"stale session from an earlier logon")
            .expect("stale context writes");

        let context_path = {
            let session = SupervisorSession::establish().expect("session establishes");
            let path = session.context_path.clone();
            let context = evohime_desktop_ipc::session::read_launch_context(&path)
                .expect("context is readable and valid");
            assert!(
                context.is_authenticated(),
                "context binds a Windows identity"
            );
            assert!(context
                .pipe_name
                .starts_with(evohime_desktop_ipc::session::PIPE_PREFIX));
            assert!(context
                .supervisor_liveness_event
                .starts_with("Local\\EvoHime.Supervisor.Liveness."));
            path
        };

        assert!(
            !context_path.exists(),
            "the session secret must not outlive the supervisor session"
        );
        std::env::remove_var("EVOHIME_DATA_DIR");
        let _ = fs::remove_dir_all(&data_dir);
    }
}
