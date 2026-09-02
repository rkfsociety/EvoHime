//! Core-owned, bounded workspace bootstrap contract.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, time::Duration};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID: usize = 128;
pub const MAX_STEPS: usize = 32;
pub const MAX_ARGS: usize = 32;
pub const MAX_TEXT: usize = 512;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStepKind {
    CheckExecutable,
    CheckVersion,
    RunCommand,
    CopyTemplateIfMissing,
    CreateDirectoryIfMissing,
    GenerateArtifact,
    VerifyFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkRequirement {
    None,
    PackageRegistry,
    GeneralInternet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepIdempotency {
    Idempotent,
    ConditionallyIdempotent,
    NonIdempotent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStatus {
    PendingReview,
    ReadyToBootstrap,
    Running,
    Prepared,
    Stale,
    Failed,
    Blocked,
    UnknownOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapStep {
    pub id: String,
    pub kind: BootstrapStepKind,
    pub logical_executable: Option<String>,
    pub args: Vec<String>,
    pub workspace_relative_path: Option<String>,
    pub network: NetworkRequirement,
    pub idempotency: StepIdempotency,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBootstrapManifest {
    pub schema_version: u32,
    pub id: String,
    pub workspace_id: String,
    pub revision: u64,
    pub steps: Vec<BootstrapStep>,
    pub cache_inputs: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BootstrapManifestError {
    #[error("unsupported workspace bootstrap schema")]
    UnsupportedVersion,
    #[error("invalid workspace bootstrap manifest")]
    Invalid,
    #[error("workspace bootstrap manifest is too large")]
    TooLarge,
    #[error("workspace bootstrap path must be relative")]
    UnsafePath,
    #[error("bootstrap effect is not supported by this runtime")]
    UnsupportedEffect,
    #[error("bootstrap network access is denied")]
    NetworkDenied,
    #[error("bootstrap command failed")]
    CommandFailed,
    #[error("bootstrap command timed out")]
    TimedOut,
}

fn bounded_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT && !value.chars().any(|c| c.is_control())
}

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

pub fn canonical_hash(
    manifest: &WorkspaceBootstrapManifest,
) -> Result<String, BootstrapManifestError> {
    Ok(hex::encode(Sha256::digest(canonical_bytes(manifest)?)))
}

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
