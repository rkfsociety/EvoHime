//! Core-owned contract for local model conversion, verification and promotion.
//!
//! This module defines immutable request identities and fail-closed job-state
//! transitions. Process execution, storage and IPC remain owned by their
//! existing runtime boundaries.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tokio::io::AsyncWriteExt;

pub use evohime_desktop_ipc::local_adapter_contract::{
    LLAMA_CPP_ARCHIVE_SHA256, LLAMA_CPP_ARCHIVE_SIZE_BYTES, LLAMA_CPP_ASSET_URL,
    LLAMA_CPP_OPENMP_LICENSE_SHA256, LLAMA_CPP_RUNTIME_FILES, LLAMA_CPP_VERSION,
};

/// Stable serialized contract identifier.
pub const CONTRACT_ID: &str = "local-model-adaptation-v1";
/// Maximum number of retained adaptation jobs.
pub const MAX_JOBS: usize = 256;
/// Maximum output GGUF size accepted by this pipeline.
pub const MAX_OUTPUT_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Returns the only staging filename accepted for a durable job ID.
pub fn staging_relative_path(job_id: &str) -> String {
    format!(
        ".adaptation-staging/{}.gguf",
        hex::encode(Sha256::digest(job_id.as_bytes()))
    )
}

/// Supported llama.cpp GGUF quantization targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantizationTarget {
    /// Q4_K_M quantization.
    Q4Km,
    /// Q5_K_M quantization.
    Q5Km,
    /// Q8_0 quantization.
    Q8_0,
}

impl QuantizationTarget {
    /// Returns the fixed llama.cpp command-line spelling for this target.
    pub const fn llama_argument(self) -> &'static str {
        match self {
            Self::Q4Km => "Q4_K_M",
            Self::Q5Km => "Q5_K_M",
            Self::Q8_0 => "Q8_0",
        }
    }
}

/// Verified immutable identity of a model artifact managed by Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceModelIdentity {
    /// Existing model registry identity.
    pub model_id: String,
    /// Immutable source model revision.
    pub revision: u64,
    /// Source GGUF file SHA-256.
    pub artifact_sha256: String,
    /// Exact source GGUF byte size.
    pub artifact_size_bytes: u64,
    /// GGUF source quantization; v1 permits only F16 or F32.
    pub source_quantization: String,
}

/// Immutable request for one local quantization operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptationRequest {
    /// Stable operation identity.
    pub job_id: String,
    /// Caller idempotency key scoped to this operation.
    pub idempotency_key: String,
    /// Exact verified managed source identity.
    pub source: SourceModelIdentity,
    /// Requested quantization target.
    pub target: QuantizationTarget,
    /// Required frozen benchmark suite digest.
    pub benchmark_suite_sha256: String,
    /// Required compatible baseline digest.
    pub baseline_sha256: String,
    /// Maximum output size permitted for the operation.
    pub max_output_bytes: u64,
    /// Exact approval-policy profile used for promotion authorization.
    pub approval_policy_id: String,
    /// Canonical hash of the pinned approval-policy profile.
    pub approval_policy_sha256: String,
    /// Revision of the policy that approved this operation.
    pub policy_revision: u64,
}

/// Immutable identity of the one supported external adapter package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterIdentity {
    /// Adapter package identifier.
    pub adapter_id: String,
    /// Fixed upstream version.
    pub version: String,
    /// Exact release archive SHA-256.
    pub archive_sha256: String,
    /// Exact release archive size.
    pub archive_size_bytes: u64,
    /// Fixed upstream asset URL.
    pub asset_url: String,
}

/// Bounded status event emitted while the explicit adapter install runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterInstallProgress {
    /// Current installation phase.
    pub phase: &'static str,
    /// Bytes downloaded during the fixed-size release asset transfer.
    pub completed_bytes: u64,
    /// Exact pinned archive size.
    pub total_bytes: u64,
}

/// Bounded result of one streamed call to the supervised local model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalInferenceStreamEvidence {
    /// Version of the local OpenAI-compatible streaming contract.
    pub adapter_version: String,
    /// Model alias served by the verified loopback runtime.
    pub model_alias: String,
    /// SHA-256 of the accumulated completion; raw text is not persisted.
    pub completion_sha256: String,
    /// Number of completion bytes accumulated from SSE deltas.
    pub completion_bytes: u64,
    /// Provider-reported prompt token count when present.
    pub prompt_tokens: Option<u64>,
    /// Provider-reported completion token count when present.
    pub completion_tokens: Option<u64>,
    /// Optional exact-output evaluator result for a deterministic probe.
    pub expected_match: Option<bool>,
    /// End-to-end response time in milliseconds.
    pub latency_ms: u64,
}

/// Calls one Supervisor-selected loopback model using bounded OpenAI SSE.
///
/// The URL is constructed from the authenticated Supervisor response. Neither
/// the renderer nor an adaptation request can select a host or model endpoint.
pub async fn local_inference_stream(
    port: u16,
    model_alias: &str,
    prompt: &str,
    max_tokens: u32,
    expected_exact_completion: Option<&str>,
) -> Result<LocalInferenceStreamEvidence, AdaptationError> {
    const MAX_PROMPT_BYTES: usize = 16 * 1024;
    const MAX_COMPLETION_BYTES: usize = 64 * 1024;
    if port == 0
        || model_alias.len() != 35
        || !model_alias.starts_with("evohime-adaptation-")
        || prompt.is_empty()
        || prompt.len() > MAX_PROMPT_BYTES
        || max_tokens == 0
        || max_tokens > 4096
    {
        return Err(AdaptationError::InvalidRequest);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AdaptationError::InvalidRequest)?;
    let started = tokio::time::Instant::now();
    let mut response = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .json(&serde_json::json!({
            "model": model_alias,
            "messages": [{"role":"user","content":prompt}],
            "stream": true,
            "stream_options": {"include_usage": true},
            "temperature": 0,
            "max_tokens": max_tokens
        }))
        .send()
        .await
        .map_err(|_| AdaptationError::InvalidRequest)?;
    if !response.status().is_success() {
        return Err(AdaptationError::InvalidRequest);
    }
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut received_bytes = 0_usize;
    let mut completion = Vec::with_capacity(4096);
    let mut prompt_tokens = None;
    let mut completion_tokens = None;
    let mut finished = false;
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AdaptationError::InvalidRequest)?;
        received_bytes = received_bytes
            .checked_add(chunk.len())
            .filter(|size| *size <= 512 * 1024)
            .ok_or(AdaptationError::InvalidRequest)?;
        if pending.len().saturating_add(chunk.len()) > 128 * 1024 {
            return Err(AdaptationError::InvalidRequest);
        }
        pending.extend_from_slice(&chunk);
        while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
            let line = pending.drain(..=newline).collect::<Vec<_>>();
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let Some(data) = line.strip_prefix(b"data:") else {
                continue;
            };
            let data = data.strip_prefix(b" ").unwrap_or(data);
            if data == b"[DONE]" {
                finished = true;
                break;
            }
            let event: serde_json::Value =
                serde_json::from_slice(data).map_err(|_| AdaptationError::InvalidRequest)?;
            if let Some(text) = event
                .get("choices")
                .and_then(serde_json::Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("delta"))
                .and_then(|delta| delta.get("content"))
                .and_then(serde_json::Value::as_str)
            {
                if completion.len().saturating_add(text.len()) > MAX_COMPLETION_BYTES {
                    return Err(AdaptationError::InvalidRequest);
                }
                completion.extend_from_slice(text.as_bytes());
            }
            if let Some(usage) = event.get("usage") {
                prompt_tokens = usage
                    .get("prompt_tokens")
                    .and_then(serde_json::Value::as_u64);
                completion_tokens = usage
                    .get("completion_tokens")
                    .and_then(serde_json::Value::as_u64);
            }
        }
        if finished {
            break;
        }
    }
    if !finished || completion.is_empty() {
        return Err(AdaptationError::InvalidRequest);
    }
    let expected_match = if let Some(expected) = expected_exact_completion {
        let output =
            std::str::from_utf8(&completion).map_err(|_| AdaptationError::InvalidRequest)?;
        Some(output.trim() == expected)
    } else {
        None
    };
    Ok(LocalInferenceStreamEvidence {
        adapter_version: "llama-openai-sse-v1".into(),
        model_alias: model_alias.into(),
        completion_sha256: hex::encode(Sha256::digest(&completion)),
        completion_bytes: completion.len() as u64,
        prompt_tokens,
        completion_tokens,
        expected_match,
        latency_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    })
}

impl AdapterIdentity {
    /// Returns the only accepted adapter descriptor.
    pub fn pinned() -> Self {
        Self {
            adapter_id: "llama.cpp-cpu".into(),
            version: LLAMA_CPP_VERSION.into(),
            archive_sha256: LLAMA_CPP_ARCHIVE_SHA256.into(),
            archive_size_bytes: LLAMA_CPP_ARCHIVE_SIZE_BYTES,
            asset_url: LLAMA_CPP_ASSET_URL.into(),
        }
    }

