//! Core-owned target identity and linearizable target transition contract.

use crate::task_memory::workspace_scope_id;
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;

/// Maximum encoded target identifier length in bytes.
pub const MAX_TARGET_ID_BYTES: usize = 80;
/// Maximum reason text length accepted for a target transition.
pub const MAX_TARGET_REASON_BYTES: usize = 256;

/// Stable identity tuple for one active runtime target generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIdentity {
    /// Content-derived identifier for this target generation.
    pub target_id: String,
    /// Normalized scope identifier for the workspace.
    pub workspace_scope: String,
    /// Model or execution route selected for the target.
    pub route_id: String,
    /// Backend selected for the target.
    pub backend_id: String,
    /// Identifier of the Core process that owns the target.
    pub core_instance_id: String,
    /// Session epoch within the Core instance.
    pub session_epoch: u64,
    /// Monotonically increasing generation for target changes.
    pub target_generation: u64,
}

impl TargetIdentity {
    /// Derives a stable target identity from workspace and runtime generation inputs.
    pub fn from_workspace(
        workspace: &Path,
        route_id: impl Into<String>,
        backend_id: impl Into<String>,
        core_instance_id: impl Into<String>,
        session_epoch: u64,
        target_generation: u64,
    ) -> Self {
        let workspace_scope = workspace_scope_id(workspace);
        let route_id = route_id.into();
        let backend_id = backend_id.into();
        let core_instance_id = core_instance_id.into();
        let mut digest = Sha256::new();
        digest.update(workspace_scope.as_bytes());
        digest.update([0]);
        digest.update(route_id.as_bytes());
        digest.update([0]);
        digest.update(backend_id.as_bytes());
        digest.update([0]);
        digest.update(core_instance_id.as_bytes());
        digest.update(session_epoch.to_le_bytes());
        digest.update(target_generation.to_le_bytes());
        let digest = digest.finalize();
        let target_id = format!(
            "target-{}",
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
        .chars()
        .take(MAX_TARGET_ID_BYTES)
        .collect();
        Self {
            target_id,
            workspace_scope,
            route_id,
            backend_id,
            core_instance_id,
            session_epoch,
            target_generation,
        }
    }

    /// Returns whether both identities belong to the same Core session generation.
    pub fn same_generation(&self, other: &Self) -> bool {
        self.core_instance_id == other.core_instance_id && self.session_epoch == other.session_epoch
    }
}

/// Current state of the target manager during or between transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetState {
    /// A target is active and may accept matching results.
    Active(TargetIdentity),
    /// A transition is in progress and results are temporarily rejected.
    Switching {
        /// The target that was active when the transition began.
        old: TargetIdentity,
        /// Generation requested by the transition.
        next_generation: u64,
    },
}

/// Event data describing an accepted target change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetChanged {
    /// Identifier of the newly active target.
    pub target_id: String,
    /// Generation of the newly active target.
    pub target_generation: u64,
    /// Core process that owns the new target.
    pub core_instance_id: String,
    /// Session epoch associated with the new target.
    pub session_epoch: u64,
}

/// Rejected target transition or stale generation operation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TargetError {
    /// The caller's expected generation does not match the active target.
    #[error("target expected generation is stale")]
    StaleGeneration,
    /// Another target transition is currently in progress.
    #[error("target transition is already in progress")]
    TransitionInProgress,
}

/// Owns the active target and serializes generation-checked switches.
#[derive(Debug, Clone)]
pub struct TargetManager {
    state: TargetState,
}

impl TargetManager {
    /// Creates a manager with the supplied target active.
    pub fn new(initial: TargetIdentity) -> Self {
        Self {
            state: TargetState::Active(initial),
        }
    }

    /// Returns the active target, or `None` while a transition is in progress.
    pub fn active(&self) -> Option<&TargetIdentity> {
        match &self.state {
            TargetState::Active(target) => Some(target),
            TargetState::Switching { .. } => None,
        }
    }

    /// Switches to a target only if the active generation matches the caller's expectation.
    pub fn switch(
        &mut self,
        next: TargetIdentity,
        expected_generation: u64,
    ) -> Result<TargetChanged, TargetError> {
        let current = self
            .active()
            .ok_or(TargetError::TransitionInProgress)?
            .clone();
        if current.target_generation != expected_generation {
            return Err(TargetError::StaleGeneration);
        }
        self.state = TargetState::Switching {
            old: current,
            next_generation: next.target_generation,
        };
        let changed = TargetChanged {
            target_id: next.target_id.clone(),
            target_generation: next.target_generation,
            core_instance_id: next.core_instance_id.clone(),
            session_epoch: next.session_epoch,
        };
        self.state = TargetState::Active(next);
        Ok(changed)
    }

    /// Checks whether a result still belongs to the currently active target generation.
    pub fn accepts_result(&self, target_id: &str, target_generation: u64) -> bool {
        self.active().is_some_and(|target| {
            target.target_id == target_id && target.target_generation == target_generation
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(path: &str, generation: u64) -> TargetIdentity {
        TargetIdentity::from_workspace(Path::new(path), "route", "builtin", "core", 1, generation)
    }

    #[test]
    fn normalized_workspace_forms_share_scope_but_target_generation_changes() {
        let a = target("C:\\Work\\", 1);
        let b = target("c:/work", 2);
        assert_eq!(a.workspace_scope, b.workspace_scope);
        assert_ne!(a.target_id, b.target_id);
    }

    #[test]
    fn stale_switch_and_late_result_are_rejected() {
        let first = target("C:\\Work", 1);
        let second = target("D:\\Work", 2);
        let mut manager = TargetManager::new(first.clone());
        assert_eq!(
            manager.switch(second.clone(), 0),
            Err(TargetError::StaleGeneration)
        );
        manager.switch(second.clone(), 1).expect("switch");
        assert!(!manager.accepts_result(&first.target_id, first.target_generation));
        assert!(manager.accepts_result(&second.target_id, second.target_generation));
    }
}
