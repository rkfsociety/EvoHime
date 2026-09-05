//! Core-owned metadata contract for managed local model runtimes.
//! Runtime processes and downloads remain outside this pure contract and are
//! admitted only through the existing supervisor/backend boundaries.
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "local-model-runtime-manager-v1";
pub const MAX_ID: usize = 128;
pub const MAX_CONTEXT: u32 = 1_048_576;
pub const MAX_CATALOG: usize = 256;
pub const MAX_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[cfg(windows)]
pub fn discover_hardware() -> Result<LocalHardwareProfile, ManagerError> {
    use std::mem::zeroed;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let cpu_threads = std::thread::available_parallelism()
        .map_err(|_| ManagerError::Invalid("cpu discovery"))?
        .get()
        .min(u16::MAX as usize) as u16;
    let mut memory: MEMORYSTATUSEX = unsafe { zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 || memory.ullTotalPhys == 0 {
        return Err(ManagerError::Invalid("memory discovery"));
    }
    let data_dir = crate::get_data_directory();
    let mut wide: Vec<u16> = data_dir.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available = 0u64;
    let disk_ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0;
    let runtime_candidates = if std::env::var_os("EVOHIME_LOCAL_ADAPTER_EXE").is_some() {
        vec!["openai-compatible-loopback-v1".into()]
    } else {
        Vec::new()
    };
    let fingerprint = canonical_hash(&(
        cpu_threads,
        memory.ullTotalPhys,
        disk_ok.then_some(available),
        &runtime_candidates,
    ));
    let profile = LocalHardwareProfile {
        schema_version: SCHEMA_VERSION,
        revision: 1,
        cpu_threads,
        ram_bytes: memory.ullTotalPhys,
        accelerator_bytes: None,
        disk_free_bytes: available,
        runtime_candidates,
        fingerprint,
    };
    profile.validate()?;
    Ok(profile)
}

#[cfg(not(windows))]
pub fn discover_hardware() -> Result<LocalHardwareProfile, ManagerError> {
    let cpu_threads = std::thread::available_parallelism()
        .map_err(|_| ManagerError::Invalid("cpu discovery"))?
        .get()
        .min(u16::MAX as usize) as u16;
    let profile = LocalHardwareProfile {
        schema_version: SCHEMA_VERSION,
        revision: 1,
        cpu_threads,
        ram_bytes: 1,
        accelerator_bytes: None,
        disk_free_bytes: 1,
        runtime_candidates: Vec::new(),
        fingerprint: canonical_hash(&cpu_threads),
    };
    Ok(profile)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    ManagedVerified,
    UserImported,
    Unverified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitStatus {
    Compatible,
    InsufficientMemory,
    Unknown,
    Unsupported,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactState {
    NotInstalled,
    Queued,
    Downloading,
    Verifying,
    Installed,
    Loading,
    Probing,
    Ready,
    Failed,
    Updating,
    Removing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPolicy {
    Manual,
    PreferWhenReady,
    NewConversationsOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Registered,
    Starting,
    Ready,
    Unavailable,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalHardwareProfile {
    pub schema_version: u32,
    pub revision: u64,
    pub cpu_threads: u16,
    pub ram_bytes: u64,
    pub accelerator_bytes: Option<u64>,
    pub disk_free_bytes: u64,
    pub runtime_candidates: Vec<String>,
    pub fingerprint: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelDescriptor {
    pub model_id: String,
    pub revision: u64,
    pub format: String,
    pub quantization: String,
    pub artifact_size_bytes: u64,
    pub artifact_hash: String,
    pub required_ram_bytes: u64,
    pub required_accelerator_bytes: Option<u64>,
    pub context_limit: u32,
    pub capabilities: BTreeSet<String>,
    pub trust: TrustLevel,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalInferenceRuntime {
    pub runtime_id: String,
    pub revision: u64,
    pub executable_hash: String,
    pub version: String,
    pub protocol: String,
    pub state: RuntimeState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelFit {
    pub model_id: String,
    pub model_revision: u64,
    pub hardware_revision: u64,
    pub status: FitStatus,
    pub estimated_memory_bytes: Option<u64>,
    pub safe_context: Option<u32>,
    pub performance_class: String,
    pub reasons: Vec<String>,
    pub input_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelRuntimeSession {
    pub session_id: String,
    pub model_id: String,
    pub model_revision: u64,
    pub runtime_id: String,
    pub runtime_revision: u64,
    pub artifact_hash: String,
    pub hardware_fingerprint: String,
    pub context_limit: u32,
    pub state: RuntimeState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedModelProfile {
    pub profile_ref: String,
    pub model_id: String,
    pub runtime_id: String,
    pub locality: String,
    pub context_limit: u32,
    pub capabilities: BTreeSet<String>,
    pub descriptor_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelManagerPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub version: u64,
    pub activation: ActivationPolicy,
    pub preferred_model_id: Option<String>,
    pub bootstrap_model_id: Option<String>,
    pub max_loaded_models: u8,
    pub reserved_memory_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationDecision {
    KeepSnapshot,
    Bootstrap,
    Preferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalArtifactRecord {
    pub model_id: String,
    pub model_revision: u64,
    pub expected_hash: String,
    pub expected_size_bytes: u64,
    pub state: ArtifactState,
    pub content_hash: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManagerError {
    #[error("unsupported local model manager schema")]
    UnsupportedSchema,
    #[error("invalid local model manager contract: {0}")]
    Invalid(&'static str),
    #[error("untrusted or unverified local artifact")]
    Untrusted,
    #[error("illegal artifact transition")]
    IllegalTransition,
    #[error("artifact download failed: {0}")]
    Download(&'static str),
}

/// Download a verified artifact into a Core-owned staging file. A shorter
/// existing file is resumed only with an explicit range response; an unknown
/// response is never appended blindly. The caller owns the managed-path check
/// and supplies cancellation from the Core operation budget.
pub async fn download_verified_artifact(
    url: &str,
    staging: &Path,
    expected_hash: &str,
    expected_size: u64,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<(), ManagerError> {
    if !(url.starts_with("https://")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with("http://localhost:"))
        || !valid_hash(expected_hash)
        || expected_size == 0
        || expected_size > MAX_ARTIFACT_BYTES
    {
        return Err(ManagerError::Download("invalid download contract"));
    }
    let existing = fs::metadata(staging)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing > expected_size {
        return Err(ManagerError::Download("staging exceeds expected size"));
    }
    if cancellation.is_cancelled() {
        return Err(ManagerError::Download("cancelled"));
    }
    if let Some(parent) = staging.parent() {
        fs::create_dir_all(parent).map_err(|_| ManagerError::Download("staging directory"))?;
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ManagerError::Download("client"))?;
    let mut request = client.get(url);
    if existing > 0 && existing < expected_size {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|_| ManagerError::Download("transport"))?;
    let status = response.status();
    let resume = existing > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !(status.is_success() && (existing == 0 || resume || status == reqwest::StatusCode::OK)) {
        return Err(ManagerError::Download("range response"));
    }
    let mut file = if resume {
        fs::OpenOptions::new()
            .append(true)
            .open(staging)
            .map_err(|_| ManagerError::Download("staging open"))?
    } else {
        fs::File::create(staging).map_err(|_| ManagerError::Download("staging create"))?
    };
    let mut downloaded = if resume { existing } else { 0 };
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancellation.is_cancelled() {
            return Err(ManagerError::Download("cancelled"));
        }
        let chunk = chunk.map_err(|_| ManagerError::Download("body"))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > expected_size {
            return Err(ManagerError::Download("response exceeds expected size"));
        }
        file.write_all(&chunk)
            .map_err(|_| ManagerError::Download("staging write"))?;
    }
    file.flush()
        .map_err(|_| ManagerError::Download("staging flush"))?;
    if downloaded != expected_size {
        return Err(ManagerError::Download("incomplete artifact"));
    }
    let (observed_hash, observed_size) = file_sha256(staging)?;
    if observed_hash != expected_hash || observed_size != expected_size {
        return Err(ManagerError::Download("artifact verification failed"));
    }
    Ok(())
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID && !value.bytes().any(|b| b.is_ascii_control())
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub fn canonical_hash<T: Serialize>(value: &T) -> String {
    let mut hash = Sha256::new();
    hash.update(CONTRACT_ID.as_bytes());
    hash.update(serde_json::to_vec(value).unwrap_or_default());
    hex::encode(hash.finalize())
}

impl LocalHardwareProfile {
    pub fn validate(&self) -> Result<(), ManagerError> {
        if self.schema_version != SCHEMA_VERSION
            || self.revision == 0
            || self.cpu_threads == 0
            || self.ram_bytes == 0
            || self.disk_free_bytes == 0
            || self.fingerprint.len() != 64
            || self.runtime_candidates.len() > 16
        {
            return Err(ManagerError::Invalid("hardware"));
        }
        Ok(())
    }
}
impl LocalModelDescriptor {
    pub fn validate(&self) -> Result<(), ManagerError> {
        if !valid_id(&self.model_id)
            || self.revision == 0
            || !valid_id(&self.format)
            || !valid_id(&self.quantization)
            || self.artifact_size_bytes == 0
            || !valid_hash(&self.artifact_hash)
            || self.required_ram_bytes == 0
            || self.context_limit == 0
            || self.context_limit > MAX_CONTEXT
            || self.trust != TrustLevel::ManagedVerified
        {
            return Err(ManagerError::Invalid("model descriptor"));
        }
        Ok(())
    }
}
impl LocalInferenceRuntime {
    pub fn validate(&self) -> Result<(), ManagerError> {
        if !valid_id(&self.runtime_id)
            || self.revision == 0
            || !valid_hash(&self.executable_hash)
            || !valid_id(&self.version)
            || self.protocol != "openai-compatible-loopback-v1"
        {
            return Err(ManagerError::Invalid("runtime identity"));
        }
        Ok(())
    }
}

impl LocalModelRuntimeSession {
    pub fn validate(&self) -> Result<(), ManagerError> {
        if !valid_id(&self.session_id)
            || !valid_id(&self.model_id)
            || self.model_revision == 0
            || !valid_id(&self.runtime_id)
            || self.runtime_revision == 0
            || !valid_hash(&self.artifact_hash)
            || !valid_hash(&self.hardware_fingerprint)
            || self.context_limit == 0
            || self.context_limit > MAX_CONTEXT
        {
            return Err(ManagerError::Invalid("runtime session"));
        }
        Ok(())
    }
}

impl LocalArtifactRecord {
    pub fn validate(&self) -> Result<(), ManagerError> {
        if !valid_id(&self.model_id)
            || self.model_revision == 0
            || !valid_hash(&self.expected_hash)
            || self.expected_size_bytes == 0
            || self
                .content_hash
                .as_deref()
                .is_some_and(|hash| !valid_hash(hash))
        {
            return Err(ManagerError::Invalid("artifact record"));
        }
        Ok(())
    }
}

/// Accept only a relative artifact name.  The caller supplies the managed
/// root; a renderer-provided absolute path or traversal component is never a
/// valid artifact location.
pub fn validate_artifact_relative_path(path: &Path) -> Result<(), ManagerError> {
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManagerError::Invalid("artifact path"));
    }
    Ok(())
}

pub fn managed_artifact_path(root: &Path, relative: &Path) -> Result<PathBuf, ManagerError> {
    validate_artifact_relative_path(relative)?;
    let metadata =
        fs::symlink_metadata(root).map_err(|_| ManagerError::Invalid("artifact root"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ManagerError::Invalid("artifact root"));
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(ManagerError::Invalid("artifact path"));
        };
        current.push(part);
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err(ManagerError::Invalid("artifact path"));
            }
        }
    }
    Ok(root.join(relative))
}

fn file_sha256(path: &Path) -> Result<(String, u64), ManagerError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| ManagerError::Invalid("artifact file"))?;
    if !metadata.file_type().is_file() {
        return Err(ManagerError::Invalid("artifact file"));
    }
    let mut file = fs::File::open(path).map_err(|_| ManagerError::Invalid("artifact file"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| ManagerError::Invalid("artifact file"))?;
        if read == 0 {
            break;
        }
        size = size.saturating_add(read as u64);
        hash.update(&buffer[..read]);
    }
    Ok((hex::encode(hash.finalize()), size))
}

/// Verify a staging file and atomically promote it into the Core-owned root.
/// Existing destinations are rejected so an immutable revision cannot be
/// silently replaced by an unreviewed download.
pub fn atomic_promote_verified_artifact(
    staging: &Path,
    destination: &Path,
    expected_hash: &str,
    expected_size: u64,
) -> Result<(), ManagerError> {
    if !valid_hash(expected_hash) || expected_size == 0 || staging == destination {
        return Err(ManagerError::Invalid("artifact promotion input"));
    }
    if destination.exists() {
        return Err(ManagerError::Invalid("artifact already installed"));
    }
    if staging.parent().is_none() || destination.parent().is_none() {
        return Err(ManagerError::Invalid("artifact path"));
    }
    let (observed_hash, observed_size) = file_sha256(staging)?;
    if observed_hash != expected_hash || observed_size != expected_size {
        return Err(ManagerError::Invalid("artifact verification failed"));
    }
    fs::create_dir_all(destination.parent().expect("checked above"))
        .map_err(|_| ManagerError::Invalid("artifact destination"))?;
    fs::rename(staging, destination).map_err(|_| ManagerError::Invalid("artifact promotion failed"))
}
impl LocalModelManagerPolicy {
    pub fn validate(&self) -> Result<(), ManagerError> {
        if self.schema_version != SCHEMA_VERSION
            || self.policy_id != CONTRACT_ID
            || self.version == 0
            || self.max_loaded_models == 0
            || self.max_loaded_models > 8
        {
            return Err(ManagerError::Invalid("policy"));
        }
        Ok(())
    }
}

pub fn compute_fit(
    hardware: &LocalHardwareProfile,
    model: &LocalModelDescriptor,
) -> Result<LocalModelFit, ManagerError> {
    hardware.validate()?;
    model.validate()?;
    let input_hash = canonical_hash(&(hardware, model));
    if hardware.runtime_candidates.is_empty() {
        return Ok(LocalModelFit {
            model_id: model.model_id.clone(),
            model_revision: model.revision,
            hardware_revision: hardware.revision,
            status: FitStatus::Unknown,
            estimated_memory_bytes: None,
            safe_context: None,
            performance_class: "unknown".into(),
            reasons: vec!["runtime_capability_unknown".into()],
            input_hash,
        });
    }
    if model.format != "gguf" {
        return Ok(LocalModelFit {
            model_id: model.model_id.clone(),
            model_revision: model.revision,
            hardware_revision: hardware.revision,
            status: FitStatus::Unsupported,
            estimated_memory_bytes: None,
            safe_context: None,
            performance_class: "unsupported".into(),
            reasons: vec!["format_not_allowlisted".into()],
            input_hash,
        });
    }
    let available = hardware
        .ram_bytes
        .saturating_add(hardware.accelerator_bytes.unwrap_or(0));
    let required = model
        .required_ram_bytes
        .saturating_add(model.required_accelerator_bytes.unwrap_or(0));
    let status = if available < required {
        FitStatus::InsufficientMemory
    } else {
        FitStatus::Compatible
    };
    let safe_context = (model.context_limit / 2).clamp(1, MAX_CONTEXT);
    Ok(LocalModelFit {
        model_id: model.model_id.clone(),
        model_revision: model.revision,
        hardware_revision: hardware.revision,
        status,
        estimated_memory_bytes: Some(required),
        safe_context: (status == FitStatus::Compatible).then_some(safe_context),
        performance_class: if status == FitStatus::Compatible {
            "conservative"
        } else {
            "insufficient"
        }
        .into(),
        reasons: if status == FitStatus::Compatible {
            vec!["conservative_headroom_applied".into()]
        } else {
            vec!["insufficient_memory".into()]
        },
        input_hash,
    })
}

pub struct ChooseActivationInput<'a> {
    pub policy: ActivationPolicy,
    pub current_model_id: &'a str,
    pub bootstrap_model_id: Option<&'a str>,
    pub preferred_model_id: Option<&'a str>,
    pub bootstrap_ready: bool,
    pub preferred_ready: bool,
    pub call_in_flight: bool,
    pub new_conversation: bool,
}

pub fn choose_activation(
    input: ChooseActivationInput<'_>,
) -> Result<ActivationDecision, ManagerError> {
    if input.current_model_id.is_empty() || input.call_in_flight {
        return Ok(ActivationDecision::KeepSnapshot);
    }
    let preferred = input.preferred_model_id.filter(|_| input.preferred_ready);
    match input.policy {
        ActivationPolicy::Manual => Ok(ActivationDecision::KeepSnapshot),
        ActivationPolicy::PreferWhenReady if preferred.is_some() => {
            Ok(ActivationDecision::Preferred)
        }
        ActivationPolicy::NewConversationsOnly if input.new_conversation && preferred.is_some() => {
            Ok(ActivationDecision::Preferred)
        }
        _ if input.bootstrap_ready && input.bootstrap_model_id.is_some() => {
            Ok(ActivationDecision::Bootstrap)
        }
        _ => Ok(ActivationDecision::KeepSnapshot),
    }
}

pub fn resource_admission(
    policy: &LocalModelManagerPolicy,
    loaded_models: u8,
    reserved_memory_bytes: u64,
    requested_memory_bytes: u64,
) -> Result<(), ManagerError> {
    policy.validate()?;
    if loaded_models >= policy.max_loaded_models
        || reserved_memory_bytes.saturating_add(requested_memory_bytes)
            > policy.reserved_memory_bytes
    {
        return Err(ManagerError::Invalid("resource budget"));
    }
    Ok(())
}

pub fn eviction_allowed(in_flight: bool) -> bool {
    !in_flight
}

pub fn allow_artifact_promotion(
    state: ArtifactState,
    trust: TrustLevel,
    observed_hash: &str,
    expected_hash: &str,
) -> Result<(), ManagerError> {
    if state != ArtifactState::Verifying
        || trust != TrustLevel::ManagedVerified
        || observed_hash != expected_hash
        || !valid_hash(expected_hash)
    {
        return Err(if trust != TrustLevel::ManagedVerified {
            ManagerError::Untrusted
        } else {
            ManagerError::Invalid("artifact promotion precondition")
        });
    }
    Ok(())
}
pub fn allow_transition(from: ArtifactState, to: ArtifactState) -> Result<(), ManagerError> {
    let allowed = matches!(
        (from, to),
        (ArtifactState::NotInstalled, ArtifactState::Queued)
            | (ArtifactState::Queued, ArtifactState::Downloading)
            | (ArtifactState::Downloading, ArtifactState::Verifying)
            | (ArtifactState::Verifying, ArtifactState::Installed)
            | (ArtifactState::Installed, ArtifactState::Loading)
            | (ArtifactState::Loading, ArtifactState::Probing)
            | (ArtifactState::Probing, ArtifactState::Ready)
            | (_, ArtifactState::Failed)
            | (ArtifactState::Ready, ArtifactState::Updating)
            | (ArtifactState::Installed, ArtifactState::Removing)
    );
    if allowed {
        Ok(())
    } else {
        Err(ManagerError::IllegalTransition)
    }
}
pub fn managed_profile(
    session: &LocalModelRuntimeSession,
    descriptor: &LocalModelDescriptor,
    runtime: &LocalInferenceRuntime,
) -> Result<ManagedModelProfile, ManagerError> {
    if session.state != RuntimeState::Ready
        || runtime.state != RuntimeState::Ready
        || session.model_id != descriptor.model_id
        || session.runtime_id != runtime.runtime_id
        || session.artifact_hash != descriptor.artifact_hash
    {
        return Err(ManagerError::Invalid("health gate not complete"));
    }
    descriptor.validate()?;
    runtime.validate()?;
    Ok(ManagedModelProfile {
        profile_ref: format!(
            "local-managed:{}:{}",
            descriptor.model_id, descriptor.revision
        ),
        model_id: descriptor.model_id.clone(),
        runtime_id: runtime.runtime_id.clone(),
        locality: "local".into(),
        context_limit: session.context_limit.min(descriptor.context_limit),
        capabilities: descriptor.capabilities.clone(),
        descriptor_hash: canonical_hash(descriptor),
    })
}

/// Adapt the manager-owned profile to the existing resilience registry shape;
/// this is a projection adapter, not a second model authority.
pub fn resilience_profile_ref(
    profile: &ManagedModelProfile,
) -> crate::model_resilience_policy::ModelProfileRef {
    crate::model_resilience_policy::ModelProfileRef {
        id: profile.profile_ref.clone(),
        provider: "local-managed".into(),
        model: profile.model_id.clone(),
        capabilities: profile.capabilities.clone(),
        privacy_boundary: evohime_model_gateway::provider_contract::PrivacyClass::Restricted,
        residency: crate::model_resilience_policy::DataResidency::Local,
        profile_hash: profile.descriptor_hash.clone(),
    }
}

/// Build the existing gateway's local route from a manager-approved profile.
/// The capability is supplied per session and is never persisted or returned
/// in manager projections.
pub fn local_gateway_route(
    profile: &ManagedModelProfile,
    session_capability: &str,
    endpoint: &str,
) -> Result<evohime_model_gateway::ModelRouteConfig, ManagerError> {
    if session_capability.is_empty() || !endpoint.starts_with("http://127.0.0.1:") {
        return Err(ManagerError::Invalid("local route boundary"));
    }
    Ok(evohime_model_gateway::ModelRouteConfig::local(
        session_capability,
        endpoint,
        profile.model_id.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hw() -> LocalHardwareProfile {
        LocalHardwareProfile {
            schema_version: 1,
            revision: 1,
            cpu_threads: 8,
            ram_bytes: 16 << 30,
            accelerator_bytes: Some(8 << 30),
            disk_free_bytes: 100 << 30,
            runtime_candidates: vec!["local".into()],
            fingerprint: "a".repeat(64),
        }
    }
    fn model(trust: TrustLevel) -> LocalModelDescriptor {
        LocalModelDescriptor {
            model_id: "m".into(),
            revision: 1,
            format: "gguf".into(),
            quantization: "q4".into(),
            artifact_size_bytes: 1,
            artifact_hash: "b".repeat(64),
            required_ram_bytes: 2 << 30,
            required_accelerator_bytes: None,
            context_limit: 8192,
            capabilities: ["chat".into()].into_iter().collect(),
            trust,
        }
    }
    #[test]
    fn fit_is_conservative_and_hashed() {
        let fit = compute_fit(&hw(), &model(TrustLevel::ManagedVerified)).unwrap();
        assert_eq!(fit.status, FitStatus::Compatible);
        assert!(fit.safe_context.unwrap() < 8192);
        assert_eq!(fit.input_hash.len(), 64);
        assert_eq!(fit.performance_class, "conservative");
    }
    #[test]
    fn unverified_cannot_promote() {
        assert_eq!(
            allow_artifact_promotion(
                ArtifactState::Verifying,
                TrustLevel::Unverified,
                &"a".repeat(64),
                &"a".repeat(64)
            ),
            Err(ManagerError::Untrusted)
        );
    }
    #[test]
    fn staging_cannot_be_ready() {
        assert_eq!(
            allow_transition(ArtifactState::Downloading, ArtifactState::Ready),
            Err(ManagerError::IllegalTransition)
        );
    }

    #[test]
    fn paths_and_hashes_are_fail_closed() {
        assert!(validate_artifact_relative_path(Path::new("models/m.gguf")).is_ok());
        assert!(validate_artifact_relative_path(Path::new("../m.gguf")).is_err());
        assert!(validate_artifact_relative_path(Path::new("C:\\m.gguf")).is_err());
        assert!(LocalModelDescriptor {
            artifact_hash: "z".repeat(64),
            ..model(TrustLevel::ManagedVerified)
        }
        .validate()
        .is_err());
    }

    #[test]
    fn managed_profile_requires_both_runtime_and_session_ready() {
        let descriptor = model(TrustLevel::ManagedVerified);
        let runtime = LocalInferenceRuntime {
            runtime_id: "runtime".into(),
            revision: 1,
            executable_hash: "c".repeat(64),
            version: "1".into(),
            protocol: "openai-compatible-loopback-v1".into(),
            state: RuntimeState::Ready,
        };
        let session = LocalModelRuntimeSession {
            session_id: "s".into(),
            model_id: descriptor.model_id.clone(),
            model_revision: descriptor.revision,
            runtime_id: runtime.runtime_id.clone(),
            runtime_revision: runtime.revision,
            artifact_hash: descriptor.artifact_hash.clone(),
            hardware_fingerprint: "d".repeat(64),
            context_limit: 4096,
            state: RuntimeState::Ready,
        };
        assert_eq!(
            managed_profile(&session, &descriptor, &runtime)
                .unwrap()
                .locality,
            "local"
        );
        let stopped = LocalModelRuntimeSession {
            state: RuntimeState::Stopped,
            ..session
        };
        assert!(managed_profile(&stopped, &descriptor, &runtime).is_err());
    }

    #[test]
    fn approved_profile_maps_to_existing_local_gateway_without_persisting_capability() {
        let descriptor = model(TrustLevel::ManagedVerified);
        let runtime = LocalInferenceRuntime {
            runtime_id: "r".into(),
            revision: 1,
            executable_hash: "c".repeat(64),
            version: "1".into(),
            protocol: "openai-compatible-loopback-v1".into(),
            state: RuntimeState::Ready,
        };
        let session = LocalModelRuntimeSession {
            session_id: "s".into(),
            model_id: "m".into(),
            model_revision: 1,
            runtime_id: "r".into(),
            runtime_revision: 1,
            artifact_hash: descriptor.artifact_hash.clone(),
            hardware_fingerprint: "d".repeat(64),
            context_limit: 4096,
            state: RuntimeState::Ready,
        };
        let profile = managed_profile(&session, &descriptor, &runtime).unwrap();
        let route = local_gateway_route(&profile, "ephemeral-capability", "http://127.0.0.1:49152")
            .unwrap();
        assert_eq!(route.literouter.model, "m");
        assert_eq!(resilience_profile_ref(&profile).provider, "local-managed");
    }

    #[test]
    fn verified_staging_is_promoted_and_wrong_hash_is_not() {
        let root = std::env::temp_dir().join(format!("evohime-local-model-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let staging = root.join("staging.part");
        let destination = root.join("model.gguf");
        let contents = b"verified artifact";
        fs::write(&staging, contents).unwrap();
        let expected = hex::encode(Sha256::digest(contents));
        assert!(atomic_promote_verified_artifact(
            &staging,
            &destination,
            &expected,
            contents.len() as u64
        )
        .is_ok());
        assert_eq!(fs::read(&destination).unwrap(), contents);
        fs::write(&staging, contents).unwrap();
        assert!(atomic_promote_verified_artifact(
            &staging,
            &root.join("second.gguf"),
            &"a".repeat(64),
            contents.len() as u64
        )
        .is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn incomplete_fit_and_activation_are_conservative() {
        let mut incomplete = hw();
        incomplete.runtime_candidates.clear();
        assert_eq!(
            compute_fit(&incomplete, &model(TrustLevel::ManagedVerified))
                .unwrap()
                .status,
            FitStatus::Unknown
        );
        assert_eq!(
            choose_activation(ChooseActivationInput {
                policy: ActivationPolicy::PreferWhenReady,
                current_model_id: "bootstrap",
                bootstrap_model_id: Some("bootstrap"),
                preferred_model_id: Some("preferred"),
                bootstrap_ready: true,
                preferred_ready: true,
                call_in_flight: true,
                new_conversation: false,
            })
            .unwrap(),
            ActivationDecision::KeepSnapshot
        );
        assert_eq!(
            choose_activation(ChooseActivationInput {
                policy: ActivationPolicy::NewConversationsOnly,
                current_model_id: "bootstrap",
                bootstrap_model_id: Some("bootstrap"),
                preferred_model_id: Some("preferred"),
                bootstrap_ready: true,
                preferred_ready: true,
                call_in_flight: false,
                new_conversation: true,
            })
            .unwrap(),
            ActivationDecision::Preferred
        );
    }
}