    /// Rejects any descriptor that differs from the pinned upstream package.
    pub fn validate(&self) -> Result<(), AdaptationError> {
        if self != &Self::pinned() {
            return Err(AdaptationError::AdapterIdentityMismatch);
        }
        Ok(())
    }
}

/// Durable lifecycle state of one adaptation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptationState {
    /// Request persisted before preflight.
    Created,
    /// Source, policy, adapter and resources passed preflight.
    Preflighted,
    /// Waiting for bounded local disk or process capacity.
    WaitingForResources,
    /// Conversion process was dispatched.
    Running,
    /// Output is being checked structurally and by runtime load.
    Verifying,
    /// Real inference calibration and benchmark evidence is being collected.
    Benchmarking,
    /// All evidence is valid and explicit promotion is now available.
    ReadyForPromotion,
    /// Cancellation intent is persisted while Supervisor stops the process.
    Cancelling,
    /// Rejection intent is persisted while Supervisor stops the process.
    Rejecting,
    /// Explicit Core promotion completed.
    Promoted,
    /// Explicitly rejected by policy or user.
    Rejected,
    /// Explicitly cancelled before publication.
    Cancelled,
    /// Operation failed with a bounded reason code.
    Failed,
    /// Restart found a non-resumable in-flight external process.
    Interrupted,
}

impl AdaptationState {
    /// Stable storage key shared by SQLite and IPC projections.
    pub const fn storage_key(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Preflighted => "preflighted",
            Self::WaitingForResources => "waiting_for_resources",
            Self::Running => "running",
            Self::Verifying => "verifying",
            Self::Benchmarking => "benchmarking",
            Self::ReadyForPromotion => "ready_for_promotion",
            Self::Cancelling => "cancelling",
            Self::Rejecting => "rejecting",
            Self::Promoted => "promoted",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    /// Returns whether a state can no longer transition.
    pub const fn terminal(self) -> bool {
        matches!(
            self,
            Self::Promoted | Self::Rejected | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }

    /// Checks the only supported state transitions.
    pub const fn allows(self, next: Self) -> bool {
        use AdaptationState as S;
        matches!(
            (self, next),
            (
                S::Created,
                S::Preflighted
                    | S::Cancelling
                    | S::Rejecting
                    | S::Rejected
                    | S::Cancelled
                    | S::Failed
            ) | (
                S::Preflighted,
                S::WaitingForResources
                    | S::Running
                    | S::Cancelling
                    | S::Rejecting
                    | S::Rejected
                    | S::Cancelled
                    | S::Failed
            ) | (
                S::WaitingForResources,
                S::Running
                    | S::Cancelling
                    | S::Rejecting
                    | S::Cancelled
                    | S::Rejected
                    | S::Failed
                    | S::Interrupted
            ) | (
                S::Running,
                S::WaitingForResources
                    | S::Verifying
                    | S::Cancelling
                    | S::Rejecting
                    | S::Cancelled
                    | S::Rejected
                    | S::Failed
                    | S::Interrupted
            ) | (
                S::Verifying,
                S::Benchmarking
                    | S::Cancelling
                    | S::Rejecting
                    | S::Cancelled
                    | S::Rejected
                    | S::Failed
                    | S::Interrupted
            ) | (
                S::Benchmarking,
                S::Benchmarking
                    | S::ReadyForPromotion
                    | S::Cancelling
                    | S::Rejecting
                    | S::Cancelled
                    | S::Rejected
                    | S::Failed
                    | S::Interrupted
            ) | (
                S::ReadyForPromotion,
                S::Promoted | S::Cancelling | S::Rejecting | S::Rejected
            ) | (S::Cancelling, S::Cancelled)
                | (S::Rejecting, S::Rejected)
        )
    }
}

/// Bounded evidence snapshot attached to a persisted job revision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptationEvidence {
    /// SHA-256 of the exact quantizer output.
    pub output_sha256: Option<String>,
    /// Exact byte size of the quantizer output.
    pub output_size_bytes: Option<u64>,
    /// SHA-256 of validated runtime metadata and load probe.
    pub runtime_probe_sha256: Option<String>,
    /// SHA-256 of local performance calibration evidence.
    pub calibration_sha256: Option<String>,
    /// SHA-256 of the real-model benchmark report.
    pub benchmark_sha256: Option<String>,
    /// Whether a durable job revision has fenced the single benchmark dispatch.
    #[serde(default)]
    pub benchmark_started: bool,
}

/// Metadata-only durable adaptation job snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptationJob {
    /// Immutable original request.
    pub request: AdaptationRequest,
    /// Current lifecycle state.
    pub state: AdaptationState,
    /// Monotonic compare-and-set revision.
    pub revision: u64,
    /// Pinned converter identity.
    pub adapter: AdapterIdentity,
    /// Output evidence collected so far.
    pub evidence: AdaptationEvidence,
    /// Canonical content hash of this exact snapshot.
    pub content_sha256: String,
}

/// Invalid adaptation request, adapter, evidence or lifecycle transition.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AdaptationError {
    /// Request fields are malformed or exceed a fixed bound.
    #[error("invalid adaptation request")]
    InvalidRequest,
    /// Adapter differs from the immutable pinned release.
    #[error("adapter identity mismatch")]
    AdapterIdentityMismatch,
    /// Proposed state transition is not allowed.
    #[error("illegal adaptation state transition")]
    IllegalTransition,
    /// Job revision or evidence hash is stale or malformed.
    #[error("invalid adaptation evidence")]
    InvalidEvidence,
    /// Pinned adapter download, verification or installation failed.
    #[error("pinned adapter installation failed")]
    AdapterInstall,
    /// Source file is not GGUF or its tensors disagree with the F16/F32 claim.
    #[error("source GGUF format or tensor precision is invalid")]
    InvalidSourceModel,
}

/// Parses a bounded GGUF header and verifies that every tensor is F16 or F32
/// with the precision declared by the managed model registry.
pub fn verify_gguf_source(path: &Path, source_quantization: &str) -> Result<(), AdaptationError> {
    let expected_file_type = match source_quantization {
        "F32" => 0,
        "F16" => 1,
        _ => return Err(AdaptationError::InvalidSourceModel),
    };
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| AdaptationError::InvalidSourceModel)?;
    if !metadata.file_type().is_file() || metadata.len() < 24 {
        return Err(AdaptationError::InvalidSourceModel);
    }
    let file_length = metadata.len();
    let mut file = File::open(path).map_err(|_| AdaptationError::InvalidSourceModel)?;
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic)
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    if &magic != b"GGUF" {
        return Err(AdaptationError::InvalidSourceModel);
    }
    let version = read_u32(&mut file)?;
    if !(2..=3).contains(&version) {
        return Err(AdaptationError::InvalidSourceModel);
    }
    let tensor_count = read_u64(&mut file)?;
    let metadata_count = read_u64(&mut file)?;
    if tensor_count == 0
        || tensor_count > 1_000_000
        || metadata_count == 0
        || metadata_count > 100_000
    {
        return Err(AdaptationError::InvalidSourceModel);
    }
    let mut parsed_values = 0_u64;
    let mut observed_file_type = None;
    for _ in 0..metadata_count {
        let key = read_string(&mut file, file_length, 65_535)?;
        let value_type = read_u32(&mut file)?;
        if key == "general.file_type" {
            if observed_file_type.is_some() || value_type != 4 {
                return Err(AdaptationError::InvalidSourceModel);
            }
            observed_file_type = Some(read_u32(&mut file)?);
        } else {
            skip_metadata_value(&mut file, file_length, value_type, 0, &mut parsed_values)?;
        }
    }
    if observed_file_type != Some(expected_file_type) {
        return Err(AdaptationError::InvalidSourceModel);
    }
    let mut expected_tensor_count = 0_u64;
    let mut other_float_tensor_count = 0_u64;
    for _ in 0..tensor_count {
        let _name = read_string(&mut file, file_length, 64)?;
        let dimensions = read_u32(&mut file)?;
        if !(1..=4).contains(&dimensions) {
            return Err(AdaptationError::InvalidSourceModel);
        }
        for _ in 0..dimensions {
            if read_u64(&mut file)? == 0 {
                return Err(AdaptationError::InvalidSourceModel);
            }
        }
        let tensor_type = read_u32(&mut file)?;
        match tensor_type {
            0 => {
                if expected_file_type == 0 {
                    expected_tensor_count += 1;
                } else {
                    other_float_tensor_count += 1;
                }
            }
            1 => {
                if expected_file_type == 1 {
                    expected_tensor_count += 1;
                } else {
                    other_float_tensor_count += 1;
                }
            }
            _ => return Err(AdaptationError::InvalidSourceModel),
        }
        if expected_tensor_count + other_float_tensor_count > tensor_count {
            return Err(AdaptationError::InvalidSourceModel);
        }
        let tensor_offset = read_u64(&mut file)?;
        if tensor_offset >= file_length {
            return Err(AdaptationError::InvalidSourceModel);
        }
        ensure_within_file(&mut file, file_length)?;
    }
    if expected_tensor_count <= other_float_tensor_count {
        return Err(AdaptationError::InvalidSourceModel);
    }
    Ok(())
}

