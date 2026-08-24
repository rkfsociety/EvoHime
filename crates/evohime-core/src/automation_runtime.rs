//! Core-owned automation queue/FSM/lease primitives (plan 16.2).

use std::collections::{HashMap, VecDeque};

use crate::automation::AutomationRunState;

pub const MAX_PENDING_COMMANDS: usize = 256;
pub const MAX_PROGRESS_MESSAGES: usize = 1024;
pub const LEASE_TTL_MS: i64 = 30_000;
pub const CANCEL_DEADLINE_MS: i64 = 5_000;
pub const PROVIDER_DEADLINE_MS: i64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    InvalidTransition,
    StaleGeneration,
    LeaseConflict,
    QueueFull,
    OperationLocked,
    PolicyRevalidationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOperation {
    pub operation_id: String,
    pub deadline_ms: i64,
    pub cancelled: bool,
}

impl ProviderOperation {
    pub fn new(operation_id: &str, now_ms: i64) -> Self {
        Self {
            operation_id: operation_id.into(),
            deadline_ms: now_ms + PROVIDER_DEADLINE_MS,
            cancelled: false,
        }
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
    pub fn expired(&self, now_ms: i64) -> bool {
        self.cancelled || now_ms >= self.deadline_ms
    }
    pub fn retryable_error(code: &str) -> bool {
        matches!(
            code,
            "provider_timeout" | "provider_unavailable" | "transport_timeout"
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStateMachine {
    pub state: AutomationRunState,
    pub generation: u64,
}

impl RunStateMachine {
    pub fn new() -> Self {
        Self {
            state: AutomationRunState::Admitted,
            generation: 1,
        }
    }
    pub fn transition(
        &mut self,
        next: AutomationRunState,
        generation: u64,
    ) -> Result<(), RuntimeError> {
        if generation != self.generation || !allowed(self.state, next) {
            return Err(if generation != self.generation {
                RuntimeError::StaleGeneration
            } else {
                RuntimeError::InvalidTransition
            });
        }
        self.state = next;
        Ok(())
    }
    pub fn fence(&self, generation: u64) -> Result<(), RuntimeError> {
        (generation == self.generation)
            .then_some(())
            .ok_or(RuntimeError::StaleGeneration)
    }
    pub fn takeover(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }
}

fn allowed(from: AutomationRunState, to: AutomationRunState) -> bool {
    use AutomationRunState::*;
    if matches!(from, Completed | Failed | Cancelled | DeadLetter) {
        return false;
    }
    matches!(
        (from, to),
        (Admitted, Queued)
            | (Queued, Starting)
            | (Starting, Running)
            | (Running, WaitingApproval)
            | (Running, Retrying)
            | (Running, Cancelling)
            | (WaitingApproval, Running)
            | (WaitingApproval, Cancelling)
            | (Retrying, Starting)
            | (Retrying, Failed)
            | (Starting, Cancelling)
            | (Cancelling, Cancelled)
            | (Running, Completed)
            | (Running, Failed)
            | (Running, DeadLetter)
            | (Queued, DeadLetter)
            | (Starting, DeadLetter)
            | (WaitingApproval, DeadLetter)
            | (Retrying, DeadLetter)
            | (Paused, Running)
            | (Running, Paused)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationQueue<T> {
    commands: VecDeque<T>,
    progress: HashMap<(String, String), T>,
}

impl<T: Clone> AutomationQueue<T> {
    pub fn new() -> Self {
        Self {
            commands: VecDeque::new(),
            progress: HashMap::new(),
        }
    }
    pub fn push_command(&mut self, command: T) -> Result<(), RuntimeError> {
        if self.commands.len() >= MAX_PENDING_COMMANDS {
            return Err(RuntimeError::QueueFull);
        }
        self.commands.push_back(command);
        Ok(())
    }
    pub fn push_progress(
        &mut self,
        run_id: impl Into<String>,
        activity_id: impl Into<String>,
        message: T,
    ) -> Result<(), RuntimeError> {
        let key = (run_id.into(), activity_id.into());
        if self.progress.len() >= MAX_PROGRESS_MESSAGES && !self.progress.contains_key(&key) {
            return Err(RuntimeError::QueueFull);
        }
        self.progress.insert(key, message);
        Ok(())
    }
    pub fn pop_command(&mut self) -> Option<T> {
        self.commands.pop_front()
    }
    pub fn command_len(&self) -> usize {
        self.commands.len()
    }
    pub fn progress_len(&self) -> usize {
        self.progress.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub owner: String,
    pub generation: u64,
    pub expires_at_ms: i64,
}

impl Lease {
    pub fn acquire(
        current: Option<&Lease>,
        owner: &str,
        generation: u64,
        now_ms: i64,
    ) -> Result<Self, RuntimeError> {
        if current.is_some_and(|lease| lease.expires_at_ms > now_ms) {
            return Err(RuntimeError::LeaseConflict);
        }
        Ok(Self {
            owner: owner.into(),
            generation,
            expires_at_ms: now_ms + LEASE_TTL_MS,
        })
    }
    pub fn renew(&mut self, owner: &str, generation: u64, now_ms: i64) -> Result<(), RuntimeError> {
        if self.owner != owner || self.generation != generation || self.expires_at_ms <= now_ms {
            return Err(RuntimeError::StaleGeneration);
        }
        self.expires_at_ms = now_ms + LEASE_TTL_MS;
        Ok(())
    }
    pub fn takeover(
        &self,
        owner: &str,
        generation: u64,
        now_ms: i64,
    ) -> Result<Self, RuntimeError> {
        if self.expires_at_ms > now_ms {
            return Err(RuntimeError::LeaseConflict);
        }
        Ok(Self {
            owner: owner.into(),
            generation: generation.saturating_add(1),
            expires_at_ms: now_ms + LEASE_TTL_MS,
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OperationLock {
    operation_id: Option<String>,
}
impl OperationLock {
    pub fn acquire(&mut self, operation_id: &str) -> Result<(), RuntimeError> {
        if self.operation_id.is_some() {
            return Err(RuntimeError::OperationLocked);
        }
        self.operation_id = Some(operation_id.into());
        Ok(())
    }
    pub fn release(&mut self, operation_id: &str) -> Result<(), RuntimeError> {
        if self.operation_id.as_deref() != Some(operation_id) {
            return Err(RuntimeError::StaleGeneration);
        }
        self.operation_id = None;
        Ok(())
    }
}

pub fn revalidate_effect(
    owner_scope: &str,
    expected_scope: &str,
    capability_hash: &str,
    expected_capability_hash: &str,
    policy_snapshot: &str,
    expected_policy_snapshot: &str,
    approval_snapshot: &str,
    expected_approval_snapshot: &str,
) -> Result<(), RuntimeError> {
    (owner_scope == expected_scope
        && capability_hash == expected_capability_hash
        && policy_snapshot == expected_policy_snapshot
        && approval_snapshot == expected_approval_snapshot)
        .then_some(())
        .ok_or(RuntimeError::PolicyRevalidationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_generation_cannot_transition_after_takeover() {
        let mut fsm = RunStateMachine::new();
        fsm.transition(AutomationRunState::Queued, 1).unwrap();
        let old = fsm.generation;
        assert_eq!(fsm.takeover(), 2);
        assert_eq!(
            fsm.transition(AutomationRunState::Starting, old),
            Err(RuntimeError::StaleGeneration)
        );
    }
    #[test]
    fn queue_rejects_commands_but_coalesces_progress() {
        let mut q = AutomationQueue::new();
        for _ in 0..MAX_PENDING_COMMANDS {
            q.push_command(1).unwrap();
        }
        assert_eq!(q.push_command(1), Err(RuntimeError::QueueFull));
        q.push_progress("r", "a", 1).unwrap();
        q.push_progress("r", "a", 2).unwrap();
        assert_eq!(q.progress_len(), 1);
    }
    #[test]
    fn expired_lease_can_be_taken_over_but_live_one_cannot() {
        let lease = Lease {
            owner: "a".into(),
            generation: 1,
            expires_at_ms: 10,
        };
        assert_eq!(lease.takeover("b", 1, 9), Err(RuntimeError::LeaseConflict));
        assert_eq!(lease.takeover("b", 1, 10).unwrap().generation, 2);
    }
    #[test]
    fn effect_revalidation_is_fail_closed() {
        assert!(revalidate_effect("o", "o", "c", "c", "p", "p", "a", "a").is_ok());
        assert_eq!(
            revalidate_effect("o", "x", "c", "c", "p", "p", "a", "a"),
            Err(RuntimeError::PolicyRevalidationFailed)
        );
    }
    #[test]
    fn provider_operation_is_bounded_and_only_transient_errors_retry() {
        let mut op = ProviderOperation::new("op", 100);
        assert!(!op.expired(101));
        assert!(ProviderOperation::retryable_error("provider_timeout"));
        assert!(!ProviderOperation::retryable_error("approval_denied"));
        op.cancel();
        assert!(op.expired(101));
    }
}
