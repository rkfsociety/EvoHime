//! Core-owned automation queue/FSM/lease primitives (plan 16.2).

use std::collections::{HashMap, VecDeque};

use crate::automation::AutomationRunState;

/// Maximum number of queued commands awaiting execution.
pub const MAX_PENDING_COMMANDS: usize = 256;
/// Maximum number of distinct coalesced progress messages retained.
pub const MAX_PROGRESS_MESSAGES: usize = 1024;
/// Lease lifetime before another worker may take over, in milliseconds.
pub const LEASE_TTL_MS: i64 = 30_000;
/// Deadline allowed for cancellation to finish, in milliseconds.
pub const CANCEL_DEADLINE_MS: i64 = 5_000;
/// Maximum provider operation duration, in milliseconds.
pub const PROVIDER_DEADLINE_MS: i64 = 120_000;

/// Queue, transition, ownership, or policy-check failure in automation runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// Requested run-state transition is not allowed.
    InvalidTransition,
    /// Worker generation no longer owns the run.
    StaleGeneration,
    /// An unexpired lease already belongs to another worker.
    LeaseConflict,
    /// A bounded command or progress queue has reached capacity.
    QueueFull,
    /// Another operation currently holds the exclusive lock.
    OperationLocked,
    /// Effect authority no longer matches current policy and approval snapshots.
    PolicyRevalidationFailed,
}

/// Bounded provider operation with explicit cancellation and deadline state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOperation {
    /// Identifier used to correlate this provider operation.
    pub operation_id: String,
    /// Absolute deadline in milliseconds.
    pub deadline_ms: i64,
    /// Whether cancellation has been requested.
    pub cancelled: bool,
}

impl ProviderOperation {
    /// Creates an operation whose deadline is measured from `now_ms`.
    pub fn new(operation_id: &str, now_ms: i64) -> Self {
        Self {
            operation_id: operation_id.into(),
            deadline_ms: now_ms + PROVIDER_DEADLINE_MS,
            cancelled: false,
        }
    }
    /// Marks the operation as cancelled.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
    /// Returns true after cancellation or when the provider deadline has passed.
    pub fn expired(&self, now_ms: i64) -> bool {
        self.cancelled || now_ms >= self.deadline_ms
    }
    /// Classifies known transient provider/transport timeout codes.
    pub fn retryable_error(code: &str) -> bool {
        matches!(
            code,
            "provider_timeout" | "provider_unavailable" | "transport_timeout"
        )
    }
}

/// Run-state machine fenced by a worker generation number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStateMachine {
    /// Current automation lifecycle state.
    pub state: AutomationRunState,
    /// Current owner generation; increments invalidate previous workers.
    pub generation: u64,
}

impl RunStateMachine {
    /// Starts in the admitted state with the initial generation.
    pub fn new() -> Self {
        Self {
            state: AutomationRunState::Admitted,
            generation: 1,
        }
    }
    /// Applies an allowed transition only when `generation` still owns the run.
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
    /// Checks that a caller's generation is still current.
    pub fn fence(&self, generation: u64) -> Result<(), RuntimeError> {
        (generation == self.generation)
            .then_some(())
            .ok_or(RuntimeError::StaleGeneration)
    }
    /// Advances ownership generation and returns the new value.
    pub fn takeover(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }
}

impl Default for RunStateMachine {
    fn default() -> Self {
        Self::new()
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

/// Bounded command queue with progress entries coalesced by run and activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationQueue<T> {
    commands: VecDeque<T>,
    progress: HashMap<(String, String), T>,
}

impl<T: Clone> AutomationQueue<T> {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self {
            commands: VecDeque::new(),
            progress: HashMap::new(),
        }
    }
    /// Appends a command unless the command queue has reached its bound.
    pub fn push_command(&mut self, command: T) -> Result<(), RuntimeError> {
        if self.commands.len() >= MAX_PENDING_COMMANDS {
            return Err(RuntimeError::QueueFull);
        }
        self.commands.push_back(command);
        Ok(())
    }
    /// Inserts or replaces progress for the same `(run_id, activity_id)` pair.
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
    /// Removes the oldest queued command.
    pub fn pop_command(&mut self) -> Option<T> {
        self.commands.pop_front()
    }
    /// Returns the number of queued commands.
    pub fn command_len(&self) -> usize {
        self.commands.len()
    }
    /// Returns the number of distinct retained progress entries.
    pub fn progress_len(&self) -> usize {
        self.progress.len()
    }
}

