//! Core-owned, metadata-only contract for task worktree isolation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BRANCH_BYTES: usize = 128;
pub const MAX_ROOT_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    Planned,
    Creating,
    Ready,
    Integrating,
    Integrated,
    CleanupPending,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    Primary,
    Isolated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWorktreeIsolationDefinition {
    pub schema_version: u32,
    pub worktree_id: String,
    pub task_id: String,
    pub repository_scope: String,
    pub branch: String,
    pub root_ref: String,
    pub base_commit: String,
    pub mode: IsolationMode,
    pub state: WorktreeState,
    pub version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWorktreeIsolationPolicy {
    pub schema_version: u32,
    pub enabled: bool,
    pub allow_auxiliary_files: bool,
    pub cleanup_unintegrated: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("unsupported task worktree schema")]
    Version,
    #[error("task worktree field is invalid")]
    Invalid,
    #[error("task worktree field is too large")]
    TooLarge,
    #[error("branch/ref injection is not allowed")]
    RefInjection,
}

pub fn validate(definition: &TaskWorktreeIsolationDefinition) -> Result<(), ContractError> {
    if definition.schema_version != SCHEMA_VERSION
        || definition.worktree_id.is_empty()
        || definition.task_id.is_empty()
        || definition.repository_scope.is_empty()
        || definition.base_commit.is_empty()
        || definition.idempotency_key.is_empty()
    {
        return Err(ContractError::Invalid);
    }
    if definition.branch.len() > MAX_BRANCH_BYTES || definition.root_ref.len() > MAX_ROOT_BYTES {
        return Err(ContractError::TooLarge);
    }
    if definition.branch.starts_with('-')
        || definition.branch.contains("..")
        || definition.branch.contains('\0')
        || definition.branch.contains(' ')
        || definition.branch.contains('~')
    {
        return Err(ContractError::RefInjection);
    }
    if definition.root_ref.contains('\0') || definition.root_ref.contains('\n') {
        return Err(ContractError::Invalid);
    }
    Ok(())
}

pub fn canonical_hash(
    definition: &TaskWorktreeIsolationDefinition,
) -> Result<String, ContractError> {
    validate(definition)?;
    let bytes = serde_json::to_vec(definition).map_err(|_| ContractError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> TaskWorktreeIsolationDefinition {
        TaskWorktreeIsolationDefinition {
            schema_version: 1,
            worktree_id: "wt".into(),
            task_id: "task".into(),
            repository_scope: "repo".into(),
            branch: "eva/task".into(),
            root_ref: "worktree/task".into(),
            base_commit: "a".repeat(40),
            mode: IsolationMode::Isolated,
            state: WorktreeState::Planned,
            version: 1,
            idempotency_key: "idem".into(),
        }
    }
    #[test]
    fn valid_definition_is_deterministically_hashed() {
        let value = definition();
        assert_eq!(
            canonical_hash(&value).unwrap(),
            canonical_hash(&value).unwrap()
        );
    }
    #[test]
    fn ref_injection_and_unknown_version_fail_closed() {
        let mut value = definition();
        value.branch = "--delete".into();
        assert_eq!(validate(&value), Err(ContractError::RefInjection));
        value = definition();
        value.schema_version = 2;
        assert_eq!(validate(&value), Err(ContractError::Invalid));
    }
}
