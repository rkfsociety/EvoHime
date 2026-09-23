//! Core-owned, bounded workspace bootstrap contract.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, time::Duration};

/// Current serialized version for workspace bootstrap manifests.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum length of manifest and workspace identifiers.
pub const MAX_ID: usize = 128;
/// Maximum number of steps or cache inputs in a manifest.
pub const MAX_STEPS: usize = 32;
/// Maximum command arguments accepted by one bootstrap step.
pub const MAX_ARGS: usize = 32;
/// Maximum length of a text field or workspace-relative path.
pub const MAX_TEXT: usize = 512;
/// Maximum serialized manifest size in bytes.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// Supported operation in a declarative workspace bootstrap plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStepKind {
    /// Check that a named executable can be resolved by policy.
    CheckExecutable,
    /// Run a bounded command to inspect an installed tool version.
    CheckVersion,
    /// Run an explicitly permitted direct process command.
    RunCommand,
    /// Copy a template only when its destination does not exist.
    CopyTemplateIfMissing,
    /// Create a workspace directory only when it does not exist.
    CreateDirectoryIfMissing,
    /// Generate an artifact from declared inputs.
    GenerateArtifact,
    /// Verify an existing workspace file.
    VerifyFile,
}

/// Network access class requested by a bootstrap step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkRequirement {
    /// Step must run without network access.
    None,
    /// Step requires access to a package registry.
    PackageRegistry,
    /// Step requires general internet access.
    GeneralInternet,
}

/// Retry safety classification for a bootstrap step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepIdempotency {
    /// Repeating the step has no additional effect.
    Idempotent,
    /// Repetition is safe only under the step's stated preconditions.
    ConditionallyIdempotent,
    /// Repetition may produce additional or irreversible effects.
    NonIdempotent,
    /// Retry safety is not known.
    Unknown,
}

/// Overall review or execution state for a bootstrap manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStatus {
    /// Manifest is awaiting human inspection.
    PendingReview,
    /// Manifest has been approved for bounded bootstrap execution.
    ReadyToBootstrap,
    /// One or more declared steps are running.
    Running,
    /// Bootstrap completed and prepared the workspace.
    Prepared,
    /// Manifest no longer matches the workspace state.
    Stale,
    /// A bootstrap step failed.
    Failed,
    /// A policy or capability constraint prevents execution.
    Blocked,
    /// Process outcome cannot be determined safely.
    UnknownOutcome,
}

/// One bounded operation in a workspace bootstrap manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapStep {
    /// Stable step identifier.
    pub id: String,
    /// Operation performed by this step.
    pub kind: BootstrapStepKind,
    /// Logical executable name for supported process operations.
    pub logical_executable: Option<String>,
    /// Bounded argument vector passed to the executable.
    pub args: Vec<String>,
    /// Optional target path relative to the workspace root.
    pub workspace_relative_path: Option<String>,
    /// Network requirement; the current direct-process executor denies non-none values.
    pub network: NetworkRequirement,
    /// Declared retry safety of this operation.
    pub idempotency: StepIdempotency,
    /// Maximum execution duration in milliseconds.
    pub timeout_ms: u64,
}

/// Integrity-bound sequence of workspace setup and verification steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBootstrapManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Stable manifest identifier.
    pub id: String,
    /// Workspace this manifest applies to.
    pub workspace_id: String,
    /// Monotonic manifest revision.
    pub revision: u64,
    /// Ordered bootstrap operations.
    pub steps: Vec<BootstrapStep>,
    /// Workspace-relative files used to identify cached preparation results.
    pub cache_inputs: Vec<String>,
    /// SHA-256 digest with this field cleared during hashing.
    pub content_hash: String,
}

/// Invalid manifest, unsafe path, denied effect, or bounded process failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BootstrapManifestError {
    /// Manifest uses an unsupported schema version.
    #[error("unsupported workspace bootstrap schema")]
    UnsupportedVersion,
    /// Manifest identity, step data, or integrity hash is invalid.
    #[error("invalid workspace bootstrap manifest")]
    Invalid,
    /// Serialized manifest exceeds its size bound.
    #[error("workspace bootstrap manifest is too large")]
    TooLarge,
    /// Workspace-relative path is absolute, empty, or traverses upward.
    #[error("workspace bootstrap path must be relative")]
    UnsafePath,
    /// The current executor does not support the requested operation.
    #[error("bootstrap effect is not supported by this runtime")]
    UnsupportedEffect,
    /// Step requested network access that the executor does not permit.
    #[error("bootstrap network access is denied")]
    NetworkDenied,
    /// Executable could not start or exited unsuccessfully.
    #[error("bootstrap command failed")]
    CommandFailed,
    /// Command exceeded its declared time limit.
    #[error("bootstrap command timed out")]
    TimedOut,
}

