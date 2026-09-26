//! Core-to-supervisor contract for the local provider lifecycle.
//!
//! The process adapter is intentionally supervisor-owned.  This module keeps
//! the bounded, testable state machine separate from the Windows Job Object
//! plumbing in `windows_supervisor.rs`.

// Модуль — bounded-контракт: часть его поверхности сегодня вызывается только
// собственными тестами, а вызовы из supervisor появятся при wiring этапа,
// которому контракт принадлежит. Удалять её нельзя — это и есть описанный в
// планах интерфейс.
#![allow(dead_code)]

use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::process::Stdio;
#[cfg(windows)]
use tokio::process::{Child, Command};

#[cfg(windows)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(windows)]
use tokio::net::TcpStream;
#[cfg(windows)]
use tokio::time::{timeout, Duration};

#[cfg(windows)]
use crate::windows_supervisor::JobObject;

pub const PORT_FIRST: u16 = 49_152;
pub const PORT_LAST: u16 = 49_252;
pub const MAX_PORT_ATTEMPTS: usize = 8;
pub const SESSION_TTL_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Starting,
    Running,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Ready,
    Degraded,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalError {
    ModelNotFound,
    PortUnavailable,
    AlreadyCancelled,
    InvalidRequest,
    ResourceLimitExceeded,
    Timeout,
    Cancelled,
    Unavailable,
    AuthenticationFailed,
    SessionExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    pub adapter_memory_bytes: u64,
    pub runtime_memory_bytes: u64,
    pub adapter_cpu_percent: u8,
    pub runtime_cpu_percent: u8,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            adapter_memory_bytes: 512 * 1024 * 1024,
            runtime_memory_bytes: 4 * 1024 * 1024 * 1024,
            adapter_cpu_percent: 25,
            runtime_cpu_percent: 75,
        }
    }
}

