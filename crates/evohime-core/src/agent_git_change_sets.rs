//! Core-owned, metadata-only contract for safe agent Git change-set candidates.
//! Git is never inferred from model output: callers provide bounded observed
//! evidence and Core validates the immutable baseline before an action.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_PATHS: usize = 256;
pub const MAX_CANDIDATE_PATHS: usize = 128;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_MESSAGE_BYTES: usize = 4096;
pub const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
pub const MAX_DIFF_SUMMARY_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathAttribution {
    AgentAuthored,
    PreExistingUser,
    ExternalConcurrent,
    GeneratedByApprovedTool,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitCandidate {
    pub version: u32,
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
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChangeSetError {
    #[error("unsupported contract version")]
    UnsupportedVersion,
    #[error("invalid or unsafe path")]
    InvalidPath,
    #[error("bounded limit exceeded: {0}")]
    LimitExceeded(&'static str),
    #[error("candidate is stale")]
    Stale,
    #[error("ambiguous attribution requires explicit review")]
    Ambiguous,
    #[error("candidate has unrelated staged changes")]
    SharedIndex,
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn validate_baseline(baseline: &GitDirtyBaseline) -> Result<(), ChangeSetError> {
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
    Ok(())
}

pub fn validate_change_set(set: &AgentGitChangeSet) -> Result<(), ChangeSetError> {
    if set.version != CONTRACT_VERSION {
        return Err(ChangeSetError::UnsupportedVersion);
    }
    validate_baseline(&GitDirtyBaseline {
        head_commit: set.base_git_head.clone(),
        tracked_modified: vec![],
        staged: vec![],
        untracked: vec![],
        relevant_hashes: vec![],
        captured_at_ms: set.created_at_ms,
    })?;
    if set.paths.len() > MAX_PATHS {
        return Err(ChangeSetError::LimitExceeded("paths"));
    }
    for path in &set.paths {
        validate_path(&path.path)?;
    }
    Ok(())
}

pub fn build_candidate(
    set: &AgentGitChangeSet,
    message: String,
    now_ms: i64,
) -> Result<GitCommitCandidate, ChangeSetError> {
    validate_change_set(set)?;
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(ChangeSetError::LimitExceeded("message"));
    }
    let ambiguous = set
        .paths
        .iter()
        .any(|p| p.attribution == PathAttribution::Ambiguous);
    if ambiguous {
        return Err(ChangeSetError::Ambiguous);
    }
    let included_paths = set
        .paths
        .iter()
        .filter(|p| {
            matches!(
                p.attribution,
                PathAttribution::AgentAuthored | PathAttribution::GeneratedByApprovedTool
            )
        })
        .map(|p| p.path.clone())
        .collect::<Vec<_>>();
    if included_paths.len() > MAX_CANDIDATE_PATHS {
        return Err(ChangeSetError::LimitExceeded("candidate_paths"));
    }
    let excluded_paths = set
        .paths
        .iter()
        .filter(|p| !included_paths.contains(&p.path))
        .map(|p| p.path.clone())
        .collect::<Vec<_>>();
    let diff_hash = sha256(
        serde_json::to_string(&included_paths)
            .unwrap_or_default()
            .as_bytes(),
    );
    Ok(GitCommitCandidate {
        version: CONTRACT_VERSION,
        id: format!("candidate-{}", &diff_hash[..16]),
        change_set_ref: set.id.clone(),
        parent_head: set.base_git_head.clone(),
        included_paths,
        excluded_paths,
        diff_hash,
        proposed_message: message,
        message_source: "user_or_bounded_proposal".into(),
        verification_status: "preflight_required".into(),
        created_at_ms: now_ms,
    })
}

fn validate_path(path: &str) -> Result<(), ChangeSetError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains("..\\")
        || path.contains("../")
        || path.contains(':')
    {
        return Err(ChangeSetError::InvalidPath);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(paths: Vec<AttributedPath>) -> AgentGitChangeSet {
        AgentGitChangeSet {
            version: 1,
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
    fn ambiguous_and_traversal_are_rejected() {
        let s = set(vec![AttributedPath {
            path: "x".into(),
            attribution: PathAttribution::Ambiguous,
            hash: "a".repeat(64),
        }]);
        assert_eq!(
            build_candidate(&s, "x".into(), 1),
            Err(ChangeSetError::Ambiguous)
        );
        assert_eq!(
            validate_baseline(&GitDirtyBaseline {
                head_commit: None,
                tracked_modified: vec!["../x".into()],
                staged: vec![],
                untracked: vec![],
                relevant_hashes: vec![],
                captured_at_ms: 0
            }),
            Err(ChangeSetError::InvalidPath)
        );
    }
}