fn bounded_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT && !value.chars().any(|c| c.is_control())
}

/// Validates schema, identities, step limits, paths, and canonical hash.
pub fn validate_manifest(
    manifest: &WorkspaceBootstrapManifest,
) -> Result<(), BootstrapManifestError> {
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(BootstrapManifestError::UnsupportedVersion);
    }
    if !bounded_text(&manifest.id)
        || manifest.id.len() > MAX_ID
        || !bounded_text(&manifest.workspace_id)
        || manifest.revision == 0
        || manifest.steps.is_empty()
        || manifest.steps.len() > MAX_STEPS
        || manifest.content_hash.len() != 64
        || manifest.cache_inputs.len() > MAX_STEPS
    {
        return Err(BootstrapManifestError::Invalid);
    }
    for input in &manifest.cache_inputs {
        validate_relative_path(input)?;
    }
    for step in &manifest.steps {
        if !bounded_text(&step.id)
            || step.args.len() > MAX_ARGS
            || step.timeout_ms == 0
            || step.timeout_ms > 30 * 60 * 1000
            || step
                .logical_executable
                .as_deref()
                .is_some_and(|v| !bounded_text(v))
        {
            return Err(BootstrapManifestError::Invalid);
        }
        if let Some(path) = &step.workspace_relative_path {
            validate_relative_path(path)?;
        }
        if matches!(step.kind, BootstrapStepKind::RunCommand) && step.logical_executable.is_none() {
            return Err(BootstrapManifestError::Invalid);
        }
    }
    let encoded = canonical_bytes(manifest)?;
    if encoded.len() > MAX_MANIFEST_BYTES {
        return Err(BootstrapManifestError::TooLarge);
    }
    if canonical_hash(manifest)? != manifest.content_hash {
        return Err(BootstrapManifestError::Invalid);
    }
    Ok(())
}

/// Rejects paths that are not bounded workspace-relative paths.
pub fn validate_relative_path(path: &str) -> Result<(), BootstrapManifestError> {
    if path.is_empty()
        || path.len() > MAX_TEXT
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains(':')
        || path
            .split(['/', '\\'])
            .any(|part| part == ".." || part.is_empty())
    {
        return Err(BootstrapManifestError::UnsafePath);
    }
    Ok(())
}

fn canonical_bytes(
    manifest: &WorkspaceBootstrapManifest,
) -> Result<Vec<u8>, BootstrapManifestError> {
    let mut without_hash = manifest.clone();
    without_hash.content_hash.clear();
    serde_json::to_vec(&without_hash).map_err(|_| BootstrapManifestError::Invalid)
}

/// Computes the SHA-256 digest of a manifest with its hash field cleared.
pub fn canonical_hash(
    manifest: &WorkspaceBootstrapManifest,
) -> Result<String, BootstrapManifestError> {
    Ok(hex::encode(Sha256::digest(canonical_bytes(manifest)?)))
}

/// Fills the manifest content hash after serializing the unhashed record.
pub fn with_content_hash(
    mut manifest: WorkspaceBootstrapManifest,
) -> Result<WorkspaceBootstrapManifest, BootstrapManifestError> {
    manifest.content_hash = canonical_hash(&manifest)?;
    Ok(manifest)
}