impl ResourceLimits {
    pub fn validate(self) -> Result<Self, LocalError> {
        if self.adapter_memory_bytes == 0
            || self.adapter_memory_bytes > 1024 * 1024 * 1024
            || self.runtime_memory_bytes == 0
            || self.runtime_memory_bytes > 12 * 1024 * 1024 * 1024
            || self.adapter_cpu_percent > 100
            || self.runtime_cpu_percent > 100
        {
            return Err(LocalError::InvalidRequest);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGrant {
    pub token: Vec<u8>,
    pub request_id: String,
    pub expires_at_ms: u64,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthEvent {
    pub request_id: String,
    pub model_id: String,
    pub process_state: ProcessState,
    pub health_status: HealthStatus,
    pub reason: Option<&'static str>,
    pub port: Option<u16>,
}

#[derive(Debug)]
struct SessionRecord {
    hash: [u8; 32],
    expires_at_ms: u64,
    used: bool,
}

#[derive(Debug)]
struct ProcessRecord {
    model_id: String,
    state: ProcessState,
    port: u16,
    references: u32,
    idle_since_ms: Option<u64>,
    limits: ResourceLimits,
    sessions: BTreeMap<String, SessionRecord>,
}

#[derive(Debug, Default)]
pub struct LocalProviderManager {
    processes: BTreeMap<String, ProcessRecord>,
    cancelled: BTreeMap<String, bool>,
}

impl LocalProviderManager {
    pub fn launch(
        &mut self,
        model_id: &str,
        request_id: &str,
        now_ms: u64,
        occupied_ports: &[u16],
        limits: ResourceLimits,
    ) -> Result<(SessionGrant, HealthEvent), LocalError> {
        if model_id.trim().is_empty() || request_id.trim().is_empty() {
            return Err(LocalError::InvalidRequest);
        }
        let limits = limits.validate()?;
        if self.cancelled.remove(request_id).is_some() {
            return Err(LocalError::Cancelled);
        }
        let port = if let Some(record) = self.processes.get(model_id) {
            record.port
        } else {
            choose_port(occupied_ports).ok_or(LocalError::PortUnavailable)?
        };
        let record = self
            .processes
            .entry(model_id.to_owned())
            .or_insert_with(|| ProcessRecord {
                model_id: model_id.to_owned(),
                state: ProcessState::Starting,
                port,
                references: 0,
                idle_since_ms: None,
                limits,
                sessions: BTreeMap::new(),
            });
        record.state = ProcessState::Running;
        record.references = record.references.saturating_add(1);
        record.idle_since_ms = None;
        let mut token = vec![0u8; 32];
        OsRng.fill_bytes(&mut token);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&Sha256::digest(&token));
        let expires_at_ms = now_ms.saturating_add(SESSION_TTL_MS);
        record.sessions.insert(
            request_id.to_owned(),
            SessionRecord {
                hash,
                expires_at_ms,
                used: false,
            },
        );
        Ok((
            SessionGrant {
                token,
                request_id: request_id.to_owned(),
                expires_at_ms,
                port: record.port,
            },
            HealthEvent {
                request_id: request_id.to_owned(),
                model_id: model_id.to_owned(),
                process_state: ProcessState::Running,
                health_status: HealthStatus::Ready,
                reason: None,
                port: Some(record.port),
            },
        ))
    }

    /// Authenticates a launch grant exactly once. The grant is consumed at
    /// request admission, so a response that takes longer than the session TTL
    /// does not invalidate an already admitted request.
    pub fn authenticate(
        &mut self,
        model_id: &str,
        request_id: &str,
        token: &[u8],
        now_ms: u64,
    ) -> Result<u16, LocalError> {
        let record = self
            .processes
            .get_mut(model_id)
            .ok_or(LocalError::ModelNotFound)?;
        let session = record
            .sessions
            .get_mut(request_id)
            .ok_or(LocalError::AuthenticationFailed)?;
        if now_ms > session.expires_at_ms {
            return Err(LocalError::SessionExpired);
        }
        if session.used || Sha256::digest(token).as_slice() != session.hash {
            return Err(LocalError::AuthenticationFailed);
        }
        session.used = true;
        Ok(record.port)
    }

    pub fn stop(
        &mut self,
        model_id: &str,
        request_id: &str,
        now_ms: u64,
    ) -> Result<HealthEvent, LocalError> {
        let Some(record) = self.processes.get_mut(model_id) else {
            return Ok(HealthEvent {
                request_id: request_id.to_owned(),
                model_id: model_id.to_owned(),
                process_state: ProcessState::Stopped,
                health_status: HealthStatus::Unavailable,
                reason: Some("already_cancelled"),
                port: None,
            });
        };
        if record.sessions.remove(request_id).is_none() {
            self.cancelled.insert(request_id.to_owned(), true);
            return Err(LocalError::AlreadyCancelled);
        }
        record.references = record.references.saturating_sub(1);
        if record.references == 0 {
            record.idle_since_ms = Some(now_ms);
            record.state = ProcessState::Stopping;
            record.state = ProcessState::Stopped;
        }
        let stopped = record.state == ProcessState::Stopped;
        Ok(HealthEvent {
            request_id: request_id.to_owned(),
            model_id: model_id.to_owned(),
            process_state: record.state,
            health_status: if stopped {
                HealthStatus::Unavailable
            } else {
                HealthStatus::Ready
            },
            reason: None,
            port: Some(record.port),
        })
    }

    pub fn reap_idle(&mut self, now_ms: u64, idle_timeout_ms: u64) -> Vec<HealthEvent> {
        let ids: Vec<String> = self
            .processes
            .iter()
            .filter(|(_, p)| {
                p.references == 0
                    && p.idle_since_ms
                        .is_some_and(|at| now_ms.saturating_sub(at) >= idle_timeout_ms)
            })
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                self.processes.remove(&id).map(|p| HealthEvent {
                    request_id: String::new(),
                    model_id: p.model_id,
                    process_state: ProcessState::Stopped,
                    health_status: HealthStatus::Unavailable,
                    reason: None,
                    port: Some(p.port),
                })
            })
            .collect()
    }

    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    pub fn is_running(&self, model_id: &str) -> bool {
        self.processes
            .get(model_id)
            .is_some_and(|process| process.state == ProcessState::Running)
    }
}

pub fn choose_port(occupied: &[u16]) -> Option<u16> {
    (PORT_FIRST..=PORT_LAST)
        .filter(|port| !occupied.contains(port))
        .take(MAX_PORT_ATTEMPTS)
        .next()
}

/// Supervisor-owned adapter process. The renderer never supplies an
/// executable or command line: the supervisor reads the configured adapter
/// path from its own environment and passes only the selected model and port.
#[cfg(windows)]
pub struct LocalAdapterProcess {
    child: Child,
    _job: JobObject,
    port: u16,
}

/// Generic external-agent process. The executable reference is resolved by the
/// supervisor from its allowlisted environment; Core never sends a shell line.
#[cfg(windows)]
pub struct ExternalAgentProcess {
    child: Child,
    _job: JobObject,
}

/// Result of one bounded supervised llama.cpp conversion.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantizerResult {
    /// Core-relative path of the verified output file.
    pub output_relative_path: String,
    /// SHA-256 of the completed output GGUF.
    pub output_sha256: String,
    /// Exact output size in bytes.
    pub output_size_bytes: u64,
}

