//! Core-owned Git change-set contract and bounded workspace operations.
//!
//! The model may propose paths and a message, but Core captures the Git
//! baseline, classifies the observed state, validates the workspace binding,
//! and performs every effect only after a fresh preflight. The renderer sees
//! only the redacted projections produced by the coordinator.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    time::{timeout, Duration},
};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_PATHS: usize = 256;
pub const MAX_CANDIDATE_PATHS: usize = 128;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_MESSAGE_BYTES: usize = 4096;
pub const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
pub const MAX_DIFF_SUMMARY_BYTES: usize = 512 * 1024;
pub const MAX_WORKSPACE_ROOT_BYTES: usize = 32 * 1024;
pub const MAX_REFERENCE_BYTES: usize = 256;
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathAttribution {
    AgentAuthored,
    PreExistingUser,
    ExternalConcurrent,
    GeneratedByApprovedTool,
    Secret,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetStatus {
    Observed,
    CandidateReady,
    Stale,
    Committed,
    Kept,
    UndoPending,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingCommitReconciliation {
    NoEffect,
    Committed(String),
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDirtyBaseline {
    pub head_commit: Option<String>,
    pub tracked_modified: Vec<String>,
    pub staged: Vec<String>,
    pub untracked: Vec<String>,
    pub relevant_hashes: Vec<String>,
    pub captured_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributedPath {
    pub path: String,
    pub attribution: PathAttribution,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGitChangeSet {
    pub version: u32,
    #[serde(default = "default_revision")]
    pub revision: u64,
    pub id: String,
    pub workspace_binding_id: String,
    pub run_id: String,
    pub task_id: Option<String>,
    pub base_git_head: Option<String>,
    pub base_dirty_fingerprint: String,
    pub workspace_change_set_ref: String,
    pub paths: Vec<AttributedPath>,
    pub status: ChangeSetStatus,
    pub created_at_ms: i64,
    pub content_hash: String,
    #[serde(default)]
    pub baseline: GitDirtyBaseline,
    #[serde(default)]
    pub workspace_root: String,
    #[serde(default)]
    pub incremental_change_run_id: Option<String>,
    #[serde(default)]
    pub task_worktree_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitCandidate {
    pub version: u32,
    #[serde(default = "default_revision")]
    pub revision: u64,
    pub id: String,
    pub change_set_ref: String,
    pub parent_head: Option<String>,
    pub included_paths: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub diff_hash: String,
    pub proposed_message: String,
    pub message_source: String,
    pub verification_status: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub included_hashes: Vec<String>,
    #[serde(default)]
    pub precondition_fingerprint: String,
    #[serde(default)]
    pub workspace_root: String,
    #[serde(default)]
    pub commit_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ObserveRequest {
    #[serde(default)]
    pub workspace_binding_id: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub workspace_change_set_ref: String,
    #[serde(default)]
    pub incremental_change_run_id: Option<String>,
    #[serde(default)]
    pub task_worktree_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CandidateRequest {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub approved_paths: Vec<String>,
    #[serde(default)]
    pub generated_paths: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChangeSetError {
    #[error("unsupported contract version")]
    UnsupportedVersion,
    #[error("invalid or unsafe path")]
    InvalidPath,
    #[error("invalid workspace")]
    InvalidWorkspace,
    #[error("workspace binding does not match the Core-derived identity")]
    WorkspaceBindingMismatch,
    #[error("bounded limit exceeded: {0}")]
    LimitExceeded(&'static str),
    #[error("candidate is stale")]
    Stale,
    #[error("ambiguous attribution requires explicit review")]
    Ambiguous,
    #[error("external concurrent change requires explicit review")]
    ExternalConcurrent,
    #[error("candidate has unrelated staged changes")]
    SharedIndex,
    #[error("no approved changes are present")]
    NoChanges,
    #[error("Git command failed")]
    GitCommandFailed,
    #[error("Git command timed out")]
    GitCommandTimedOut,
    #[error("commit outcome is unknown and requires reconciliation")]
    CommitOutcomeUnknown,
    #[error("undo cannot safely remove a non-file path")]
    UnsafeUndo,
    #[error("referenced incremental change run is invalid")]
    InvalidIncrementalReference,
    #[error("referenced task worktree is invalid or stale")]
    InvalidWorktreeReference,
    #[error("agent Git change-set payload is invalid")]
    InvalidPayload,
}

#[derive(Debug, Clone)]
struct PathState {
    hash: String,
    staged: bool,
    modified: bool,
    untracked: bool,
}

#[derive(Debug, Clone)]
struct GitSnapshot {
    root: PathBuf,
    head: Option<String>,
    baseline: GitDirtyBaseline,
    paths: BTreeMap<String, PathState>,
}

fn default_revision() -> u64 {
    1
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn validate_baseline(baseline: &GitDirtyBaseline) -> Result<(), ChangeSetError> {
    if let Some(head) = &baseline.head_commit {
        validate_commit_id(head)?;
    }
    for paths in [
        &baseline.tracked_modified,
        &baseline.staged,
        &baseline.untracked,
    ] {
        if paths.len() > MAX_PATHS {
            return Err(ChangeSetError::LimitExceeded("paths"));
        }
        for path in paths {
            validate_path(path)?;
        }
    }
    if baseline.relevant_hashes.len() > MAX_PATHS {
        return Err(ChangeSetError::LimitExceeded("hashes"));
    }
    for entry in &baseline.relevant_hashes {
        let (path, hash) = entry
            .rsplit_once('=')
            .ok_or(ChangeSetError::InvalidPayload)?;
        validate_path(path)?;
        validate_hash(hash)?;
    }
    Ok(())
}

pub fn validate_change_set(set: &AgentGitChangeSet) -> Result<(), ChangeSetError> {
    if set.version != CONTRACT_VERSION {
        return Err(ChangeSetError::UnsupportedVersion);
    }
    if set.revision == 0 {
        return Err(ChangeSetError::InvalidPayload);
    }
    validate_reference(&set.id, MAX_REFERENCE_BYTES)?;
    validate_reference(&set.workspace_binding_id, MAX_REFERENCE_BYTES)?;
    validate_reference(&set.run_id, MAX_REFERENCE_BYTES)?;
    validate_reference(&set.workspace_change_set_ref, MAX_REFERENCE_BYTES)?;
    if let Some(task_id) = &set.task_id {
        validate_reference(task_id, MAX_REFERENCE_BYTES)?;
    }
    if !set.workspace_root.is_empty() {
        validate_workspace_root_text(&set.workspace_root)?;
    }
    if let Some(head) = &set.base_git_head {
        validate_commit_id(head)?;
    }
    validate_baseline(&set.baseline)?;
    if set.paths.len() > MAX_PATHS {
        return Err(ChangeSetError::LimitExceeded("paths"));
    }
    validate_hash(&set.base_dirty_fingerprint)?;
    validate_hash(&set.content_hash)?;
    validate_integration_references(set)?;
    let mut seen = BTreeSet::new();
    for path in &set.paths {
        validate_path(&path.path)?;
        validate_hash(&path.hash)?;
        if !seen.insert(path.path.as_str()) {
            return Err(ChangeSetError::InvalidPath);
        }
    }
    Ok(())
}

pub fn validate_candidate(candidate: &GitCommitCandidate) -> Result<(), ChangeSetError> {
    if candidate.version != CONTRACT_VERSION {
        return Err(ChangeSetError::UnsupportedVersion);
    }
    if candidate.revision == 0 {
        return Err(ChangeSetError::InvalidPayload);
    }
    validate_reference(&candidate.id, MAX_REFERENCE_BYTES)?;
    validate_reference(&candidate.change_set_ref, MAX_REFERENCE_BYTES)?;
    if !candidate.workspace_root.is_empty() {
        validate_workspace_root_text(&candidate.workspace_root)?;
    }
    if let Some(parent_head) = &candidate.parent_head {
        validate_commit_id(parent_head)?;
    }
    if candidate.included_paths.is_empty()
        || candidate.included_paths.len() > MAX_CANDIDATE_PATHS
        || candidate.excluded_paths.len() > MAX_PATHS
        || candidate.included_hashes.len() != candidate.included_paths.len()
    {
        return Err(ChangeSetError::LimitExceeded("candidate_paths"));
    }
    if candidate.proposed_message.len() > MAX_MESSAGE_BYTES
        || candidate.proposed_message.as_bytes().contains(&0)
    {
        return Err(ChangeSetError::LimitExceeded("message"));
    }
    if candidate.message_source.len() > MAX_REFERENCE_BYTES
        || candidate
            .message_source
            .chars()
            .any(|character| character.is_control())
    {
        return Err(ChangeSetError::InvalidPayload);
    }
    validate_reference(&candidate.verification_status, MAX_REFERENCE_BYTES)?;
    validate_hash(&candidate.diff_hash)?;
    validate_hash(&candidate.precondition_fingerprint)?;
    let mut seen = BTreeSet::new();
    for path in candidate
        .included_paths
        .iter()
        .chain(candidate.excluded_paths.iter())
    {
        validate_path(path)?;
        if !seen.insert(path.as_str()) {
            return Err(ChangeSetError::InvalidPath);
        }
    }
    let mut included_hash_paths = BTreeSet::new();
    for included in &candidate.included_hashes {
        let (path, hash) = included
            .rsplit_once('=')
            .ok_or(ChangeSetError::InvalidPath)?;
        validate_path(path)?;
        validate_hash(hash)?;
        if is_sensitive_path(path)
            || !candidate.included_paths.iter().any(|p| p == path)
            || !included_hash_paths.insert(path)
        {
            return Err(ChangeSetError::InvalidPath);
        }
    }
    if included_hash_paths.len() != candidate.included_paths.len() {
        return Err(ChangeSetError::InvalidPayload);
    }
    if let Some(commit_id) = &candidate.commit_id {
        validate_commit_id(commit_id)?;
    }
    Ok(())
}

pub fn build_candidate(
    set: &AgentGitChangeSet,
    message: String,
    now_ms: i64,
) -> Result<GitCommitCandidate, ChangeSetError> {
    validate_change_set(set)?;
    if message.len() > MAX_MESSAGE_BYTES || message.as_bytes().contains(&0) {
        return Err(ChangeSetError::LimitExceeded("message"));
    }
    if set
        .paths
        .iter()
        .any(|p| p.attribution == PathAttribution::Ambiguous)
    {
        return Err(ChangeSetError::Ambiguous);
    }
    let included = set
        .paths
        .iter()
        .filter(|p| {
            matches!(
                p.attribution,
                PathAttribution::AgentAuthored | PathAttribution::GeneratedByApprovedTool
            ) && !is_sensitive_path(&p.path)
        })
        .map(|p| (p.path.clone(), p.hash.clone()))
        .collect::<Vec<_>>();
    if included.is_empty() {
        return Err(ChangeSetError::NoChanges);
    }
    if included.len() > MAX_CANDIDATE_PATHS {
        return Err(ChangeSetError::LimitExceeded("candidate_paths"));
    }
    let included_paths = included
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let included_hashes = included
        .iter()
        .map(|(path, hash)| format!("{path}={hash}"))
        .collect::<Vec<_>>();
    let excluded_paths = set
        .paths
        .iter()
        .filter(|p| !included_paths.iter().any(|path| path == &p.path))
        .map(|p| p.path.clone())
        .collect::<Vec<_>>();
    let diff_hash = sha256(
        serde_json::to_string(&included_hashes)
            .map_err(|_| ChangeSetError::GitCommandFailed)?
            .as_bytes(),
    );
    Ok(GitCommitCandidate {
        version: CONTRACT_VERSION,
        revision: set.revision,
        id: format!("candidate-{}-{}", set.id, &diff_hash[..16]),
        change_set_ref: set.id.clone(),
        parent_head: set.base_git_head.clone(),
        included_paths,
        excluded_paths,
        diff_hash,
        proposed_message: message,
        message_source: "user_or_bounded_proposal".into(),
        verification_status: "preflight_required".into(),
        created_at_ms: now_ms,
        included_hashes,
        precondition_fingerprint: set.base_dirty_fingerprint.clone(),
        workspace_root: set.workspace_root.clone(),
        commit_id: None,
    })
}

pub async fn observe(
    payload: &[u8],
    change_set_id: &str,
    workspace_root: &str,
    now_ms: i64,
) -> Result<AgentGitChangeSet, ChangeSetError> {
    let value = serde_json::from_slice::<serde_json::Value>(payload)
        .map_err(|_| ChangeSetError::InvalidPayload)?;
    let request: ObserveRequest =
        serde_json::from_value(value).map_err(|_| ChangeSetError::InvalidPayload)?;
    let root = validate_workspace_root(workspace_root)?;
    let binding = if request.workspace_binding_id.is_empty() {
        crate::task_memory::workspace_scope_id(&root)
    } else {
        request.workspace_binding_id.clone()
    };
    if binding != crate::task_memory::workspace_scope_id(&root) {
        return Err(ChangeSetError::WorkspaceBindingMismatch);
    }
    let snapshot = capture_snapshot(root, now_ms).await?;
    let id = if change_set_id.is_empty() {
        format!(
            "change-set-{}",
            &baseline_fingerprint(&snapshot.baseline)[..16]
        )
    } else {
        change_set_id.to_owned()
    };
    let paths = snapshot
        .paths
        .iter()
        .map(|(path, state)| AttributedPath {
            path: path.clone(),
            attribution: if is_sensitive_path(path) {
                PathAttribution::Secret
            } else {
                PathAttribution::PreExistingUser
            },
            hash: state.hash.clone(),
        })
        .collect::<Vec<_>>();
    let mut set = AgentGitChangeSet {
        version: CONTRACT_VERSION,
        revision: 1,
        id,
        workspace_binding_id: binding,
        run_id: if request.run_id.is_empty() {
            "agent-run-unknown".into()
        } else {
            request.run_id
        },
        task_id: request.task_id,
        base_git_head: snapshot.head.clone(),
        base_dirty_fingerprint: baseline_fingerprint(&snapshot.baseline),
        workspace_change_set_ref: if request.workspace_change_set_ref.is_empty() {
            "git-baseline".into()
        } else {
            request.workspace_change_set_ref
        },
        paths,
        status: ChangeSetStatus::Observed,
        created_at_ms: now_ms,
        content_hash: String::new(),
        baseline: snapshot.baseline,
        workspace_root: snapshot.root.to_string_lossy().into_owned(),
        incremental_change_run_id: request.incremental_change_run_id,
        task_worktree_id: request.task_worktree_id,
    };
    validate_integration_references(&set)?;
    set.content_hash = content_hash(&set)?;
    Ok(set)
}

pub fn validate_integration_references(set: &AgentGitChangeSet) -> Result<(), ChangeSetError> {
    for reference in [&set.incremental_change_run_id, &set.task_worktree_id] {
        if let Some(reference) = reference {
            if reference.is_empty()
                || reference.len() > MAX_REFERENCE_BYTES
                || reference.contains('\0')
                || reference.chars().any(|character| character.is_control())
            {
                return Err(ChangeSetError::InvalidPath);
            }
        }
    }
    Ok(())
}

pub async fn make_candidate(
    set: &AgentGitChangeSet,
    payload: &[u8],
    now_ms: i64,
) -> Result<(AgentGitChangeSet, GitCommitCandidate), ChangeSetError> {
    validate_change_set(set)?;
    if matches!(
        set.status,
        ChangeSetStatus::Committed
            | ChangeSetStatus::Kept
            | ChangeSetStatus::Stale
            | ChangeSetStatus::UndoPending
            | ChangeSetStatus::Unknown
    ) {
        return Err(ChangeSetError::Stale);
    }
    let root = validate_workspace_root(&set.workspace_root)?;
    if set.workspace_binding_id != crate::task_memory::workspace_scope_id(&root) {
        return Err(ChangeSetError::WorkspaceBindingMismatch);
    }
    let request: CandidateRequest =
        serde_json::from_slice(payload).map_err(|_| ChangeSetError::InvalidPayload)?;
    let approved = bounded_paths(&request.approved_paths)?;
    let generated = bounded_paths(&request.generated_paths)?;
    let snapshot = capture_snapshot(root, now_ms).await?;
    if snapshot.head != set.base_git_head {
        return Err(ChangeSetError::Stale);
    }
    let baseline_paths = set
        .baseline
        .tracked_modified
        .iter()
        .chain(set.baseline.staged.iter())
        .chain(set.baseline.untracked.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut paths = Vec::new();
    for path in baseline_paths {
        let hash = snapshot
            .paths
            .get(&path)
            .map(|state| state.hash.clone())
            .unwrap_or_else(|| sha256(b"<deleted-after-baseline>"));
        paths.push(AttributedPath {
            path,
            attribution: PathAttribution::PreExistingUser,
            hash,
        });
    }
    for (path, state) in &snapshot.paths {
        if paths.iter().any(|item: &AttributedPath| item.path == *path) {
            continue;
        }
        let attribution = if is_sensitive_path(path) {
            PathAttribution::Secret
        } else if generated.contains(path) {
            PathAttribution::GeneratedByApprovedTool
        } else if approved.contains(path) {
            PathAttribution::AgentAuthored
        } else {
            PathAttribution::ExternalConcurrent
        };
        paths.push(AttributedPath {
            path: path.clone(),
            attribution,
            hash: state.hash.clone(),
        });
    }
    let mut next = set.clone();
    next.paths = paths;
    next.status = ChangeSetStatus::CandidateReady;
    next.content_hash = content_hash(&next)?;
    let message = if request.message.is_empty() {
        "Agent change set".into()
    } else {
        request.message
    };
    let mut candidate = build_candidate(&next, message, now_ms)?;
    candidate.parent_head = snapshot.head;
    candidate.precondition_fingerprint = baseline_fingerprint(&snapshot.baseline);
    candidate.included_hashes = next
        .paths
        .iter()
        .filter(|path| {
            matches!(
                path.attribution,
                PathAttribution::AgentAuthored | PathAttribution::GeneratedByApprovedTool
            ) && !is_sensitive_path(&path.path)
        })
        .map(|path| format!("{}={}", path.path, path.hash))
        .collect();
    candidate.diff_hash = sha256(
        serde_json::to_string(&candidate.included_hashes)
            .map_err(|_| ChangeSetError::GitCommandFailed)?
            .as_bytes(),
    );
    candidate.id = format!("candidate-{}-{}", set.id, &candidate.diff_hash[..16]);
    Ok((next, candidate))
}

pub async fn preflight_candidate(candidate: &GitCommitCandidate) -> Result<(), ChangeSetError> {
    validate_candidate(candidate)?;
    let root = validate_workspace_root(&candidate.workspace_root)?;
    let snapshot = capture_snapshot(root.clone(), candidate.created_at_ms).await?;
    if snapshot.head != candidate.parent_head
        || baseline_fingerprint(&snapshot.baseline) != candidate.precondition_fingerprint
    {
        return Err(ChangeSetError::Stale);
    }
    Ok(())
}

pub async fn commit_candidate(candidate: &GitCommitCandidate) -> Result<String, ChangeSetError> {
    preflight_candidate(candidate).await?;
    let root = validate_workspace_root(&candidate.workspace_root)?;
    let pathspec = nul_pathspec(&candidate.included_paths)?;
    let args = [
        "commit",
        "--only",
        "--pathspec-from-file=-",
        "--pathspec-file-nul",
        "-m",
        candidate.proposed_message.as_str(),
    ];
    run_git(&root, &args, Some(&pathspec)).await?;
    current_head(&root)
        .await?
        .ok_or(ChangeSetError::CommitOutcomeUnknown)
}

pub async fn undo_candidate(
    candidate: &GitCommitCandidate,
    baseline: &GitDirtyBaseline,
) -> Result<String, ChangeSetError> {
    validate_candidate(candidate)?;
    let root = validate_workspace_root(&candidate.workspace_root)?;
    let snapshot = capture_snapshot(root.clone(), candidate.created_at_ms).await?;
    if let Some(commit_id) = &candidate.commit_id {
        if snapshot.head.as_deref() != Some(commit_id.as_str()) {
            return Err(ChangeSetError::Stale);
        }
        if dirty_state_fingerprint(&snapshot.baseline) != dirty_state_fingerprint(baseline) {
            return Err(ChangeSetError::Stale);
        }
        run_git(&root, &["revert", "--no-edit", commit_id], None)
            .await
            .map_err(|_| ChangeSetError::CommitOutcomeUnknown)?;
        let after = capture_snapshot(root.clone(), candidate.created_at_ms)
            .await
            .map_err(|_| ChangeSetError::CommitOutcomeUnknown)?;
        if dirty_state_fingerprint(&after.baseline) != dirty_state_fingerprint(baseline) {
            return Err(ChangeSetError::CommitOutcomeUnknown);
        }
        return current_head(&root)
            .await?
            .ok_or(ChangeSetError::CommitOutcomeUnknown);
    }
    if baseline_fingerprint(&snapshot.baseline) != candidate.precondition_fingerprint {
        return Err(ChangeSetError::Stale);
    }
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    for path in &candidate.included_paths {
        let tracked_listing = run_git(&root, &["ls-files", "--stage", "--", path], None)
            .await
            .map_err(|_| ChangeSetError::CommitOutcomeUnknown)?;
        if !tracked_listing.is_empty() {
            tracked.push(path.clone());
        } else {
            untracked.push(path.clone());
        }
    }
    if !tracked.is_empty() {
        let pathspec = nul_pathspec(&tracked)?;
        run_git(
            &root,
            &[
                "restore",
                "--worktree",
                "--staged",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            Some(&pathspec),
        )
        .await
        .map_err(|_| ChangeSetError::CommitOutcomeUnknown)?;
    }
    for path in untracked {
        let full = root.join(path);
        if let Ok(metadata) = std::fs::symlink_metadata(&full) {
            if metadata.is_dir() {
                return Err(ChangeSetError::UnsafeUndo);
            }
            std::fs::remove_file(full).map_err(|_| ChangeSetError::CommitOutcomeUnknown)?;
        }
    }
    let after = capture_snapshot(root, candidate.created_at_ms)
        .await
        .map_err(|_| ChangeSetError::CommitOutcomeUnknown)?;
    if baseline_fingerprint(&after.baseline) != baseline_fingerprint(baseline) {
        return Err(ChangeSetError::CommitOutcomeUnknown);
    }
    current_head(&after.root)
        .await?
        .ok_or(ChangeSetError::CommitOutcomeUnknown)
}

pub async fn reconcile_pending_commit(
    candidate: &GitCommitCandidate,
    baseline: &GitDirtyBaseline,
) -> Result<PendingCommitReconciliation, ChangeSetError> {
    validate_candidate(candidate)?;
    if candidate.verification_status != "commit_pending" {
        return Err(ChangeSetError::InvalidPayload);
    }
    let root = validate_workspace_root(&candidate.workspace_root)?;
    let snapshot = capture_snapshot(root.clone(), candidate.created_at_ms).await?;
    if snapshot.head == candidate.parent_head
        && dirty_state_fingerprint(&snapshot.baseline) == dirty_state_fingerprint(baseline)
    {
        return Ok(PendingCommitReconciliation::NoEffect);
    }
    let head = match snapshot.head {
        Some(head) => head,
        None => return Ok(PendingCommitReconciliation::Unknown),
    };
    let parents = run_git(&root, &["rev-list", "--parents", "-n", "1", "HEAD"], None).await?;
    let parent_parts = String::from_utf8_lossy(&parents)
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let observed_parent = parent_parts.get(1).cloned();
    if parent_parts.len() > 2 || observed_parent != candidate.parent_head {
        return Ok(PendingCommitReconciliation::Unknown);
    }
    let message = run_git(&root, &["show", "-s", "--format=%B", "HEAD"], None).await?;
    let message = String::from_utf8_lossy(&message)
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    if message != candidate.proposed_message {
        return Ok(PendingCommitReconciliation::Unknown);
    }
    let changed = run_git(
        &root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--no-renames",
            "--name-only",
            "-r",
            "-z",
            "HEAD",
        ],
        None,
    )
    .await?;
    let mut changed_paths = BTreeSet::new();
    for path in changed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8(path.to_vec()).map_err(|_| ChangeSetError::InvalidPayload)?;
        validate_path(&path)?;
        changed_paths.insert(path);
    }
    let expected_paths = candidate
        .included_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if changed_paths != expected_paths
        || dirty_state_fingerprint(&snapshot.baseline) != dirty_state_fingerprint(baseline)
    {
        return Ok(PendingCommitReconciliation::Unknown);
    }
    Ok(PendingCommitReconciliation::Committed(head))
}

fn bounded_paths(paths: &[String]) -> Result<BTreeSet<String>, ChangeSetError> {
    if paths.len() > MAX_CANDIDATE_PATHS {
        return Err(ChangeSetError::LimitExceeded("approved_paths"));
    }
    paths
        .iter()
        .map(|path| {
            validate_path(path)?;
            Ok(path.clone())
        })
        .collect()
}

fn content_hash(set: &AgentGitChangeSet) -> Result<String, ChangeSetError> {
    let mut copy = set.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| ChangeSetError::GitCommandFailed)?;
    Ok(sha256(&bytes))
}

fn validate_hash(value: &str) -> Result<(), ChangeSetError> {
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(ChangeSetError::InvalidPath);
    }
    Ok(())
}

fn validate_commit_id(value: &str) -> Result<(), ChangeSetError> {
    if !matches!(value.len(), 40 | 64)
        || !value.chars().all(|character| character.is_ascii_hexdigit())
    {
        return Err(ChangeSetError::InvalidPayload);
    }
    Ok(())
}

fn validate_reference(value: &str, max_bytes: usize) -> Result<(), ChangeSetError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.chars().any(|character| character.is_control())
    {
        return Err(ChangeSetError::InvalidPayload);
    }
    Ok(())
}

fn validate_workspace_root_text(value: &str) -> Result<(), ChangeSetError> {
    if value.is_empty()
        || value.len() > MAX_WORKSPACE_ROOT_BYTES
        || value.chars().any(|character| character.is_control())
    {
        return Err(ChangeSetError::LimitExceeded("workspace_root"));
    }
    Ok(())
}

fn baseline_fingerprint(baseline: &GitDirtyBaseline) -> String {
    let mut copy = baseline.clone();
    copy.captured_at_ms = 0;
    serde_json::to_vec(&copy)
        .map(|bytes| sha256(&bytes))
        .unwrap_or_default()
}

fn dirty_state_fingerprint(baseline: &GitDirtyBaseline) -> String {
    let mut copy = baseline.clone();
    copy.head_commit = None;
    copy.captured_at_ms = 0;
    serde_json::to_vec(&copy)
        .map(|bytes| sha256(&bytes))
        .unwrap_or_default()
}

fn validate_path(path: &str) -> Result<(), ChangeSetError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .split('/')
            .any(|part| part.eq_ignore_ascii_case(".git"))
        || path.chars().any(|char| char.is_control())
    {
        return Err(ChangeSetError::InvalidPath);
    }
    Ok(())
}

fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".p12")
        || lower.contains("/secrets/")
        || lower.starts_with("secrets/")
        || lower.starts_with(".ssh/")
}

fn validate_workspace_root(value: &str) -> Result<PathBuf, ChangeSetError> {
    validate_workspace_root_text(value)?;
    let path = PathBuf::from(value);
    if !path.is_absolute() || !path.is_dir() {
        return Err(ChangeSetError::InvalidWorkspace);
    }
    path.canonicalize()
        .map_err(|_| ChangeSetError::InvalidWorkspace)
}

async fn capture_snapshot(root: PathBuf, now_ms: i64) -> Result<GitSnapshot, ChangeSetError> {
    let top = run_git(&root, &["rev-parse", "--show-toplevel"], None).await?;
    let reported = PathBuf::from(String::from_utf8_lossy(&top).trim());
    let reported = reported
        .canonicalize()
        .map_err(|_| ChangeSetError::InvalidWorkspace)?;
    let root = root
        .canonicalize()
        .map_err(|_| ChangeSetError::InvalidWorkspace)?;
    if !same_path(&root, &reported) {
        return Err(ChangeSetError::InvalidWorkspace);
    }
    let head = current_head(&root).await?;
    let status = run_git(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        None,
    )
    .await?;
    let raw = run_git(&root, &["diff", "--raw", "-z"], None).await?;
    let cached = run_git(&root, &["diff", "--cached", "--raw", "-z"], None).await?;
    if raw
        .len()
        .saturating_add(cached.len())
        .saturating_add(status.len())
        > MAX_DIFF_SUMMARY_BYTES
    {
        return Err(ChangeSetError::LimitExceeded("diff_summary"));
    }
    let paths = parse_status(&status, &root).await?;
    if paths.len() > MAX_PATHS {
        return Err(ChangeSetError::LimitExceeded("paths"));
    }
    let mut tracked_modified = Vec::new();
    let mut staged = Vec::new();
    let mut untracked = Vec::new();
    let mut relevant_hashes = Vec::new();
    for (path, state) in &paths {
        if state.modified {
            tracked_modified.push(path.clone());
        }
        if state.staged {
            staged.push(path.clone());
        }
        if state.untracked {
            untracked.push(path.clone());
        }
        relevant_hashes.push(format!("{}={}", path, state.hash));
    }
    let baseline = GitDirtyBaseline {
        head_commit: head.clone(),
        tracked_modified,
        staged,
        untracked,
        relevant_hashes,
        captured_at_ms: now_ms,
    };
    Ok(GitSnapshot {
        root,
        head,
        baseline,
        paths,
    })
}

async fn parse_status(
    status: &[u8],
    root: &Path,
) -> Result<BTreeMap<String, PathState>, ChangeSetError> {
    let mut result = BTreeMap::new();
    let entries = status
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    let mut iterator = entries.peekable();
    while let Some(entry) = iterator.next() {
        if entry.len() < 4 || entry[2] != b' ' {
            return Err(ChangeSetError::GitCommandFailed);
        }
        let x = entry[0] as char;
        let y = entry[1] as char;
        let mut path_bytes = &entry[3..];
        if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            path_bytes = iterator.next().ok_or(ChangeSetError::GitCommandFailed)?;
        }
        let path =
            String::from_utf8(path_bytes.to_vec()).map_err(|_| ChangeSetError::InvalidPath)?;
        validate_path(&path)?;
        let hash = hash_path(root, &path)?;
        result.insert(
            path,
            PathState {
                hash,
                staged: x != ' ' && x != '?',
                modified: y != ' ' && y != '?',
                untracked: x == '?' && y == '?',
            },
        );
    }
    Ok(result)
}

fn hash_path(root: &Path, path: &str) -> Result<String, ChangeSetError> {
    let full = root.join(path);
    let metadata = match std::fs::symlink_metadata(&full) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(sha256(b"<deleted>")),
    };
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(full).map_err(|_| ChangeSetError::GitCommandFailed)?;
        return Ok(sha256(target.to_string_lossy().as_bytes()));
    }
    if metadata.is_dir() {
        return Ok(sha256(b"<directory>"));
    }
    let mut file = std::fs::File::open(full).map_err(|_| ChangeSetError::GitCommandFailed)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_usize;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| ChangeSetError::GitCommandFailed)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        if total > MAX_EVIDENCE_BYTES.saturating_mul(16) {
            return Err(ChangeSetError::LimitExceeded("file_hash"));
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn current_head(root: &Path) -> Result<Option<String>, ChangeSetError> {
    match run_git(root, &["rev-parse", "--verify", "HEAD"], None).await {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).trim().to_owned())),
        Err(ChangeSetError::GitCommandFailed) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn run_git(
    root: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, ChangeSetError> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| ChangeSetError::GitCommandFailed)?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take().ok_or(ChangeSetError::GitCommandFailed)?;
        stdin
            .write_all(input)
            .await
            .map_err(|_| ChangeSetError::GitCommandFailed)?;
        stdin
            .shutdown()
            .await
            .map_err(|_| ChangeSetError::GitCommandFailed)?;
    }
    let output = timeout(GIT_COMMAND_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| ChangeSetError::GitCommandTimedOut)?
        .map_err(|_| ChangeSetError::GitCommandFailed)?;
    if output.stdout.len() > MAX_DIFF_SUMMARY_BYTES {
        return Err(ChangeSetError::LimitExceeded("git_output"));
    }
    if !output.status.success() {
        return Err(ChangeSetError::GitCommandFailed);
    }
    Ok(output.stdout)
}