/// Executes the explicitly supported direct-process subset. Output is reduced
/// to status metadata and never becomes durable state.
pub async fn run_bounded(
    workspace_root: &Path,
    manifest: &WorkspaceBootstrapManifest,
) -> Result<Vec<serde_json::Value>, BootstrapManifestError> {
    validate_manifest(manifest)?;
    let profile = evohime_tool_runtime::execution_policy_profiles::ExecutionPolicyProfile::resolve(
        "process.run",
    )
    .map_err(|_| BootstrapManifestError::UnsupportedEffect)?;
    let mut results = Vec::with_capacity(manifest.steps.len());
    for step in &manifest.steps {
        if step.network != NetworkRequirement::None {
            return Err(BootstrapManifestError::NetworkDenied);
        }
        if !matches!(
            step.kind,
            BootstrapStepKind::CheckExecutable
                | BootstrapStepKind::CheckVersion
                | BootstrapStepKind::RunCommand
        ) {
            return Err(BootstrapManifestError::UnsupportedEffect);
        }
        let executable = step
            .logical_executable
            .as_deref()
            .ok_or(BootstrapManifestError::Invalid)?;
        evohime_tool_runtime::execution_policy_profiles::validate_program_name(executable)
            .map_err(|_| BootstrapManifestError::UnsupportedEffect)?;
        let mut command = tokio::process::Command::new(executable);
        command
            .args(&step.args)
            .current_dir(workspace_root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        evohime_tool_runtime::execution_policy_profiles::apply_environment(&mut command);
        let mut child = command
            .spawn()
            .map_err(|_| BootstrapManifestError::CommandFailed)?;
        let _guard =
            evohime_tool_runtime::execution_policy_profiles::ProcessGuard::attach(&child, &profile)
                .map_err(|_| BootstrapManifestError::UnsupportedEffect)?;
        let status = tokio::time::timeout(Duration::from_millis(step.timeout_ms), child.wait())
            .await
            .map_err(|_| BootstrapManifestError::TimedOut)?
            .map_err(|_| BootstrapManifestError::CommandFailed)?;
        if !status.success() {
            return Err(BootstrapManifestError::CommandFailed);
        }
        results.push(serde_json::json!({
            "step_id": step.id,
            "status": "completed",
            "exit_code": status.code(),
            "policy_hash": profile.profile_hash,
        }));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> WorkspaceBootstrapManifest {
        with_content_hash(WorkspaceBootstrapManifest {
            schema_version: 1,
            id: "bootstrap".into(),
            workspace_id: "workspace".into(),
            revision: 1,
            steps: vec![BootstrapStep {
                id: "check-cargo".into(),
                kind: BootstrapStepKind::CheckExecutable,
                logical_executable: Some("cargo".into()),
                args: vec![],
                workspace_relative_path: Some("Cargo.lock".into()),
                network: NetworkRequirement::None,
                idempotency: StepIdempotency::Idempotent,
                timeout_ms: 1_000,
            }],
            cache_inputs: vec!["Cargo.lock".into()],
            content_hash: String::new(),
        })
        .unwrap()
    }
    #[test]
    fn valid_manifest_is_hashable_and_bounded() {
        assert!(validate_manifest(&manifest()).is_ok());
    }
    #[test]
    fn changed_manifest_hash_is_rejected() {
        let mut value = manifest();
        value.steps[0].id = "changed".into();
        assert_eq!(
            validate_manifest(&value),
            Err(BootstrapManifestError::Invalid)
        );
    }
    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        assert_eq!(
            validate_relative_path("../Cargo.lock"),
            Err(BootstrapManifestError::UnsafePath)
        );
        assert_eq!(
            validate_relative_path("C:\\secret"),
            Err(BootstrapManifestError::UnsafePath)
        );
        assert_eq!(
            validate_relative_path("/etc/passwd"),
            Err(BootstrapManifestError::UnsafePath)
        );
    }
    #[test]
    fn unknown_run_command_without_executable_is_rejected() {
        let mut value = manifest();
        value.steps[0].kind = BootstrapStepKind::RunCommand;
        value.steps[0].logical_executable = None;
        let value = with_content_hash(value).unwrap();
        assert_eq!(
            validate_manifest(&value),
            Err(BootstrapManifestError::Invalid)
        );
    }

    #[tokio::test]
    async fn direct_check_uses_bounded_process_policy() {
        let root = tempfile::tempdir().unwrap();
        let mut value = manifest();
        value.steps[0].logical_executable = Some("git".into());
        value.steps[0].args = vec!["--version".into()];
        let value = with_content_hash(value).unwrap();
        let result = run_bounded(root.path(), &value).await.unwrap();
        assert_eq!(result[0]["status"], "completed");
        assert_eq!(result[0]["policy_hash"].as_str().unwrap().len(), 64);
    }

    #[tokio::test]
    async fn mutating_step_is_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let mut value = manifest();
        value.steps[0].kind = BootstrapStepKind::CopyTemplateIfMissing;
        let value = with_content_hash(value).unwrap();
        assert_eq!(
            run_bounded(root.path(), &value).await,
            Err(BootstrapManifestError::UnsupportedEffect)
        );
    }
}