/// Quantizer child attached to a Windows Job Object until it exits.
#[cfg(windows)]
pub struct LocalQuantizerProcess {
    child: Child,
    job: Option<JobObject>,
    output_path: PathBuf,
    output_relative_path: String,
    max_output_bytes: u64,
    deadline: tokio::time::Instant,
}

/// Pinned llama-server process used to verify and exercise one adapted model.
#[cfg(windows)]
pub struct LocalInferenceProcess {
    child: Child,
    job: Option<JobObject>,
    port: u16,
    model_alias: String,
    startup_deadline: Option<tokio::time::Instant>,
}

/// Verified source and bounded resource request for a pinned inference process.
#[cfg(windows)]
pub struct PinnedInferenceRequest<'a> {
    /// Managed EvoHime data directory.
    pub data_root: &'a Path,
    /// Durable adaptation job identity.
    pub job_id: &'a str,
    /// Managed relative path to the promoted GGUF.
    pub model_relative_path: &'a Path,
    /// Expected SHA-256 digest of the model bytes.
    pub expected_model_sha256: &'a str,
    /// Expected model size in bytes.
    pub expected_model_size: u64,
    /// Bounded CPU thread count for inference.
    pub threads: u16,
    /// Job Object memory limit in bytes.
    pub memory_limit_bytes: u64,
    /// Job Object CPU limit as a percentage.
    pub cpu_limit_percent: u8,
}

/// Verified source, target and bounded resource request for pinned quantization.
#[cfg(windows)]
pub struct PinnedQuantizerRequest<'a> {
    /// Managed EvoHime data directory.
    pub data_root: &'a Path,
    /// Durable adaptation job identity.
    pub job_id: &'a str,
    /// Managed relative path to the source GGUF.
    pub source_relative_path: &'a Path,
    /// Expected SHA-256 digest of the source bytes.
    pub expected_source_sha256: &'a str,
    /// Expected source size in bytes.
    pub expected_source_size: u64,
    /// Fixed llama.cpp quantization target.
    pub target: &'a str,
    /// Bounded CPU thread count for quantization.
    pub threads: u16,
    /// Job Object memory limit in bytes.
    pub memory_limit_bytes: u64,
    /// Job Object CPU limit as a percentage.
    pub cpu_limit_percent: u8,
    /// Maximum output artifact size in bytes.
    pub max_output_bytes: u64,
}