fn read_u32(file: &mut File) -> Result<u32, AdaptationError> {
    let mut bytes = [0_u8; 4];
    file.read_exact(&mut bytes)
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(file: &mut File) -> Result<u64, AdaptationError> {
    let mut bytes = [0_u8; 8];
    file.read_exact(&mut bytes)
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    Ok(u64::from_le_bytes(bytes))
}

fn ensure_within_file(file: &mut File, file_length: u64) -> Result<(), AdaptationError> {
    if file
        .stream_position()
        .map_err(|_| AdaptationError::InvalidSourceModel)?
        > file_length
    {
        Err(AdaptationError::InvalidSourceModel)
    } else {
        Ok(())
    }
}

fn skip_bytes(file: &mut File, file_length: u64, amount: u64) -> Result<(), AdaptationError> {
    let position = file
        .stream_position()
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    let end = position
        .checked_add(amount)
        .filter(|end| *end <= file_length)
        .ok_or(AdaptationError::InvalidSourceModel)?;
    file.seek(SeekFrom::Start(end))
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    Ok(())
}

fn read_string(
    file: &mut File,
    file_length: u64,
    max_bytes: u64,
) -> Result<String, AdaptationError> {
    let length = read_u64(file)?;
    if length == 0 || length > max_bytes {
        return Err(AdaptationError::InvalidSourceModel);
    }
    let position = file
        .stream_position()
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    let end = position
        .checked_add(length)
        .filter(|end| *end <= file_length)
        .ok_or(AdaptationError::InvalidSourceModel)?;
    let capacity = usize::try_from(length).map_err(|_| AdaptationError::InvalidSourceModel)?;
    let mut bytes = vec![0; capacity];
    file.read_exact(&mut bytes)
        .map_err(|_| AdaptationError::InvalidSourceModel)?;
    if file
        .stream_position()
        .map_err(|_| AdaptationError::InvalidSourceModel)?
        != end
    {
        return Err(AdaptationError::InvalidSourceModel);
    }
    String::from_utf8(bytes).map_err(|_| AdaptationError::InvalidSourceModel)
}

fn skip_metadata_value(
    file: &mut File,
    file_length: u64,
    value_type: u32,
    depth: u8,
    parsed_values: &mut u64,
) -> Result<(), AdaptationError> {
    *parsed_values = parsed_values
        .checked_add(1)
        .filter(|count| *count <= 1_000_000)
        .ok_or(AdaptationError::InvalidSourceModel)?;
    let bytes = match value_type {
        0 | 1 | 7 => 1,
        2 | 3 => 2,
        4..=6 => 4,
        8 => {
            let length = read_u64(file)?;
            skip_bytes(file, file_length, length)?;
            return Ok(());
        }
        9 => {
            if depth >= 8 {
                return Err(AdaptationError::InvalidSourceModel);
            }
            let element_type = read_u32(file)?;
            let count = read_u64(file)?;
            if count > 1_000_000 {
                return Err(AdaptationError::InvalidSourceModel);
            }
            for _ in 0..count {
                skip_metadata_value(file, file_length, element_type, depth + 1, parsed_values)?;
            }
            return Ok(());
        }
        10..=12 => 8,
        _ => return Err(AdaptationError::InvalidSourceModel),
    };
    skip_bytes(file, file_length, bytes)
}

/// Explicitly downloads, verifies and installs the fixed llama.cpp CPU package.
///
/// This function is called only by the authenticated `install_adapter`
/// operation. It accepts no caller-controlled URL or destination.
static ADAPTER_INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Downloads, verifies and installs the fixed llama.cpp CPU package.
///
/// This operation accepts no caller-controlled URL or destination. Progress
/// is reported through `progress`, and the returned path identifies the
/// validated versioned installation under `data_root`.
///
/// # Errors
///
/// Returns [`AdaptationError::AdapterInstall`] when the installation path,
/// pinned archive, extraction, or post-install validation fails.
pub async fn install_pinned_adapter(
    data_root: &Path,
    progress: tokio::sync::mpsc::UnboundedSender<AdapterInstallProgress>,
) -> Result<PathBuf, AdaptationError> {
    let _install_guard = ADAPTER_INSTALL_LOCK.lock().await;
    let data_root =
        std::fs::canonicalize(data_root).map_err(|_| AdaptationError::AdapterInstall)?;
    let tools_root = data_root.join("tools");
    std::fs::create_dir_all(&tools_root).map_err(|_| AdaptationError::AdapterInstall)?;
    if unsafe_install_path_metadata(
        &std::fs::symlink_metadata(&tools_root).map_err(|_| AdaptationError::AdapterInstall)?,
    ) {
        return Err(AdaptationError::AdapterInstall);
    }
    let staging = tools_root.join(".llama-cpp-b10981-staging");
    let archive = tools_root.join(".llama-cpp-b10981.zip");
    let destination = tools_root.join("llama.cpp").join(LLAMA_CPP_VERSION);
    if destination.exists() {
        return adapter_install_is_valid(&destination)
            .then_some(destination)
            .ok_or(AdaptationError::AdapterInstall);
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&staging) {
        if !metadata.file_type().is_dir() || unsafe_install_path_metadata(&metadata) {
            return Err(AdaptationError::AdapterInstall);
        }
        std::fs::remove_dir_all(&staging).map_err(|_| AdaptationError::AdapterInstall)?;
    }
    if std::fs::symlink_metadata(&archive).is_ok() {
        let metadata =
            std::fs::symlink_metadata(&archive).map_err(|_| AdaptationError::AdapterInstall)?;
        if !metadata.file_type().is_file() || unsafe_install_path_metadata(&metadata) {
            return Err(AdaptationError::AdapterInstall);
        }
        std::fs::remove_file(&archive).map_err(|_| AdaptationError::AdapterInstall)?;
    }
    let _ = progress.send(AdapterInstallProgress {
        phase: "downloading",
        completed_bytes: 0,
        total_bytes: LLAMA_CPP_ARCHIVE_SIZE_BYTES,
    });
    if let Err(error) = download_pinned_adapter(&archive, &progress).await {
        let _ = std::fs::remove_file(&archive);
        return Err(error);
    }
    let _ = progress.send(AdapterInstallProgress {
        phase: "verifying_and_extracting",
        completed_bytes: LLAMA_CPP_ARCHIVE_SIZE_BYTES,
        total_bytes: LLAMA_CPP_ARCHIVE_SIZE_BYTES,
    });
    let extracted = staging.clone();
    let verified_archive = archive.clone();
    let published_directory = destination.clone();
    let install_result = tokio::task::spawn_blocking(move || {
        let extract_result = evohime_tool_runtime::archive::extract_verified_zip(
            &verified_archive,
            &extracted,
            LLAMA_CPP_ARCHIVE_SHA256,
            LLAMA_CPP_ARCHIVE_SIZE_BYTES,
        );
        if extract_result.is_err() {
            let _ = std::fs::remove_file(&verified_archive);
            return Err(AdaptationError::AdapterInstall);
        }
        let manifest = serde_json::json!({
            "adapter_id": "llama.cpp-cpu",
            "version": LLAMA_CPP_VERSION,
            "archive_sha256": LLAMA_CPP_ARCHIVE_SHA256,
            "archive_size_bytes": LLAMA_CPP_ARCHIVE_SIZE_BYTES,
        });
        let manifest_path = extracted.join("evohime-adapter.json");
        let manifest_bytes =
            serde_json::to_vec(&manifest).map_err(|_| AdaptationError::AdapterInstall)?;
        std::fs::write(&manifest_path, manifest_bytes)
            .map_err(|_| AdaptationError::AdapterInstall)?;
        std::fs::write(
            extracted.join("LICENSE-llama.cpp"),
            include_str!("../../../docs/licenses/llama.cpp-MIT.txt"),
        )
        .map_err(|_| AdaptationError::AdapterInstall)?;
        if !adapter_install_is_valid(&extracted) {
            let _ = std::fs::remove_dir_all(&extracted);
            let _ = std::fs::remove_file(&verified_archive);
            return Err(AdaptationError::AdapterInstall);
        }
        let parent = published_directory
            .parent()
            .ok_or(AdaptationError::AdapterInstall)?;
        if let Ok(metadata) = std::fs::symlink_metadata(parent) {
            if unsafe_install_path_metadata(&metadata) || !metadata.file_type().is_dir() {
                return Err(AdaptationError::AdapterInstall);
            }
        } else {
            std::fs::create_dir(parent).map_err(|_| AdaptationError::AdapterInstall)?;
        }
        if published_directory.exists() {
            return Err(AdaptationError::AdapterInstall);
        }
        std::fs::rename(&extracted, &published_directory)
            .map_err(|_| AdaptationError::AdapterInstall)?;
        std::fs::remove_file(&verified_archive).map_err(|_| AdaptationError::AdapterInstall)?;
        Ok(published_directory)
    })
    .await
    .map_err(|_| AdaptationError::AdapterInstall)?;
    install_result
}

fn unsafe_install_path_metadata(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

async fn download_pinned_adapter(
    archive_path: &Path,
    progress: &tokio::sync::mpsc::UnboundedSender<AdapterInstallProgress>,
) -> Result<(), AdaptationError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| AdaptationError::AdapterInstall)?;
    let mut target =
        reqwest::Url::parse(LLAMA_CPP_ASSET_URL).map_err(|_| AdaptationError::AdapterInstall)?;
    let mut response = None;
    for _ in 0..=3 {
        let current = client
            .get(target.clone())
            .send()
            .await
            .map_err(|_| AdaptationError::AdapterInstall)?;
        if current.status().is_redirection() {
            let next = current
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| target.join(value).ok())
                .ok_or(AdaptationError::AdapterInstall)?;
            if !allowed_asset_redirect(&next) {
                return Err(AdaptationError::AdapterInstall);
            }
            target = next;
            continue;
        }
        if !current.status().is_success()
            || current
                .content_length()
                .is_some_and(|length| length != LLAMA_CPP_ARCHIVE_SIZE_BYTES)
        {
            return Err(AdaptationError::AdapterInstall);
        }
        response = Some(current);
        break;
    }
    let response = response.ok_or(AdaptationError::AdapterInstall)?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(archive_path)
        .await
        .map_err(|_| AdaptationError::AdapterInstall)?;
    let mut downloaded = 0_u64;
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AdaptationError::AdapterInstall)?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > LLAMA_CPP_ARCHIVE_SIZE_BYTES {
            return Err(AdaptationError::AdapterInstall);
        }
        file.write_all(&chunk)
            .await
            .map_err(|_| AdaptationError::AdapterInstall)?;
        let _ = progress.send(AdapterInstallProgress {
            phase: "downloading",
            completed_bytes: downloaded,
            total_bytes: LLAMA_CPP_ARCHIVE_SIZE_BYTES,
        });
    }
    file.flush()
        .await
        .map_err(|_| AdaptationError::AdapterInstall)?;
    if downloaded != LLAMA_CPP_ARCHIVE_SIZE_BYTES {
        return Err(AdaptationError::AdapterInstall);
    }
    Ok(())
}

