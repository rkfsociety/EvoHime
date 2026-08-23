//! Core-owned target identity and linearizable target transition contract.

use crate::task_memory::workspace_scope_id;
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;

pub const MAX_TARGET_ID_BYTES: usize = 80;
pub const MAX_TARGET_REASON_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIdentity {
    pub target_id: String,
    pub workspace_scope: String,
    pub route_id: String,
    pub backend_id: String,
    pub core_instance_id: String,
    pub session_epoch: u64,
    pub target_generation: u64,
}

impl TargetIdentity {
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

    pub fn same_generation(&self, other: &Self) -> bool {
        self.core_instance_id == other.core_instance_id && self.session_epoch == other.session_epoch
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetState {
    Active(TargetIdentity),
    Switching {
        old: TargetIdentity,
        next_generation: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetChanged {
    pub target_id: String,
    pub target_generation: u64,
    pub core_instance_id: String,
    pub session_epoch: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TargetError {
    #[error("target expected generation is stale")]
    StaleGeneration,
    #[error("target transition is already in progress")]
    TransitionInProgress,
}

#[derive(Debug, Clone)]
pub struct TargetManager {
    state: TargetState,
}

impl TargetManager {
    pub fn new(initial: TargetIdentity) -> Self {
        Self {
            state: TargetState::Active(initial),
        }
    }

    pub fn active(&self) -> Option<&TargetIdentity> {
        match &self.state {
            TargetState::Active(target) => Some(target),
            TargetState::Switching { .. } => None,
        }
    }

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