/// Starts the hash-pinned CPU server for a Core-verified staged GGUF.
#[cfg(windows)]
pub async fn spawn_pinned_inference(
    request: PinnedInferenceRequest<'_>,
) -> Result<LocalInferenceProcess, LocalError> {
    let PinnedInferenceRequest {
        data_root,
        job_id,
        model_relative_path,
        expected_model_sha256,
        expected_model_size,
        threads,
        memory_limit_bytes,
        cpu_limit_percent,
    } = request;
    if job_id.trim().is_empty()
        || job_id.len() > 128
        || job_id.bytes().any(|byte| byte.is_ascii_control())
        || !(512 * 1024 * 1024..=32 * 1024 * 1024 * 1024).contains(&memory_limit_bytes)
        || cpu_limit_percent == 0
        || cpu_limit_percent > 100
        || expected_model_size == 0
        || expected_model_size > 16 * 1024 * 1024 * 1024
        || threads == 0
        || threads > 64
        || expected_model_sha256.len() != 64
        || !expected_model_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !is_managed_relative_path(model_relative_path)
    {
        return Err(LocalError::InvalidRequest);
    }
    let data_root = std::fs::canonicalize(data_root).map_err(|_| LocalError::Unavailable)?;
    let adapter_dir = data_root.join("tools").join("llama.cpp").join("b10981");
    if !evohime_desktop_ipc::local_adapter_contract::verify_runtime_files(&adapter_dir) {
        return Err(LocalError::ModelNotFound);
    }
    let models_root = data_root.join("models");
    let model_path = resolve_managed_model_path(&models_root, model_relative_path, true)?;
    let path_for_hash = model_path.clone();
    let (actual_hash, actual_size) =
        tokio::task::spawn_blocking(move || hash_file_sha256(&path_for_hash))
            .await
            .map_err(|_| LocalError::Unavailable)??;
    if actual_hash != expected_model_sha256 || actual_size != expected_model_size {
        return Err(LocalError::InvalidRequest);
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|_| LocalError::Unavailable)?;
    let port = listener
        .local_addr()
        .map_err(|_| LocalError::Unavailable)?
        .port();
    drop(listener);
    let model_alias = format!("evohime-adaptation-{}", &actual_hash[..16]);
    let job = JobObject::create_with_limits(Some(memory_limit_bytes), Some(cpu_limit_percent))
        .map_err(|_| LocalError::ResourceLimitExceeded)?;
    let mut child = Command::new(adapter_dir.join("llama-server.exe"))
        .arg("--model")
        .arg(&model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--alias")
        .arg(&model_alias)
        .arg("--ctx-size")
        .arg("2048")
        .arg("--threads")
        .arg(threads.to_string())
        .arg("--n-gpu-layers")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| LocalError::Unavailable)?;
    if job.assign(&child).is_err() {
        let _ = child.start_kill();
        return Err(LocalError::ResourceLimitExceeded);
    }
    Ok(LocalInferenceProcess {
        child,
        job: Some(job),
        port,
        model_alias,
        startup_deadline: Some(tokio::time::Instant::now() + Duration::from_secs(15 * 60)),
    })
}

#[cfg(windows)]
impl LocalInferenceProcess {
    /// Returns the fixed alias derived from the verified model digest.
    pub fn model_alias(&self) -> &str {
        &self.model_alias
    }

    /// Probes a loaded model using the loopback OpenAI models endpoint.
    pub async fn probe(&mut self) -> Result<Option<u16>, LocalError> {
        if self
            .startup_deadline
            .is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
        {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
            self.job.take();
            return Err(LocalError::Timeout);
        }
        if self
            .child
            .try_wait()
            .map_err(|_| LocalError::Unavailable)?
            .is_some()
        {
            self.job.take();
            return Err(LocalError::Unavailable);
        }
        let port = self.port;
        let alias = self.model_alias.clone();
        let result = timeout(Duration::from_secs(3), async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port))
                .await
                .map_err(|_| LocalError::Unavailable)?;
            let request = format!(
                "GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(request.as_bytes())
                .await
                .map_err(|_| LocalError::Unavailable)?;
            let mut body = Vec::with_capacity(16 * 1024);
            let mut buffer = [0_u8; 2048];
            while body.len() < 16 * 1024 {
                let read = stream
                    .read(&mut buffer)
                    .await
                    .map_err(|_| LocalError::Unavailable)?;
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&buffer[..read]);
            }
            let text = std::str::from_utf8(&body).map_err(|_| LocalError::Unavailable)?;
            if !text.starts_with("HTTP/1.1 200") && !text.starts_with("HTTP/1.0 200") {
                return Err(LocalError::Unavailable);
            }
            let (_, payload) = text.split_once("\r\n\r\n").ok_or(LocalError::Unavailable)?;
            let value: serde_json::Value =
                serde_json::from_str(payload).map_err(|_| LocalError::Unavailable)?;
            let models = value
                .get("data")
                .and_then(serde_json::Value::as_array)
                .ok_or(LocalError::Unavailable)?;
            if models.iter().any(|model| {
                model.get("id").and_then(serde_json::Value::as_str) == Some(alias.as_str())
            }) {
                Ok(Some(port))
            } else {
                Ok(None)
            }
        })
        .await;
        match result {
            Ok(Ok(ready)) => {
                if ready.is_some() {
                    self.startup_deadline = None;
                }
                Ok(ready)
            }
            Ok(Err(LocalError::Unavailable)) | Err(_) => Ok(None),
            Ok(Err(error)) => Err(error),
        }
    }

    /// Stops the model server and releases its process-tree job.
    pub async fn stop(&mut self) -> Result<(), LocalError> {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        self.job.take();
        Ok(())
    }
}