fn allowed_asset_redirect(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && (url.host_str() == Some("github.com")
            && url.path()
                == "/ggml-org/llama.cpp/releases/download/b10981/llama-b10981-bin-win-cpu-x64.zip"
            || url.host_str() == Some("release-assets.githubusercontent.com"))
}

#[cfg(test)]
fn file_sha256_matches(path: &Path, expected: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.len() > 16 * 1024 * 1024 {
        return false;
    }
    std::fs::read(path).is_ok_and(|bytes| hex::encode(Sha256::digest(bytes)) == expected)
}

pub(crate) fn adapter_install_is_valid(directory: &Path) -> bool {
    let Some(install_parent) = directory.parent() else {
        return false;
    };
    let Some(tools_root) = install_parent.parent() else {
        return false;
    };
    if !std::fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.file_type().is_dir())
        || !std::fs::symlink_metadata(install_parent)
            .is_ok_and(|metadata| metadata.file_type().is_dir())
        || !std::fs::symlink_metadata(tools_root)
            .is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return false;
    }
    let files_valid = evohime_desktop_ipc::local_adapter_contract::verify_runtime_files(directory)
        && std::fs::symlink_metadata(directory.join("LICENSE-llama.cpp"))
            .is_ok_and(|metadata| metadata.file_type().is_file());
    if !files_valid {
        return false;
    }
    std::fs::read(directory.join("evohime-adapter.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|manifest| {
            manifest
                .get("adapter_id")
                .and_then(serde_json::Value::as_str)
                == Some("llama.cpp-cpu")
                && manifest.get("version").and_then(serde_json::Value::as_str)
                    == Some(LLAMA_CPP_VERSION)
                && manifest
                    .get("archive_sha256")
                    .and_then(serde_json::Value::as_str)
                    == Some(LLAMA_CPP_ARCHIVE_SHA256)
                && manifest
                    .get("archive_size_bytes")
                    .and_then(serde_json::Value::as_u64)
                    == Some(LLAMA_CPP_ARCHIVE_SIZE_BYTES)
        })
}

impl AdaptationRequest {
    /// Validates all caller-controlled identity and resource bounds.
    pub fn validate(&self) -> Result<(), AdaptationError> {
        if !valid_id(&self.job_id)
            || !valid_id(&self.idempotency_key)
            || !valid_id(&self.source.model_id)
            || self.source.revision == 0
            || !valid_hash(&self.source.artifact_sha256)
            || self.source.artifact_size_bytes == 0
            || self.source.artifact_size_bytes > MAX_OUTPUT_BYTES
            || !matches!(self.source.source_quantization.as_str(), "F16" | "F32")
            || !valid_hash(&self.benchmark_suite_sha256)
            || !valid_hash(&self.baseline_sha256)
            || self.max_output_bytes == 0
            || self.max_output_bytes > MAX_OUTPUT_BYTES
            || !valid_id(&self.approval_policy_id)
            || !valid_hash(&self.approval_policy_sha256)
            || self.policy_revision == 0
        {
            return Err(AdaptationError::InvalidRequest);
        }
        Ok(())
    }

    /// Computes a stable hash of the exact serialized request.
    pub fn content_sha256(&self) -> Result<String, AdaptationError> {
        self.validate()?;
        hash_json(self)
    }
}

impl AdaptationJob {
    /// Creates a revision-one job tied to the one immutable adapter package.
    pub fn create(request: AdaptationRequest) -> Result<Self, AdaptationError> {
        request.validate()?;
        let mut job = Self {
            request,
            state: AdaptationState::Created,
            revision: 1,
            adapter: AdapterIdentity::pinned(),
            evidence: AdaptationEvidence::default(),
            content_sha256: String::new(),
        };
        job.refresh_hash()?;
        Ok(job)
    }

    /// Advances one allowed state and revision with an updated evidence snapshot.
    pub fn transition(
        &mut self,
        next: AdaptationState,
        evidence: AdaptationEvidence,
    ) -> Result<(), AdaptationError> {
        if !self.state.allows(next) || self.revision == u64::MAX {
            return Err(AdaptationError::IllegalTransition);
        }
        validate_evidence(&evidence, self.request.max_output_bytes)?;
        if next == AdaptationState::ReadyForPromotion
            && (evidence.output_sha256.is_none()
                || evidence.runtime_probe_sha256.is_none()
                || evidence.calibration_sha256.is_none()
                || evidence.benchmark_sha256.is_none())
        {
            return Err(AdaptationError::InvalidEvidence);
        }
        if next == AdaptationState::Benchmarking
            && evidence.benchmark_started
            && evidence.calibration_sha256.is_none()
        {
            return Err(AdaptationError::InvalidEvidence);
        }
        self.state = next;
        self.revision += 1;
        self.evidence = evidence;
        self.refresh_hash()
    }

    /// Validates the snapshot and its canonical content hash.
    pub fn validate(&self) -> Result<(), AdaptationError> {
        self.request.validate()?;
        self.adapter.validate()?;
        validate_evidence(&self.evidence, self.request.max_output_bytes)?;
        if self.revision == 0 || self.content_sha256 != self.compute_hash()? {
            return Err(AdaptationError::InvalidEvidence);
        }
        if self.state == AdaptationState::ReadyForPromotion
            && (self.evidence.output_sha256.is_none()
                || self.evidence.runtime_probe_sha256.is_none()
                || self.evidence.calibration_sha256.is_none()
                || self.evidence.benchmark_sha256.is_none())
        {
            return Err(AdaptationError::InvalidEvidence);
        }
        if self.state == AdaptationState::Benchmarking
            && self.evidence.benchmark_started
            && self.evidence.calibration_sha256.is_none()
        {
            return Err(AdaptationError::InvalidEvidence);
        }
        Ok(())
    }

    fn refresh_hash(&mut self) -> Result<(), AdaptationError> {
        self.content_sha256 = self.compute_hash()?;
        Ok(())
    }