impl<T: Clone> Default for AutomationQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Time-bounded ownership record for one automation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// Worker or process holding the lease.
    pub owner: String,
    /// Generation that fences stale lease holders.
    pub generation: u64,
    /// Absolute lease expiry time in milliseconds.
    pub expires_at_ms: i64,
}

impl Lease {
    /// Acquires a lease only if there is no currently live lease.
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
    /// Extends a live lease when owner and generation still match.
    pub fn renew(&mut self, owner: &str, generation: u64, now_ms: i64) -> Result<(), RuntimeError> {
        if self.owner != owner || self.generation != generation || self.expires_at_ms <= now_ms {
            return Err(RuntimeError::StaleGeneration);
        }
        self.expires_at_ms = now_ms + LEASE_TTL_MS;
        Ok(())
    }
    /// Creates the next generation after the current lease has expired.
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

/// Exclusive lock preventing overlapping provider operations.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OperationLock {
    operation_id: Option<String>,
}
impl OperationLock {
    /// Acquires the lock for an operation if it is currently free.
    pub fn acquire(&mut self, operation_id: &str) -> Result<(), RuntimeError> {
        if self.operation_id.is_some() {
            return Err(RuntimeError::OperationLocked);
        }
        self.operation_id = Some(operation_id.into());
        Ok(())
    }
    /// Releases the lock only for its owning operation identifier.
    pub fn release(&mut self, operation_id: &str) -> Result<(), RuntimeError> {
        if self.operation_id.as_deref() != Some(operation_id) {
            return Err(RuntimeError::StaleGeneration);
        }
        self.operation_id = None;
        Ok(())
    }
}

/// Authority snapshots that must still match before performing a side effect.
pub struct EffectRevalidation<'a> {
    /// Scope bound to the effect owner.
    pub owner_scope: &'a str,
    /// Scope expected by the current request.
    pub expected_scope: &'a str,
    /// Capability digest captured when the effect was prepared.
    pub capability_hash: &'a str,
    /// Capability digest required at execution time.
    pub expected_capability_hash: &'a str,
    /// Policy snapshot captured when the effect was prepared.
    pub policy_snapshot: &'a str,
    /// Policy snapshot required at execution time.
    pub expected_policy_snapshot: &'a str,
    /// Approval snapshot captured when the effect was prepared.
    pub approval_snapshot: &'a str,
    /// Approval snapshot required at execution time.
    pub expected_approval_snapshot: &'a str,
}

/// Fails closed unless owner, capability, policy, and approval snapshots match.
pub fn revalidate_effect(effect: EffectRevalidation<'_>) -> Result<(), RuntimeError> {
    (effect.owner_scope == effect.expected_scope
        && effect.capability_hash == effect.expected_capability_hash
        && effect.policy_snapshot == effect.expected_policy_snapshot
        && effect.approval_snapshot == effect.expected_approval_snapshot)
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
        assert!(revalidate_effect(EffectRevalidation {
            owner_scope: "o",
            expected_scope: "o",
            capability_hash: "c",
            expected_capability_hash: "c",
            policy_snapshot: "p",
            expected_policy_snapshot: "p",
            approval_snapshot: "a",
            expected_approval_snapshot: "a",
        })
        .is_ok());
        assert_eq!(
            revalidate_effect(EffectRevalidation {
                owner_scope: "o",
                expected_scope: "x",
                capability_hash: "c",
                expected_capability_hash: "c",
                policy_snapshot: "p",
                expected_policy_snapshot: "p",
                approval_snapshot: "a",
                expected_approval_snapshot: "a",
            }),
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