/// Starts only the installed hash-pinned llama.cpp quantizer with typed args.
#[cfg(windows)]
pub async fn spawn_pinned_quantizer(
    request: PinnedQuantizerRequest<'_>,
) -> Result<LocalQuantizerProcess, LocalError> {
    const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
    let PinnedQuantizerRequest {
        data_root,
        job_id,
        source_relative_path,
        expected_source_sha256,
        expected_source_size,
        target,
        threads,
        memory_limit_bytes,
        cpu_limit_percent,
        max_output_bytes,
    } = request;
    if job_id.trim().is_empty()
        || job_id.len() > 128
        || job_id.bytes().any(|byte| byte.is_ascii_control())
        || !matches!(target, "Q4_K_M" | "Q5_K_M" | "Q8_0")
        || threads == 0
        || threads > 256
        || !(512 * 1024 * 1024..=32 * 1024 * 1024 * 1024).contains(&memory_limit_bytes)
        || cpu_limit_percent == 0
        || cpu_limit_percent > 100
        || max_output_bytes == 0
        || max_output_bytes > MAX_OUTPUT_BYTES
        || expected_source_size == 0
        || expected_source_size > 16 * 1024 * 1024 * 1024
        || expected_source_sha256.len() != 64
        || !expected_source_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !is_managed_relative_path(source_relative_path)
    {
        return Err(LocalError::InvalidRequest);
    }
    let data_root = std::fs::canonicalize(data_root).map_err(|_| LocalError::Unavailable)?;
    let tools_root = data_root.join("tools");
    let adapter_dir = tools_root.join("llama.cpp").join("b10981");
    if !evohime_desktop_ipc::local_adapter_contract::verify_runtime_files(&adapter_dir) {
        return Err(LocalError::ModelNotFound);
    }
    let models_root = data_root.join("models");
    let source_path = resolve_managed_model_path(&models_root, source_relative_path, true)?;
    let source_for_hash = source_path.clone();
    let (observed_hash, observed_size) =
        tokio::task::spawn_blocking(move || hash_file_sha256(&source_for_hash))
            .await
            .map_err(|_| LocalError::Unavailable)??;
    if observed_hash != expected_source_sha256 || observed_size != expected_source_size {
        return Err(LocalError::InvalidRequest);
    }
    let staging_dir = models_root.join(".adaptation-staging");
    match std::fs::symlink_metadata(&staging_dir) {
        Ok(metadata) if metadata.file_type().is_dir() && !has_unsafe_reparse_point(&metadata) => {}
        Ok(_) => return Err(LocalError::InvalidRequest),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&staging_dir).map_err(|_| LocalError::Unavailable)?;
        }
        Err(_) => return Err(LocalError::Unavailable),
    }
    let file_name = encode_hex(Sha256::digest(job_id.as_bytes()));
    let output_relative_path = format!(".adaptation-staging/{file_name}.gguf");
    let output_path =
        resolve_managed_model_path(&models_root, Path::new(&output_relative_path), false)?;
    match std::fs::symlink_metadata(&output_path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            std::fs::remove_file(&output_path).map_err(|_| LocalError::Unavailable)?;
        }
        Ok(_) => return Err(LocalError::InvalidRequest),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(LocalError::Unavailable),
    }
    let executable = adapter_dir.join("llama-quantize.exe");
    let job = JobObject::create_with_limits(Some(memory_limit_bytes), Some(cpu_limit_percent))
        .map_err(|_| LocalError::ResourceLimitExceeded)?;
    let mut child = Command::new(executable)
        .arg(&source_path)
        .arg(&output_path)
        .arg(target)
        .arg(threads.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| LocalError::Unavailable)?;
    if job.assign(&child).is_err() {
        let _ = child.start_kill();
        return Err(LocalError::ResourceLimitExceeded);
    }
    Ok(LocalQuantizerProcess {
        child,
        job: Some(job),
        output_path,
        output_relative_path,
        max_output_bytes,
        deadline: tokio::time::Instant::now() + Duration::from_secs(6 * 60 * 60),
    })
}