    fn compute_hash(&self) -> Result<String, AdaptationError> {
        let mut snapshot = self.clone();
        snapshot.content_sha256.clear();
        let bytes = serde_json::to_vec(&snapshot).map_err(|_| AdaptationError::InvalidEvidence)?;
        let mut hasher = Sha256::new();
        hasher.update(CONTRACT_ID.as_bytes());
        hasher.update(bytes);
        Ok(hex::encode(hasher.finalize()))
    }
}

fn validate_evidence(
    evidence: &AdaptationEvidence,
    max_output_bytes: u64,
) -> Result<(), AdaptationError> {
    for hash in [
        evidence.output_sha256.as_deref(),
        evidence.runtime_probe_sha256.as_deref(),
        evidence.calibration_sha256.as_deref(),
        evidence.benchmark_sha256.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !valid_hash(hash) {
            return Err(AdaptationError::InvalidEvidence);
        }
    }
    match (evidence.output_sha256.as_ref(), evidence.output_size_bytes) {
        (Some(_), Some(size)) if size > 0 && size <= max_output_bytes => Ok(()),
        (None, None) => Ok(()),
        _ => Err(AdaptationError::InvalidEvidence),
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Reconciles in-flight local model adaptation work after a Core restart.
///
/// Quantization has no resumable checkpoint in v1, so a running job is
/// stopped, its partial staging output is moved to quarantine, and the durable
/// job becomes terminal `Interrupted`. Verification is retried from its
/// already-hashed output. A benchmark with frozen durable inputs remains in
/// `Benchmarking`; an explicit retry reuses its incomplete run after restarting
/// the supervised runtime.
#[cfg(windows)]
pub async fn recover_after_restart(
    journal: &crate::EventJournal,
    models_root: &Path,
) -> Result<u32, String> {
    let summaries = {
        let database = journal.database().lock().await;
        evohime_local_storage::local_model_adaptation_store::list_jobs(
            database.connection(),
            MAX_JOBS as u32,
        )
        .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
    };
    let mut recovered = 0_u32;
    for (job_id, row_revision, row_state, snapshot) in summaries {
        let mut job: AdaptationJob = serde_json::from_slice(&snapshot)
            .map_err(|_| "adaptation_recovery_corrupt_job".to_string())?;
        job.validate()
            .map_err(|_| "adaptation_recovery_corrupt_job".to_string())?;
        if job.request.job_id != job_id
            || job.revision != row_revision
            || job.state.storage_key() != row_state
        {
            return Err("adaptation_recovery_integrity_failed".into());
        }
        if reconcile_prepared_publication(journal, models_root, &mut job, row_revision).await? {
            recovered = recovered.saturating_add(1);
            continue;
        }
        match job.state {
            AdaptationState::Cancelling | AdaptationState::Rejecting => {
                stop_supervisor_adaptation(serde_json::json!({
                    "op":"adaptation_quantize_cancel", "job_id":job_id
                }))
                .await?;
                stop_supervisor_adaptation(serde_json::json!({
                    "op":"adaptation_runtime_stop", "job_id":job_id
                }))
                .await?;
                let next = if job.state == AdaptationState::Cancelling {
                    AdaptationState::Cancelled
                } else {
                    AdaptationState::Rejected
                };
                job.transition(next, job.evidence.clone())
                    .map_err(|_| "adaptation_recovery_transition_denied".to_string())?;
                let snapshot = serde_json::to_vec(&job)
                    .map_err(|_| "adaptation_recovery_serialization_failed".to_string())?;
                let mut database = journal.database().lock().await;
                let transaction = database
                    .connection_mut()
                    .transaction()
                    .map_err(|_| "adaptation_recovery_transaction_failed".to_string())?;
                let current = evohime_local_storage::local_model_adaptation_store::get_job(
                    &transaction,
                    &job_id,
                )
                .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
                .ok_or_else(|| "adaptation_recovery_job_missing".to_string())?;
                if current.0 != row_revision
                    || !evohime_local_storage::local_model_adaptation_store::put_job(
                        &transaction,
                        &job_id,
                        job.revision,
                        job.state.storage_key(),
                        &job.request.idempotency_key,
                        &current.2,
                        &job.content_sha256,
                        &current.4,
                        &snapshot,
                        crate::task_memory::now_millis() as i64,
                    )
                    .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
                {
                    return Err("adaptation_recovery_revision_conflict".into());
                }
                if let Some((_, _, run_state, _)) =
                    evohime_local_storage::domains::evaluation::get_run(&transaction, &job_id)
                        .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
                {
                    if run_state == "running" {
                        let report = serde_json::to_string(&serde_json::json!({
                            "status":job.state.storage_key(),"redacted":true
                        }))
                        .map_err(|_| "adaptation_recovery_serialization_failed".to_string())?;
                        if !evohime_local_storage::domains::evaluation::save_report(
                            &transaction,
                            &job_id,
                            &report,
                            job.state.storage_key(),
                            crate::task_memory::now_millis() as i64,
                        )
                        .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
                        {
                            return Err("adaptation_recovery_benchmark_run_missing".into());
                        }
                    }
                }
                transaction
                    .commit()
                    .map_err(|_| "adaptation_recovery_transaction_failed".to_string())?;
                recovered = recovered.saturating_add(1);
            }
            AdaptationState::Preflighted => {
                stop_supervisor_adaptation(serde_json::json!({
                    "op":"adaptation_quantize_cancel", "job_id":job_id
                }))
                .await?;
            }
            AdaptationState::Running => {
                stop_supervisor_adaptation(serde_json::json!({
                    "op":"adaptation_quantize_cancel", "job_id":job_id
                }))
                .await?;
                quarantine_partial_staging(models_root, &job, row_revision)?;
                recover_interrupted_job(journal, &mut job, row_revision).await?;
                recovered = recovered.saturating_add(1);
            }
            AdaptationState::Verifying => {
                stop_supervisor_adaptation(serde_json::json!({
                    "op":"adaptation_runtime_stop", "job_id":job_id
                }))
                .await?;
            }
            AdaptationState::Benchmarking if job.evidence.benchmark_started => {
                let (run, stored_policy, frozen_input) = {
                    let database = journal.database().lock().await;
                    (
                        evohime_local_storage::domains::evaluation::get_run(
                            database.connection(),
                            &job_id,
                        )
                        .map_err(|_| "adaptation_recovery_benchmark_read_failed".to_string())?,
                        evohime_local_storage::domains::evaluation::get_run_policy_json(
                            database.connection(),
                            &job_id,
                        )
                        .map_err(|_| "adaptation_recovery_benchmark_read_failed".to_string())?,
                        evohime_local_storage::local_model_adaptation_store::get_benchmark_inputs(
                            database.connection(),
                            &job_id,
                        )
                        .map_err(|_| "adaptation_recovery_benchmark_read_failed".to_string())?,
                    )
                };
                let run = run.ok_or_else(|| "adaptation_recovery_benchmark_missing".to_string())?;
                let stored_policy = stored_policy
                    .ok_or_else(|| "adaptation_recovery_benchmark_policy_missing".to_string())?;
                let (input_hash, input_json) = frozen_input
                    .ok_or_else(|| "adaptation_recovery_benchmark_input_missing".to_string())?;
                if run.2 != "running"
                    || run.3.is_some()
                    || input_json.len() > 192 * 1024
                    || input_hash != crate::local_model_runtime_manager::canonical_hash(&input_json)
                {
                    return Err("adaptation_recovery_benchmark_identity_conflict".into());
                }
                let (suite, policy, baselines): (
                    crate::agent_benchmark_matrix::BenchmarkSuite,
                    crate::agent_benchmark_matrix::BenchmarkPolicy,
                    std::collections::BTreeMap<String, crate::agent_benchmark_matrix::Baseline>,
                ) = serde_json::from_slice(&input_json)
                    .map_err(|_| "adaptation_recovery_benchmark_input_corrupt".to_string())?;
                if run.0 != suite.id
                    || run.1 != suite.version
                    || suite
                        .canonical_hash()
                        .map_err(|_| "adaptation_recovery_benchmark_input_corrupt".to_string())?
                        != job.request.benchmark_suite_sha256
                    || crate::local_model_runtime_manager::canonical_hash(&(&policy, &baselines))
                        != job.request.baseline_sha256
                    || policy.mode != crate::agent_benchmark_matrix::BenchmarkMode::Real
                    || stored_policy
                        != serde_json::to_string(&(&policy, &baselines)).map_err(|_| {
                            "adaptation_recovery_benchmark_input_corrupt".to_string()
                        })?
                {
                    return Err("adaptation_recovery_benchmark_identity_conflict".into());
                }
                stop_supervisor_adaptation(serde_json::json!({
                    "op":"adaptation_runtime_stop", "job_id":job_id
                }))
                .await?;
                // The exact suite, policy, baselines and run identity are durable,
                // so Core can explicitly redispatch the benchmark after restart.
                // Leave the state and `running` run row intact for that replay.
                recovered = recovered.saturating_add(1);
            }
            _ => {}
        }
    }
    Ok(recovered)
}

#[cfg(windows)]
async fn reconcile_prepared_publication(
    journal: &crate::EventJournal,
    models_root: &Path,
    job: &mut AdaptationJob,
    row_revision: u64,
) -> Result<bool, String> {
    let database = journal.database().lock().await;
    let Some(publication) = evohime_local_storage::local_model_adaptation_store::get_publication(
        database.connection(),
        &job.request.job_id,
    )
    .map_err(|_| "adaptation_recovery_publication_read_failed".to_string())?
    else {
        return Ok(false);
    };
    if publication.5 == "registered" {
        if job.state != AdaptationState::Promoted {
            return Err("adaptation_recovery_registered_publication_conflict".into());
        }
        return Ok(false);
    }
    if publication.5 != "prepared"
        || job.state != AdaptationState::ReadyForPromotion
        || job.revision != row_revision
    {
        return Ok(false);
    }
    let expected_output = job
        .evidence
        .output_sha256
        .as_deref()
        .ok_or_else(|| "adaptation_recovery_output_missing".to_string())?;
    let expected_benchmark = job
        .evidence
        .benchmark_sha256
        .as_deref()
        .ok_or_else(|| "adaptation_recovery_benchmark_missing".to_string())?;
    let output_size = job
        .evidence
        .output_size_bytes
        .ok_or_else(|| "adaptation_recovery_output_size_missing".to_string())?;
    let expected_model_id = format!(
        "adapt-{}-{}",
        &crate::local_model_runtime_manager::canonical_hash(&job.request.job_id)[..16],
        job.request.target.llama_argument().to_ascii_lowercase()
    );
    let expected_relative_path = format!("adapted/{expected_model_id}/1.gguf");
    if publication.0 != expected_model_id
        || publication.1 != 1
        || publication.2 != expected_relative_path
        || publication.3 != expected_output
        || publication.4 != output_size
    {
        return Err("adaptation_recovery_publication_identity_conflict".into());
    }
    let source_id = format!(
        "model:{}:{}",
        job.request.source.model_id, job.request.source.revision
    );
    let source_row = evohime_local_storage::local_model_runtime_manager_store::get_record(
        database.connection(),
        &source_id,
    )
    .map_err(|_| "adaptation_recovery_source_read_failed".to_string())?
    .ok_or_else(|| "adaptation_recovery_source_missing".to_string())?;
    let source: crate::local_model_runtime_manager::LocalModelDescriptor =
        serde_json::from_slice(&source_row.3)
            .map_err(|_| "adaptation_recovery_source_corrupt".to_string())?;
    if source_row.0 != "model"
        || source_row.1 != job.request.source.revision
        || source_row.2 != crate::local_model_runtime_manager::canonical_hash(&source_row.3)
        || source.model_id != job.request.source.model_id
        || source.revision != job.request.source.revision
        || source.artifact_hash != job.request.source.artifact_sha256
    {
        return Err("adaptation_recovery_source_identity_conflict".into());
    }
    let model = crate::local_model_runtime_manager::LocalModelDescriptor {
        model_id: expected_model_id.clone(),
        revision: 1,
        format: "gguf".into(),
        quantization: job.request.target.llama_argument().into(),
        artifact_size_bytes: output_size,
        artifact_hash: expected_output.into(),
        required_ram_bytes: source.required_ram_bytes,
        required_accelerator_bytes: None,
        context_limit: source.context_limit,
        capabilities: source.capabilities,
        trust: crate::local_model_runtime_manager::TrustLevel::ManagedVerified,
    };
    model
        .validate()
        .map_err(|_| "adaptation_recovery_model_invalid".to_string())?;
    let artifact = crate::local_model_runtime_manager::LocalArtifactRecord {
        model_id: expected_model_id.clone(),
        model_revision: 1,
        relative_path: Some(expected_relative_path.clone()),
        expected_hash: expected_output.into(),
        expected_size_bytes: output_size,
        state: crate::local_model_runtime_manager::ArtifactState::Installed,
        content_hash: Some(expected_output.into()),
    };
    artifact
        .validate()
        .map_err(|_| "adaptation_recovery_artifact_invalid".to_string())?;
    let model_json = serde_json::to_vec(&model)
        .map_err(|_| "adaptation_recovery_serialization_failed".to_string())?;
    let artifact_json = serde_json::to_vec(&artifact)
        .map_err(|_| "adaptation_recovery_serialization_failed".to_string())?;
    let publication_hash = crate::local_model_runtime_manager::canonical_hash(&(
        &job.request,
        &model,
        &artifact,
        expected_output,
        expected_benchmark,
    ));
    if publication.6 != publication_hash {
        return Err("adaptation_recovery_publication_hash_conflict".into());
    }
    drop(database);
    let destination = crate::local_model_runtime_manager::managed_artifact_path(
        models_root,
        Path::new(&expected_relative_path),
    )
    .map_err(|_| "adaptation_recovery_destination_invalid".to_string())?;
    let stage_relative = staging_relative_path(&job.request.job_id);
    let stage = crate::local_model_runtime_manager::managed_artifact_path(
        models_root,
        Path::new(&stage_relative),
    )
    .map_err(|_| "adaptation_recovery_staging_invalid".to_string())?;
    let root = models_root.to_path_buf();
    let relative = expected_relative_path.clone();
    let stage_for_worker = stage.clone();
    let destination_for_worker = destination.clone();
    let hash_for_worker = expected_output.to_owned();
    tokio::task::spawn_blocking(move || {
        if destination_for_worker.exists() {
            crate::local_model_runtime_manager::verify_managed_artifact(
                &root,
                Path::new(&relative),
                &hash_for_worker,
                output_size,
            )
            .map(|_| ())
        } else {
            crate::local_model_runtime_manager::atomic_promote_verified_artifact(
                &stage_for_worker,
                &destination_for_worker,
                &hash_for_worker,
                output_size,
            )
        }
    })
    .await
    .map_err(|_| "adaptation_recovery_publication_worker_failed".to_string())?
    .map_err(|_| "adaptation_recovery_artifact_unverified".to_string())?;
    let mut promoted_job = job.clone();
    promoted_job
        .transition(AdaptationState::Promoted, promoted_job.evidence.clone())
        .map_err(|_| "adaptation_recovery_transition_denied".to_string())?;
    let snapshot = serde_json::to_vec(&promoted_job)
        .map_err(|_| "adaptation_recovery_serialization_failed".to_string())?;
    let mut database = journal.database().lock().await;
    let transaction = database
        .connection_mut()
        .transaction()
        .map_err(|_| "adaptation_recovery_transaction_failed".to_string())?;
    let current = evohime_local_storage::local_model_adaptation_store::get_job(
        &transaction,
        &job.request.job_id,
    )
    .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
    .ok_or_else(|| "adaptation_recovery_job_missing".to_string())?;
    if current.0 != row_revision || current.1 != AdaptationState::ReadyForPromotion.storage_key() {
        return Err("adaptation_recovery_revision_conflict".into());
    }
    for (record_id, kind, json) in [
        (
            format!("model:{}:1", expected_model_id),
            "model",
            model_json,
        ),
        (
            format!("artifact:{}:1", expected_model_id),
            "artifact",
            artifact_json,
        ),
    ] {
        let hash = crate::local_model_runtime_manager::canonical_hash(&json);
        if let Some(existing) =
            evohime_local_storage::local_model_runtime_manager_store::get_record(
                &transaction,
                &record_id,
            )
            .map_err(|_| "adaptation_recovery_registry_read_failed".to_string())?
        {
            if existing.0 != kind || existing.1 != 1 || existing.2 != hash || existing.3 != json {
                return Err("adaptation_recovery_registry_conflict".into());
            }
        } else if !evohime_local_storage::local_model_runtime_manager_store::put_record(
            &transaction,
            &record_id,
            kind,
            1,
            &hash,
            &json,
            crate::task_memory::now_millis() as i64,
        )
        .map_err(|_| "adaptation_recovery_registry_write_failed".to_string())?
        {
            return Err("adaptation_recovery_registry_conflict".into());
        }
    }
    if !evohime_local_storage::local_model_adaptation_store::put_publication(
        &transaction,
        &promoted_job.request.job_id,
        &expected_model_id,
        1,
        &expected_relative_path,
        expected_output,
        output_size,
        "registered",
        &publication_hash,
        crate::task_memory::now_millis() as i64,
    )
    .map_err(|_| "adaptation_recovery_publication_write_failed".to_string())?
    {
        return Err("adaptation_recovery_publication_conflict".into());
    }
    if !evohime_local_storage::local_model_adaptation_store::put_job(
        &transaction,
        &promoted_job.request.job_id,
        promoted_job.revision,
        promoted_job.state.storage_key(),
        &promoted_job.request.idempotency_key,
        &current.2,
        &promoted_job.content_sha256,
        &current.4,
        &snapshot,
        crate::task_memory::now_millis() as i64,
    )
    .map_err(|_| "adaptation_recovery_job_write_failed".to_string())?
    {
        return Err("adaptation_recovery_revision_conflict".into());
    }
    transaction
        .commit()
        .map_err(|_| "adaptation_recovery_transaction_failed".to_string())?;
    *job = promoted_job;
    Ok(true)
}

#[cfg(windows)]
async fn stop_supervisor_adaptation(request: serde_json::Value) -> Result<(), String> {
    let response = crate::analysis_kernel::supervisor_command(request)
        .await
        .map_err(|_| "adaptation_recovery_supervisor_unavailable".to_string())?;
    let already_stopped =
        response.get("reason").and_then(serde_json::Value::as_str) == Some("job_not_running");
    if response.get("accepted") != Some(&serde_json::Value::Bool(true)) && !already_stopped {
        return Err("adaptation_recovery_stop_rejected".into());
    }
    Ok(())
}

#[cfg(windows)]
async fn recover_interrupted_job(
    journal: &crate::EventJournal,
    job: &mut AdaptationJob,
    expected_revision: u64,
) -> Result<(), String> {
    let expected_state = job.state.storage_key();
    job.transition(AdaptationState::Interrupted, job.evidence.clone())
        .map_err(|_| "adaptation_recovery_transition_denied".to_string())?;
    let snapshot = serde_json::to_vec(job)
        .map_err(|_| "adaptation_recovery_serialization_failed".to_string())?;
    let database = journal.database().lock().await;
    let current = evohime_local_storage::local_model_adaptation_store::get_job(
        database.connection(),
        &job.request.job_id,
    )
    .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
    .ok_or_else(|| "adaptation_recovery_job_missing".to_string())?;
    if current.0 != expected_revision || current.1 != expected_state {
        return Err("adaptation_recovery_revision_conflict".into());
    }
    if !evohime_local_storage::local_model_adaptation_store::put_job(
        database.connection(),
        &job.request.job_id,
        job.revision,
        job.state.storage_key(),
        &job.request.idempotency_key,
        &current.2,
        &job.content_sha256,
        &current.4,
        &snapshot,
        crate::task_memory::now_millis() as i64,
    )
    .map_err(|_| "adaptation_recovery_storage_failed".to_string())?
    {
        return Err("adaptation_recovery_revision_conflict".into());
    }
    Ok(())
}

#[cfg(windows)]
fn quarantine_partial_staging(
    models_root: &Path,
    job: &AdaptationJob,
    revision: u64,
) -> Result<(), String> {
    use std::fs;
    let source = models_root.join(staging_relative_path(&job.request.job_id));
    let source_metadata = match fs::symlink_metadata(&source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("adaptation_recovery_staging_read_failed".into()),
    };
    let root_metadata = fs::symlink_metadata(models_root)
        .map_err(|_| "adaptation_recovery_models_root_invalid".to_string())?;
    if !root_metadata.is_dir() || unsafe_path_metadata(&root_metadata) {
        return Err("adaptation_recovery_models_root_invalid".into());
    }
    if !source_metadata.is_file() || unsafe_path_metadata(&source_metadata) {
        return Err("adaptation_recovery_staging_invalid".into());
    }
    let staging_parent = source
        .parent()
        .ok_or_else(|| "adaptation_recovery_staging_invalid".to_string())?;
    let parent_metadata = fs::symlink_metadata(staging_parent)
        .map_err(|_| "adaptation_recovery_staging_invalid".to_string())?;
    if !parent_metadata.is_dir() || unsafe_path_metadata(&parent_metadata) {
        return Err("adaptation_recovery_staging_invalid".into());
    }
    let quarantine = models_root.join(".adaptation-quarantine");
    let quarantine_metadata = match fs::symlink_metadata(&quarantine) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&quarantine)
                .map_err(|_| "adaptation_recovery_quarantine_unavailable".to_string())?;
            fs::symlink_metadata(&quarantine)
                .map_err(|_| "adaptation_recovery_quarantine_unavailable".to_string())?
        }
        Err(_) => return Err("adaptation_recovery_quarantine_unavailable".into()),
    };
    if !quarantine_metadata.is_dir() || unsafe_path_metadata(&quarantine_metadata) {
        return Err("adaptation_recovery_quarantine_invalid".into());
    }
    let filename = format!(
        "{}-{revision}.gguf",
        hex::encode(Sha256::digest(job.request.job_id.as_bytes()))
    );
    let destination = quarantine.join(filename);
    match fs::symlink_metadata(&destination) {
        Ok(_) => return Err("adaptation_recovery_quarantine_collision".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("adaptation_recovery_quarantine_unavailable".into()),
    }
    fs::rename(source, destination).map_err(|_| "adaptation_recovery_quarantine_failed".to_string())
}