fn nul_pathspec(paths: &[String]) -> Result<Vec<u8>, ChangeSetError> {
    if paths.len() > MAX_CANDIDATE_PATHS {
        return Err(ChangeSetError::LimitExceeded("candidate_paths"));
    }
    let mut output = Vec::new();
    for path in paths {
        validate_path(path)?;
        output.extend_from_slice(path.as_bytes());
        output.push(0);
    }
    Ok(output)
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: Vec<AttributedPath>) -> AgentGitChangeSet {
        AgentGitChangeSet {
            version: 1,
            revision: 1,
            id: "set-1".into(),
            workspace_binding_id: "ws-1".into(),
            run_id: "run-1".into(),
            task_id: None,
            base_git_head: Some("a".repeat(40)),
            base_dirty_fingerprint: "b".repeat(64),
            workspace_change_set_ref: "wcs-1".into(),
            paths,
            status: ChangeSetStatus::Observed,
            created_at_ms: 1,
            content_hash: "c".repeat(64),
            baseline: GitDirtyBaseline::default(),
            workspace_root: String::new(),
            incremental_change_run_id: None,
            task_worktree_id: None,
        }
    }

    #[test]
    fn excludes_preexisting_and_builds_stable_candidate() {
        let s = set(vec![
            AttributedPath {
                path: "src/a.rs".into(),
                attribution: PathAttribution::AgentAuthored,
                hash: "a".repeat(64),
            },
            AttributedPath {
                path: "README.md".into(),
                attribution: PathAttribution::PreExistingUser,
                hash: "b".repeat(64),
            },
        ]);
        let c = build_candidate(&s, "feat: safe change".into(), 2).unwrap();
        assert_eq!(c.included_paths, vec!["src/a.rs"]);
        assert_eq!(c.excluded_paths, vec!["README.md"]);
    }

    #[test]
    fn excludes_external_and_sensitive_paths() {
        let external = set(vec![
            AttributedPath {
                path: "x".into(),
                attribution: PathAttribution::ExternalConcurrent,
                hash: "a".repeat(64),
            },
            AttributedPath {
                path: "y".into(),
                attribution: PathAttribution::AgentAuthored,
                hash: "b".repeat(64),
            },
        ]);
        let candidate = build_candidate(&external, "x".into(), 1).unwrap();
        assert_eq!(candidate.included_paths, vec!["y".to_owned()]);
        assert_eq!(candidate.excluded_paths, vec!["x".to_owned()]);
        let sensitive = set(vec![
            AttributedPath {
                path: ".env".into(),
                attribution: PathAttribution::AgentAuthored,
                hash: "a".repeat(64),
            },
            AttributedPath {
                path: "y".into(),
                attribution: PathAttribution::AgentAuthored,
                hash: "b".repeat(64),
            },
        ]);
        let candidate = build_candidate(&sensitive, "x".into(), 1).unwrap();
        assert_eq!(candidate.included_paths, vec!["y".to_owned()]);
        assert_eq!(candidate.excluded_paths, vec![".env".to_owned()]);
    }

    #[test]
    fn integration_references_are_bounded_and_optional() {
        let mut set = set(Vec::new());
        assert!(validate_integration_references(&set).is_ok());
        set.incremental_change_run_id = Some("incremental-run-1".into());
        set.task_worktree_id = Some("worktree-1".into());
        assert!(validate_integration_references(&set).is_ok());
        set.task_worktree_id = Some("bad\0reference".into());
        assert_eq!(
            validate_integration_references(&set),
            Err(ChangeSetError::InvalidPath)
        );
    }

    #[tokio::test]
    async fn malformed_payloads_fail_closed() {
        assert_eq!(
            observe(b"not-json", "set-invalid", "", 1).await,
            Err(ChangeSetError::InvalidPayload)
        );
        assert_eq!(
            observe(
                br#"{"base_git_head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
                "set-invalid",
                "",
                1,
            )
            .await,
            Err(ChangeSetError::LimitExceeded("workspace_root"))
        );
    }

    #[test]
    fn rejects_traversal_and_measures_utf8_limits_in_bytes() {
        assert_eq!(
            validate_baseline(&GitDirtyBaseline {
                tracked_modified: vec!["../x".into()],
                ..GitDirtyBaseline::default()
            }),
            Err(ChangeSetError::InvalidPath)
        );
        let s = set(vec![AttributedPath {
            path: "x".into(),
            attribution: PathAttribution::AgentAuthored,
            hash: "a".repeat(64),
        }]);
        assert_eq!(
            build_candidate(&s, "я".repeat(MAX_MESSAGE_BYTES), 1),
            Err(ChangeSetError::LimitExceeded("message"))
        );
    }

    #[tokio::test]
    async fn captures_baseline_and_commits_only_approved_path() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        git(&root, &["init"]).await;
        git(&root, &["config", "user.email", "test@example.invalid"]).await;
        git(&root, &["config", "user.name", "EvoHime Test"]).await;
        std::fs::write(root.join("existing.txt"), "existing").unwrap();
        git(&root, &["add", "existing.txt"]).await;
        git(&root, &["commit", "-m", "initial"]).await;
        let binding = crate::task_memory::workspace_scope_id(&root);
        let payload =
            serde_json::json!({"workspace_binding_id": binding, "run_id":"run-1"}).to_string();
        let set = observe(payload.as_bytes(), "set-1", &root.to_string_lossy(), 1)
            .await
            .unwrap();
        std::fs::write(root.join("agent.txt"), "agent").unwrap();
        std::fs::write(root.join("user.txt"), "user").unwrap();
        git(&root, &["add", "user.txt"]).await;
        let payload =
            serde_json::json!({"message":"agent: change", "approved_paths":["agent.txt"]})
                .to_string();
        let (set, candidate) = make_candidate(&set, payload.as_bytes(), 2).await.unwrap();
        assert_eq!(candidate.included_paths, vec!["agent.txt"]);
        assert!(candidate.excluded_paths.contains(&"user.txt".to_owned()));
        let commit = commit_candidate(&candidate).await.unwrap();
        assert!(!commit.is_empty());
        assert!(root.join("agent.txt").exists());
        assert!(root.join("user.txt").exists());
        let status = git_output(&root, &["status", "--porcelain=v1"]).await;
        assert!(
            status.contains("A  user.txt"),
            "unrelated staged change was committed: {status}"
        );
        assert_eq!(set.status, ChangeSetStatus::CandidateReady);
    }

    #[tokio::test]
    async fn committed_undo_reverts_only_after_baseline_reconciliation() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        git(&root, &["init"]).await;
        git(&root, &["config", "user.email", "test@example.invalid"]).await;
        git(&root, &["config", "user.name", "EvoHime Test"]).await;
        std::fs::write(root.join("existing.txt"), "existing").unwrap();
        git(&root, &["add", "existing.txt"]).await;
        git(&root, &["commit", "-m", "initial"]).await;

        let binding = crate::task_memory::workspace_scope_id(&root);
        let observe_payload = serde_json::json!({"workspace_binding_id": binding}).to_string();
        let set = observe(
            observe_payload.as_bytes(),
            "set-undo",
            &root.to_string_lossy(),
            1,
        )
        .await
        .unwrap();
        std::fs::write(root.join("agent.txt"), "agent").unwrap();
        let candidate_payload =
            serde_json::json!({"message":"agent: undoable", "approved_paths":["agent.txt"]})
                .to_string();
        let (_next, candidate) = make_candidate(&set, candidate_payload.as_bytes(), 2)
            .await
            .unwrap();
        let mut pending = candidate.clone();
        pending.verification_status = "commit_pending".into();
        assert_eq!(
            reconcile_pending_commit(&pending, &set.baseline)
                .await
                .unwrap(),
            PendingCommitReconciliation::NoEffect
        );
        let commit_id = commit_candidate(&candidate).await.unwrap();
        pending = candidate.clone();
        pending.verification_status = "commit_pending".into();
        assert_eq!(
            reconcile_pending_commit(&pending, &set.baseline)
                .await
                .unwrap(),
            PendingCommitReconciliation::Committed(commit_id.clone())
        );
        let mut committed = candidate;
        committed.commit_id = Some(commit_id.clone());
        let reverted_head = undo_candidate(&committed, &set.baseline).await.unwrap();

        assert_ne!(reverted_head, commit_id);
        assert!(!root.join("agent.txt").exists());
    }

    async fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(root)
            .args(args)
            .status()
            .await
            .unwrap();
        assert!(status.success(), "git command failed: {args:?}");
    }

    async fn git_output(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(output.status.success(), "git command failed: {args:?}");
        String::from_utf8(output.stdout).unwrap()
    }
}