#[cfg(windows)]
impl LocalQuantizerProcess {
    /// Returns None while running and a bounded content result after success.
    pub async fn poll(&mut self) -> Result<Option<QuantizerResult>, LocalError> {
        if tokio::time::Instant::now() >= self.deadline {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
            self.job.take();
            return Err(LocalError::Timeout);
        }
        if std::fs::symlink_metadata(&self.output_path)
            .is_ok_and(|metadata| metadata.len() > self.max_output_bytes)
        {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
            self.job.take();
            return Err(LocalError::InvalidRequest);
        }
        let Some(status) = self.child.try_wait().map_err(|_| LocalError::Unavailable)? else {
            return Ok(None);
        };
        self.job.take();
        if !status.success() {
            return Err(LocalError::Unavailable);
        }
        let metadata =
            std::fs::symlink_metadata(&self.output_path).map_err(|_| LocalError::Unavailable)?;
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() > self.max_output_bytes
        {
            return Err(LocalError::InvalidRequest);
        }
        let output_path = self.output_path.clone();
        let (output_sha256, output_size_bytes) =
            tokio::task::spawn_blocking(move || hash_file_sha256(&output_path))
                .await
                .map_err(|_| LocalError::Unavailable)??;
        Ok(Some(QuantizerResult {
            output_relative_path: self.output_relative_path.clone(),
            output_sha256,
            output_size_bytes,
        }))
    }

    /// Kills this process tree and waits for the child to exit.
    pub async fn cancel(&mut self) -> Result<(), LocalError> {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        self.job.take();
        Ok(())
    }
}

#[cfg(windows)]
fn is_managed_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

#[cfg(windows)]
fn resolve_managed_model_path(
    root: &Path,
    relative: &Path,
    must_exist: bool,
) -> Result<PathBuf, LocalError> {
    if !is_managed_relative_path(relative)
        || !std::fs::symlink_metadata(root).is_ok_and(|metadata| {
            metadata.file_type().is_dir() && !has_unsafe_reparse_point(&metadata)
        })
    {
        return Err(LocalError::InvalidRequest);
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(LocalError::InvalidRequest);
        };
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if has_unsafe_reparse_point(&metadata) => {
                return Err(LocalError::InvalidRequest);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !must_exist => {}
            Err(_) => return Err(LocalError::Unavailable),
        }
    }
    if must_exist
        && !std::fs::symlink_metadata(&current).is_ok_and(|metadata| {
            metadata.file_type().is_file() && !has_unsafe_reparse_point(&metadata)
        })
    {
        return Err(LocalError::ModelNotFound);
    }
    Ok(current)
}

#[cfg(windows)]
fn hash_file_sha256(path: &Path) -> Result<(String, u64), LocalError> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| LocalError::Unavailable)?;
    if !metadata.file_type().is_file() || has_unsafe_reparse_point(&metadata) {
        return Err(LocalError::InvalidRequest);
    }
    let mut file = std::fs::File::open(path).map_err(|_| LocalError::Unavailable)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| LocalError::Unavailable)?;
        if read == 0 {
            break;
        }
        size = size.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    Ok((encode_hex(hasher.finalize()), size))
}