#[cfg(windows)]
fn unsafe_path_metadata(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, AdaptationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| AdaptationError::InvalidRequest)?;
    let mut hasher = Sha256::new();
    hasher.update(CONTRACT_ID.as_bytes());
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> AdaptationRequest {
        AdaptationRequest {
            job_id: "job-1".into(),
            idempotency_key: "key-1".into(),
            source: SourceModelIdentity {
                model_id: "model-1".into(),
                revision: 1,
                artifact_sha256: "a".repeat(64),
                artifact_size_bytes: 1_024,
                source_quantization: "F16".into(),
            },
            target: QuantizationTarget::Q4Km,
            benchmark_suite_sha256: "b".repeat(64),
            baseline_sha256: "c".repeat(64),
            max_output_bytes: 4096,
            approval_policy_id: "local-promotion".into(),
            approval_policy_sha256: "c".repeat(64),
            policy_revision: 1,
        }
    }

    fn put_gguf_string(bytes: &mut Vec<u8>, value: &[u8]) {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value);
    }

    fn write_gguf(path: &Path, file_type: u32, tensor_types: &[u32], empty_string_value: bool) {
        use std::io::Write;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&(tensor_types.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&(if empty_string_value { 2_u64 } else { 1_u64 }).to_le_bytes());
        put_gguf_string(&mut bytes, b"general.file_type");
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&file_type.to_le_bytes());
        if empty_string_value {
            put_gguf_string(&mut bytes, b"general.name");
            bytes.extend_from_slice(&8_u32.to_le_bytes());
            bytes.extend_from_slice(&0_u64.to_le_bytes());
        }
        for (index, tensor_type) in tensor_types.iter().enumerate() {
            put_gguf_string(&mut bytes, format!("weight-{index}").as_bytes());
            bytes.extend_from_slice(&1_u32.to_le_bytes());
            bytes.extend_from_slice(&1_u64.to_le_bytes());
            bytes.extend_from_slice(&tensor_type.to_le_bytes());
            bytes.extend_from_slice(&0_u64.to_le_bytes());
        }
        bytes.resize(bytes.len().max(64), 0);
        let mut file = File::create(path).unwrap();
        file.write_all(&bytes).unwrap();
    }

    #[test]
    fn job_requires_real_immutable_evidence_before_promotion() {
        let mut job = AdaptationJob::create(request()).unwrap();
        assert_eq!(job.state, AdaptationState::Created);
        assert!(job.adapter.validate().is_ok());
        job.transition(AdaptationState::Preflighted, AdaptationEvidence::default())
            .unwrap();
        job.transition(AdaptationState::Running, AdaptationEvidence::default())
            .unwrap();
        job.transition(AdaptationState::Verifying, AdaptationEvidence::default())
            .unwrap();
        assert_eq!(
            job.transition(AdaptationState::Benchmarking, AdaptationEvidence::default()),
            Ok(())
        );
        assert_eq!(
            job.transition(
                AdaptationState::ReadyForPromotion,
                AdaptationEvidence::default()
            ),
            Err(AdaptationError::InvalidEvidence)
        );
    }

    #[test]
    fn resource_wait_can_resume_without_recreating_the_job() {
        let mut job = AdaptationJob::create(request()).unwrap();
        job.transition(AdaptationState::Preflighted, AdaptationEvidence::default())
            .unwrap();
        job.transition(
            AdaptationState::WaitingForResources,
            AdaptationEvidence::default(),
        )
        .unwrap();
        job.transition(AdaptationState::Running, AdaptationEvidence::default())
            .unwrap();
        assert_eq!(job.state, AdaptationState::Running);
        assert_eq!(job.revision, 4);
        assert!(job.validate().is_ok());
    }

    #[test]
    fn supervisor_capacity_rejection_can_wait_and_retry_durably() {
        let mut job = AdaptationJob::create(request()).unwrap();
        job.transition(AdaptationState::Preflighted, AdaptationEvidence::default())
            .unwrap();
        job.transition(AdaptationState::Running, AdaptationEvidence::default())
            .unwrap();
        job.transition(
            AdaptationState::WaitingForResources,
            AdaptationEvidence::default(),
        )
        .unwrap();
        job.transition(AdaptationState::Running, AdaptationEvidence::default())
            .unwrap();
        assert_eq!(job.state, AdaptationState::Running);
        assert_eq!(job.revision, 5);
        assert!(job.validate().is_ok());
    }

    #[test]
    fn benchmark_cannot_start_before_calibration_evidence() {
        let mut job = AdaptationJob::create(request()).unwrap();
        job.transition(AdaptationState::Preflighted, AdaptationEvidence::default())
            .unwrap();
        job.transition(AdaptationState::Running, AdaptationEvidence::default())
            .unwrap();
        job.transition(AdaptationState::Verifying, AdaptationEvidence::default())
            .unwrap();
        let mut evidence = AdaptationEvidence {
            output_sha256: Some("a".repeat(64)),
            output_size_bytes: Some(128),
            runtime_probe_sha256: Some("b".repeat(64)),
            ..AdaptationEvidence::default()
        };
        job.transition(AdaptationState::Benchmarking, evidence.clone())
            .unwrap();
        evidence.benchmark_started = true;
        assert_eq!(
            job.transition(AdaptationState::Benchmarking, evidence),
            Err(AdaptationError::InvalidEvidence)
        );
    }

    #[test]
    fn cancellation_and_rejection_persist_intent_before_terminal_state() {
        let mut cancelled = AdaptationJob::create(request()).unwrap();
        cancelled
            .transition(AdaptationState::Preflighted, AdaptationEvidence::default())
            .unwrap();
        cancelled
            .transition(AdaptationState::Running, AdaptationEvidence::default())
            .unwrap();
        cancelled
            .transition(AdaptationState::Cancelling, AdaptationEvidence::default())
            .unwrap();
        cancelled
            .transition(AdaptationState::Cancelled, AdaptationEvidence::default())
            .unwrap();
        assert_eq!(cancelled.state, AdaptationState::Cancelled);

        let mut rejected = AdaptationJob::create(request()).unwrap();
        rejected
            .transition(AdaptationState::Preflighted, AdaptationEvidence::default())
            .unwrap();
        rejected
            .transition(AdaptationState::Rejecting, AdaptationEvidence::default())
            .unwrap();
        rejected
            .transition(AdaptationState::Rejected, AdaptationEvidence::default())
            .unwrap();
        assert_eq!(rejected.state, AdaptationState::Rejected);
    }

    #[test]
    fn request_rejects_unverified_source_format_and_adapter_changes() {
        let mut invalid = request();
        invalid.source.source_quantization = "Q8_0".into();
        assert_eq!(invalid.validate(), Err(AdaptationError::InvalidRequest));
        let mut adapter = AdapterIdentity::pinned();
        adapter.archive_sha256 = "0".repeat(64);
        assert_eq!(
            adapter.validate(),
            Err(AdaptationError::AdapterIdentityMismatch)
        );
    }

    #[test]
    fn source_gguf_parser_checks_file_type_tensors_and_empty_metadata_strings() {
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("valid.gguf");
        write_gguf(&valid, 1, &[1, 1, 0], true);
        assert_eq!(verify_gguf_source(&valid, "F16"), Ok(()));
        assert_eq!(
            verify_gguf_source(&valid, "F32"),
            Err(AdaptationError::InvalidSourceModel)
        );

        let quantized = directory.path().join("quantized.gguf");
        write_gguf(&quantized, 1, &[1, 2], false);
        assert_eq!(
            verify_gguf_source(&quantized, "F16"),
            Err(AdaptationError::InvalidSourceModel)
        );

        let mismatched = directory.path().join("mismatched.gguf");
        write_gguf(&mismatched, 0, &[0, 0], false);
        assert_eq!(
            verify_gguf_source(&mismatched, "F16"),
            Err(AdaptationError::InvalidSourceModel)
        );
    }

    #[test]
    fn source_gguf_parser_rejects_truncated_files_and_unknown_versions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad.gguf");
        write_gguf(&path, 0, &[0], false);
        std::fs::write(&path, b"GGUF\x03\x00\x00\x00").unwrap();
        assert_eq!(
            verify_gguf_source(&path, "F32"),
            Err(AdaptationError::InvalidSourceModel)
        );
        std::fs::write(
            &path,
            b"GGUF\x04\x00\x00\x00\x01\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0",
        )
        .unwrap();
        assert_eq!(
            verify_gguf_source(&path, "F32"),
            Err(AdaptationError::InvalidSourceModel)
        );
    }

    #[test]
    fn pinned_adapter_redirects_are_https_and_host_allowlisted() {
        assert!(allowed_asset_redirect(
            &reqwest::Url::parse(LLAMA_CPP_ASSET_URL).unwrap()
        ));
        assert!(allowed_asset_redirect(
            &reqwest::Url::parse("https://release-assets.githubusercontent.com/asset?sig=x")
                .unwrap()
        ));
        assert!(!allowed_asset_redirect(
            &reqwest::Url::parse("http://release-assets.githubusercontent.com/asset").unwrap()
        ));
        assert!(!allowed_asset_redirect(
            &reqwest::Url::parse("https://example.com/asset").unwrap()
        ));
    }

    #[test]
    fn installed_package_rejects_manifest_only_or_modified_runtime_files() {
        let directory = tempfile::tempdir().unwrap();
        let required_files = [
            "llama-quantize.exe",
            "llama-quantize-impl.dll",
            "llama-server.exe",
            "llama-server-impl.dll",
            "llama-common.dll",
            "llama.dll",
            "ggml.dll",
            "ggml-base.dll",
            "ggml-cpu-x64.dll",
            "LICENSE-LLVM-OpenMP",
            "LICENSE-llama.cpp",
        ];
        for name in required_files {
            std::fs::write(directory.path().join(name), b"verified fixture").unwrap();
        }
        let manifest = serde_json::json!({
            "adapter_id": "llama.cpp-cpu",
            "version": LLAMA_CPP_VERSION,
            "archive_sha256": LLAMA_CPP_ARCHIVE_SHA256,
            "archive_size_bytes": LLAMA_CPP_ARCHIVE_SIZE_BYTES
        });
        std::fs::write(
            directory.path().join("evohime-adapter.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(!adapter_install_is_valid(directory.path()));
        let mut altered = manifest;
        altered["archive_sha256"] = serde_json::json!("0".repeat(64));
        std::fs::write(
            directory.path().join("evohime-adapter.json"),
            serde_json::to_vec(&altered).unwrap(),
        )
        .unwrap();
        assert!(!adapter_install_is_valid(directory.path()));
    }

    #[test]
    fn pinned_runtime_hash_allowlist_is_complete_and_well_formed() {
        assert_eq!(LLAMA_CPP_RUNTIME_FILES.len(), 10);
        assert!(LLAMA_CPP_RUNTIME_FILES
            .iter()
            .all(|(_, hash)| valid_hash(hash)));
        assert!(valid_hash(LLAMA_CPP_OPENMP_LICENSE_SHA256));
        let names: std::collections::BTreeSet<_> = LLAMA_CPP_RUNTIME_FILES
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names.len(), LLAMA_CPP_RUNTIME_FILES.len());
    }

    #[test]
    fn installed_file_hash_check_rejects_mismatch_and_non_files() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("runtime.dll");
        let bytes = b"fixed runtime bytes";
        std::fs::write(&file, bytes).unwrap();
        let hash = hex::encode(Sha256::digest(bytes));
        assert!(file_sha256_matches(&file, &hash));
        assert!(!file_sha256_matches(&file, &"0".repeat(64)));
        assert!(!file_sha256_matches(directory.path(), &hash));
    }

    #[tokio::test]
    async fn local_inference_stream_consumes_bounded_sse_and_keeps_only_evidence() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
            }
            let headers = String::from_utf8_lossy(&request);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            let body_start = request.len();
            request.resize(body_start + content_length, 0);
            stream.read_exact(&mut request[body_start..]).await.unwrap();
            let body =
                b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        });
        let evidence = local_inference_stream(
            port,
            "evohime-adaptation-0123456789abcdef",
            "probe",
            8,
            Some("ok"),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(evidence.expected_match, Some(true));
        assert_eq!(evidence.completion_bytes, 2);
        assert!(valid_hash(&evidence.completion_sha256));
        assert_eq!(evidence.adapter_version, "llama-openai-sse-v1");
    }
}