#[cfg(windows)]
fn has_unsafe_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(windows)]
fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(windows)]
impl ExternalAgentProcess {
    pub async fn spawn(executable_ref: &str, run_id: &str) -> Result<Self, LocalError> {
        if executable_ref.trim().is_empty()
            || executable_ref.len() > 96
            || run_id.trim().is_empty()
            || run_id.len() > 96
        {
            return Err(LocalError::InvalidRequest);
        }
        let key = format!(
            "EVOHIME_EXTERNAL_AGENT_{}",
            executable_ref.replace(['.', '-'], "_")
        );
        let executable = std::env::var_os(key).ok_or(LocalError::ModelNotFound)?;
        let job = JobObject::create_with_limits(Some(1024 * 1024 * 1024), Some(50))
            .map_err(|_| LocalError::ResourceLimitExceeded)?;
        let mut child = Command::new(executable)
            .arg("--evohime-protocol")
            .arg("evohime.external-agent/v1")
            .arg("--run-id")
            .arg(run_id)
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| LocalError::ModelNotFound)?;
        if job.assign(&child).is_err() {
            let _ = child.start_kill();
            return Err(LocalError::ResourceLimitExceeded);
        }
        Ok(Self { child, _job: job })
    }
    pub async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

#[cfg(windows)]
impl LocalAdapterProcess {
    pub async fn spawn(model_id: &str, port: u16) -> Result<Self, LocalError> {
        Self::spawn_with_limits(model_id, port, ResourceLimits::default()).await
    }

    pub async fn spawn_with_limits(
        model_id: &str,
        port: u16,
        limits: ResourceLimits,
    ) -> Result<Self, LocalError> {
        if model_id.trim().is_empty() || !(PORT_FIRST..=PORT_LAST).contains(&port) {
            return Err(LocalError::InvalidRequest);
        }
        let limits = limits.validate()?;
        let executable =
            std::env::var_os("EVOHIME_LOCAL_ADAPTER_EXE").ok_or(LocalError::ModelNotFound)?;
        let job = JobObject::create_with_limits(
            Some(limits.adapter_memory_bytes),
            Some(limits.adapter_cpu_percent),
        )
        .map_err(|_| LocalError::ResourceLimitExceeded)?;
        let mut child = Command::new(executable)
            .arg("--model-id")
            .arg(model_id)
            .arg("--port")
            .arg(port.to_string())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| LocalError::ModelNotFound)?;
        if job.assign(&child).is_err() {
            let _ = child.start_kill();
            return Err(LocalError::ResourceLimitExceeded);
        }
        Ok(Self {
            child,
            _job: job,
            port,
        })
    }

    pub async fn stop(&mut self) -> Result<(), LocalError> {
        self.child
            .start_kill()
            .map_err(|_| LocalError::AlreadyCancelled)?;
        let _ = self.child.wait().await;
        Ok(())
    }

    /// Bounded OpenAI-compatible capability probe. Process existence alone is
    /// not a health signal: the adapter must expose the requested model and a
    /// valid models response on the supervisor-selected loopback port.
    pub async fn probe(&mut self, model_id: &str) -> Result<(), LocalError> {
        let port = self.port;
        let result = timeout(Duration::from_secs(2), async {
            let mut stream = TcpStream::connect(("127.0.0.1", port))
                .await
                .map_err(|_| LocalError::Unavailable)?;
            let request = format!(
                "GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(request.as_bytes())
                .await
                .map_err(|_| LocalError::Unavailable)?;
            let mut body = Vec::with_capacity(16 * 1024);
            let mut buffer = [0_u8; 4096];
            while body.len() < 16 * 1024 {
                let read = stream
                    .read(&mut buffer)
                    .await
                    .map_err(|_| LocalError::Unavailable)?;
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&buffer[..read]);
            }
            let text = std::str::from_utf8(&body).map_err(|_| LocalError::Unavailable)?;
            let payload = text
                .split_once("\r\n\r\n")
                .map(|(_, payload)| payload)
                .ok_or(LocalError::Unavailable)?;
            if !text.starts_with("HTTP/1.1 200") && !text.starts_with("HTTP/1.0 200") {
                return Err(LocalError::Unavailable);
            }
            let value: serde_json::Value =
                serde_json::from_str(payload).map_err(|_| LocalError::Unavailable)?;
            let models = value
                .get("data")
                .and_then(serde_json::Value::as_array)
                .ok_or(LocalError::Unavailable)?;
            if models.iter().any(|candidate| {
                candidate.get("id").and_then(serde_json::Value::as_str) == Some(model_id)
            }) {
                Ok(())
            } else {
                Err(LocalError::ModelNotFound)
            }
        })
        .await;
        result.unwrap_or(Err(LocalError::Timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reuses_process_and_stops_idempotently() {
        let mut manager = LocalProviderManager::default();
        let (first, _) = manager
            .launch("m", "r1", 0, &[], ResourceLimits::default())
            .unwrap();
        let (second, _) = manager
            .launch("m", "r2", 1, &[], ResourceLimits::default())
            .unwrap();
        assert_eq!(first.port, second.port);
        assert_eq!(manager.process_count(), 1);
        assert!(manager.stop("m", "r1", 2).is_ok());
        assert!(manager.stop("m", "r2", 3).is_ok());
        assert_eq!(manager.process_count(), 1);
        assert!(manager.stop("m", "r2", 4).is_err());
    }
    #[test]
    fn launch_stop_race_is_cancelled() {
        let mut manager = LocalProviderManager::default();
        manager.cancelled.insert("r".into(), true);
        assert_eq!(
            manager.launch("m", "r", 0, &[], ResourceLimits::default()),
            Err(LocalError::Cancelled)
        );
    }
    #[test]
    fn session_grant_is_single_use_and_time_bounded() {
        let mut manager = LocalProviderManager::default();
        let (grant, _) = manager
            .launch("m", "r", 1_000, &[], ResourceLimits::default())
            .unwrap();
        assert_eq!(
            manager.authenticate("m", "r", &grant.token, 1_001),
            Ok(grant.port)
        );
        assert_eq!(
            manager.authenticate("m", "r", &grant.token, 1_002),
            Err(LocalError::AuthenticationFailed)
        );
        let (expired, _) = manager
            .launch("m", "r2", 1_000, &[], ResourceLimits::default())
            .unwrap();
        assert_eq!(
            manager.authenticate("m", "r2", &expired.token, expired.expires_at_ms + 1),
            Err(LocalError::SessionExpired)
        );
    }
    #[test]
    fn port_selection_is_bounded() {
        let occupied: Vec<u16> = (PORT_FIRST..PORT_FIRST + 8).collect();
        assert_eq!(choose_port(&occupied), Some(PORT_FIRST + 8));
    }
}

#[cfg(all(test, windows))]
mod adaptation_process_tests {
    use super::*;
    use crate::windows_supervisor::JobObject;

    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "evohime-adaptation-process-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    async fn fake_quantizer(
        root: &Path,
        command: &str,
        max_output_bytes: u64,
    ) -> LocalQuantizerProcess {
        let output_path = root.join("fake-output.gguf");
        let child = Command::new("cmd")
            .args(["/C", command])
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let job = JobObject::create().unwrap();
        job.assign(&child).unwrap();
        LocalQuantizerProcess {
            child,
            job: Some(job),
            output_path: output_path.clone(),
            output_relative_path: ".adaptation-staging/fake-output.gguf".into(),
            max_output_bytes,
            deadline: tokio::time::Instant::now() + Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn fake_quantizer_completion_returns_hash_and_size() {
        let root = root();
        let command = "echo fake-output>fake-output.gguf";
        let mut process = fake_quantizer(&root, command, 1024).await;
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(result) = process.poll().await.unwrap() {
                    break result;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let contents = std::fs::read(root.join("fake-output.gguf")).unwrap();
        assert_eq!(result.output_size_bytes, contents.len() as u64);
        assert_eq!(result.output_sha256, encode_hex(Sha256::digest(contents)));
        assert_eq!(
            result.output_relative_path,
            ".adaptation-staging/fake-output.gguf"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn fake_quantizer_rejects_oversized_output_and_kills_child() {
        let root = root();
        std::fs::write(root.join("fake-output.gguf"), b"too large").unwrap();
        let mut process = fake_quantizer(&root, "ping -n 30 127.0.0.1 >NUL", 2).await;
        assert_eq!(process.poll().await, Err(LocalError::InvalidRequest));
        assert!(process.child.try_wait().unwrap().is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn fake_quantizer_cancel_waits_for_child_exit() {
        let root = root();
        let mut process = fake_quantizer(&root, "ping -n 30 127.0.0.1 >NUL", 1024).await;
        process.cancel().await.unwrap();
        assert!(process.child.try_wait().unwrap().is_some());
        std::fs::remove_dir_all(root).unwrap();
    }
}
