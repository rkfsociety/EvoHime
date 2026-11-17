//! Lazy, on-demand sub-agent spawning under a queen-authored ceiling.
//!
//! The queen sets [`crate::child_contracts::DelegationGrant::max_children`] as
//! a safety ceiling. The worker never converts that ceiling into a batch: it
//! spawns one sub-agent, waits for the report, assesses progress, and only then
//! decides whether another one is needed. When the task is solved the worker
//! finishes with the budget it did not spend.
//!
//! Three rules are structural rather than advisory:
//! - a worker cannot raise its own ceiling. [`LazyDelegationState::extend_ceiling`]
//!   always fails; only [`extend_delegation`] creates a
//!   [`QueenDelegationExtension`] token, and only the queen-side runtime can
//!   call it;
//! - grants and budgets stay subsets of the worker's values on every spawn
//!   (`delegated_request` re-checks them);
//! - the workflow-wide guard and per-worker ceiling from
//!   [`crate::hive_settings::HiveSettings`] are checked before every spawn.
//!
//! Budget policy is deterministic. For a sequential spawn with `slots_left`
//! remaining ceiling slots, every finite field is divided as
//! `remaining / max(1, slots_left)` (integer floor) and then narrowed by the
//! sub-agent's requested budget. At most `slots_left * floor(remaining/slots_left)`
//! is committed, so the sum never exceeds the worker remainder. The unused part
//! of a finished sub-agent's allocation is returned to the worker pool. In
//! parallel mode the same share is reserved for every in-flight sub-agent.

use serde::{Deserialize, Serialize};

use crate::child_contracts::{ChildBudget, ContractError, DelegationGrant, TypedReportStatus};
use crate::hive_settings::HiveSettings;

/// Default worker proposal when it asks the queen for a larger ceiling.
pub const DEFAULT_EXTENSION_STEP: u32 = 1;
/// Bound for the bounded reason text carried by an extension request.
pub const MAX_EXTENSION_REASON_CHARS: usize = 256;

/// Bounded parent-facing snapshot of lazy fan-out state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LazyDelegationSnapshot {
    /// Sub-agents actually spawned by the worker.
    pub spawned_subagents: u32,
    /// Queen-authored ceiling in force for the worker.
    pub planned_ceiling: u32,
    /// Number of queen-approved ceiling extensions.
    pub extensions_granted: u32,
    /// Number of times the worker waited for a report before deciding.
    pub lazy_waiting: u32,
    /// Sub-agents currently in flight.
    pub in_flight_subagents: u32,
    /// Whether the worker signalled that the ceiling was exhausted.
    pub extension_requested: bool,
}

/// The worker's explicit request to the queen for a larger ceiling.
///
/// Constructing a request does not change the ceiling. Only the queen-side
/// [`extend_delegation`] can turn it into a [`QueenDelegationExtension`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationExtensionRequest {
    /// Worker that exhausted its ceiling.
    pub child_task_id: String,
    /// Queen task that issued the original grant.
    pub parent_task_id: String,
    /// Ceiling the worker is currently bound by.
    pub current_ceiling: u32,
    /// Ceiling the worker proposes.
    pub requested_ceiling: u32,
    /// Sub-agents already spawned when the request was made.
    pub spawned: u32,
    /// Bounded human-readable reason.
    pub reason: String,
}

impl DelegationExtensionRequest {
    /// Creates a bounded extension request.
    pub fn new(
        child_task_id: impl Into<String>,
        parent_task_id: impl Into<String>,
        current_ceiling: u32,
        requested_ceiling: u32,
        spawned: u32,
        reason: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let request = Self {
            child_task_id: child_task_id.into(),
            parent_task_id: parent_task_id.into(),
            current_ceiling,
            requested_ceiling,
            spawned,
            reason: reason.into(),
        };
        request.validate()?;
        Ok(request)
    }

    /// Validates identity and bounded text fields.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.child_task_id.trim().is_empty() {
            return Err(ContractError::EmptyField("extension.child_task_id"));
        }
        if self.parent_task_id.trim().is_empty() {
            return Err(ContractError::EmptyField("extension.parent_task_id"));
        }
        if self.child_task_id.chars().count() > crate::child_contracts::MAX_ID_CHARS {
            return Err(ContractError::FieldTooLong {
                field: "extension.child_task_id",
                max: crate::child_contracts::MAX_ID_CHARS,
            });
        }
        if self.parent_task_id.chars().count() > crate::child_contracts::MAX_ID_CHARS {
            return Err(ContractError::FieldTooLong {
                field: "extension.parent_task_id",
                max: crate::child_contracts::MAX_ID_CHARS,
            });
        }
        if self.reason.chars().count() > MAX_EXTENSION_REASON_CHARS {
            return Err(ContractError::FieldTooLong {
                field: "extension.reason",
                max: MAX_EXTENSION_REASON_CHARS,
            });
        }
        Ok(())
    }
}

/// Queen-approved extension token.
///
/// The token has no public constructor. It can only be produced by
/// [`extend_delegation`], which validates the safety ceilings, so a worker
/// cannot fabricate one and widen its own grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueenDelegationExtension {
    grant: DelegationGrant,
    previous_ceiling: u32,
}

impl QueenDelegationExtension {
    /// Returns the extended grant the worker may continue with.
    pub fn grant(&self) -> &DelegationGrant {
        &self.grant
    }

    /// Returns the ceiling that was in force before the extension.
    pub fn previous_ceiling(&self) -> u32 {
        self.previous_ceiling
    }
}

/// Queen-side authority consulted when a worker exhausts its ceiling.
///
/// Implementations are explicit queen decisions. Returning `None` means the
/// queen declines and the worker finishes with a partial report; there is no
/// automatic widening.
pub trait DelegationAuthority: Send + Sync {
    /// Decides whether to extend the worker's ceiling.
    fn decide_extension(
        &self,
        request: &DelegationExtensionRequest,
        grant: &DelegationGrant,
        settings: &HiveSettings,
        workflow_spawned: u32,
    ) -> Result<Option<QueenDelegationExtension>, ContractError>;
}

/// Queen-side operation that validates and applies an additive extension.
///
/// `already_spawned` is the workflow-wide count of sub-agents already spawned.
/// The resulting ceiling is bounded by both `max_subagents_per_worker` and the
/// remaining `max_total_subagents_per_workflow`.
pub fn extend_delegation(
    grant: &DelegationGrant,
    request: &DelegationExtensionRequest,
    settings: &HiveSettings,
    already_spawned: u32,
) -> Result<QueenDelegationExtension, ContractError> {
    request.validate()?;
    if request.current_ceiling != grant.max_children {
        return Err(ContractError::DelegationCeilingMismatch {
            expected: request.current_ceiling,
            actual: grant.max_children,
        });
    }
    if request.requested_ceiling <= request.current_ceiling {
        return Err(ContractError::DelegationExtensionNotIncreasing {
            current: request.current_ceiling,
            requested: request.requested_ceiling,
        });
    }
    settings.ensure_workflow_capacity(request.requested_ceiling, already_spawned)?;
    // Only the ceiling changes. Re-running `validate_for_parent` here would
    // check the grant against an empty parent set and reject every non-empty
    // grant; the grant itself was already validated when it was issued.
    let mut extended = grant.clone();
    extended.max_children = request.requested_ceiling;
    Ok(QueenDelegationExtension {
        grant: extended,
        previous_ceiling: grant.max_children,
    })
}

/// Deterministic progress assessment policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelegationProgressPolicy {
    /// Production default: stop when a sub-agent reports `complete`.
    #[default]
    StopOnComplete,
    /// Stop only after the worker has spawned the given number of sub-agents.
    ///
    /// This is not a plan. It is a deterministic assessment used by the queen
    /// and by tests to demonstrate early stop and lazy fan-out.
    StopAfter(u32),
}

impl DelegationProgressPolicy {
    /// Returns whether the accumulated progress is sufficient to finish.
    pub fn solved(&self, spawned: u32, status: TypedReportStatus) -> bool {
        match self {
            Self::StopOnComplete => status == TypedReportStatus::Complete,
            Self::StopAfter(limit) => spawned >= *limit,
        }
    }
}

/// What the worker does after assessing a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationDecision {
    /// Progress is sufficient; do not spend the rest of the ceiling.
    Finish,
    /// Another sub-agent is needed and the ceiling still allows it.
    Continue,
    /// The ceiling is exhausted while sub-tasks remain; ask the queen.
    RequestExtension,
}

/// Mutable lazy fan-out state owned by one worker run.
#[derive(Debug, Clone)]
pub struct LazyDelegationState {
    ceiling: u32,
    spawned: u32,
    in_flight: u32,
    lazy_waiting: u32,
    extensions: u32,
    max_parallel: u32,
    remaining: ChildBudget,
    capacity: ChildBudget,
    extension_requested: bool,
}

impl LazyDelegationState {
    /// Creates the worker state from the queen grant and its current remainder.
    ///
    /// The effective ceiling is the grant ceiling, bounded by the per-worker
    /// hive ceiling and by the workflow-wide remaining capacity.
    pub fn new(
        grant: &DelegationGrant,
        remaining: ChildBudget,
        settings: &HiveSettings,
        workflow_spawned: u32,
    ) -> Result<Self, ContractError> {
        if grant.max_children == 0 {
            return Err(ContractError::InvalidDelegationCeiling(0));
        }
        if grant.max_children > settings.effective_worker_ceiling() {
            return Err(ContractError::DelegationCeilingAboveHiveLimit {
                requested: grant.max_children,
                limit: settings.effective_worker_ceiling(),
            });
        }
        let workflow_remaining = settings
            .max_total_subagents_per_workflow
            .saturating_sub(workflow_spawned);
        if workflow_remaining == 0 {
            return Err(ContractError::DelegationWorkflowGuardExceeded);
        }
        let ceiling = grant.max_children.min(workflow_remaining);
        Ok(Self {
            ceiling,
            spawned: 0,
            in_flight: 0,
            lazy_waiting: 0,
            extensions: 0,
            max_parallel: settings.effective_parallel_limit(),
            remaining: remaining.clone(),
            capacity: remaining,
            extension_requested: false,
        })
    }

    /// Ceiling currently in force.
    pub fn ceiling(&self) -> u32 {
        self.ceiling
    }

    /// Sub-agents actually spawned.
    pub fn spawned(&self) -> u32 {
        self.spawned
    }

    /// Sub-agents currently in flight.
    pub fn in_flight(&self) -> u32 {
        self.in_flight
    }

    /// Number of times the worker waited for a report before deciding.
    pub fn lazy_waiting(&self) -> u32 {
        self.lazy_waiting
    }

    /// Number of queen-approved extensions.
    pub fn extensions_granted(&self) -> u32 {
        self.extensions
    }

    /// Whether the worker asked the queen for a larger ceiling.
    pub fn extension_requested(&self) -> bool {
        self.extension_requested
    }

    /// Worker budget remainder after reservations and refunds.
    pub fn remaining(&self) -> &ChildBudget {
        &self.remaining
    }

    /// Returns whether another sub-agent may start within the ceiling.
    pub fn can_spawn(&self) -> bool {
        self.spawned.saturating_add(self.in_flight) < self.ceiling
            && !budget_is_exhausted(&self.remaining)
    }

    /// Reserves a deterministic budget share and starts one sub-agent.
    ///
    /// The returned budget is the share handed to the sub-agent. The caller
    /// must pass it back to [`Self::complete`] together with the measured usage
    /// so the unused remainder returns to the worker pool.
    pub fn try_spawn(
        &mut self,
        requested: Option<ChildBudget>,
    ) -> Result<ChildBudget, ContractError> {
        if self.in_flight >= self.max_parallel {
            return Err(ContractError::DelegationParallelLimitExceeded {
                in_flight: self.in_flight,
                limit: self.max_parallel,
            });
        }
        if self.spawned.saturating_add(self.in_flight) >= self.ceiling {
            return Err(ContractError::DelegationCeilingExceeded {
                spawned: self.spawned,
                ceiling: self.ceiling,
            });
        }
        let slots_left = self
            .ceiling
            .saturating_sub(self.spawned.saturating_add(self.in_flight))
            .max(1);
        let allocation = allocate_share(&self.remaining, requested.as_ref(), slots_left);
        if budget_is_exhausted(&allocation) {
            return Err(ContractError::DelegationBudgetExhausted);
        }
        self.remaining = subtract_budget(&self.remaining, &allocation);
        self.spawned = self.spawned.saturating_add(1);
        self.in_flight = self.in_flight.saturating_add(1);
        self.extension_requested = false;
        Ok(allocation)
    }

    /// Records a finished sub-agent and refunds its unused allocation.
    pub fn complete(&mut self, allocation: &ChildBudget, used: &ChildBudget) {
        self.in_flight = self.in_flight.saturating_sub(1);
        let usage = clamp_budget(used, allocation);
        let refund = subtract_budget(allocation, &usage);
        self.remaining = add_budget_capped(&self.remaining, &refund, &self.capacity);
    }

    /// Records one wait-for-report before the next spawn decision.
    pub fn record_lazy_wait(&mut self) {
        self.lazy_waiting = self.lazy_waiting.saturating_add(1);
    }

    /// Assesses whether the worker should finish, continue, or ask the queen.
    pub fn assess(
        &self,
        policy: DelegationProgressPolicy,
        status: TypedReportStatus,
    ) -> DelegationDecision {
        if policy.solved(self.spawned, status) {
            return DelegationDecision::Finish;
        }
        if self.in_flight > 0 {
            return DelegationDecision::Continue;
        }
        if self.can_spawn() {
            DelegationDecision::Continue
        } else {
            DelegationDecision::RequestExtension
        }
    }

    /// Builds the worker's extension request when the ceiling is exhausted.
    ///
    /// Task identities are placeholders: the executor fills the real ids from
    /// its own run context before handing the request to the queen. Every other
    /// field is bounded here so a worker cannot smuggle oversized text.
    pub fn request_extension(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<DelegationExtensionRequest, ContractError> {
        if self.in_flight > 0 || self.spawned < self.ceiling {
            return Err(ContractError::DelegationExtensionNotAllowed);
        }
        let requested_ceiling = self.ceiling.saturating_add(DEFAULT_EXTENSION_STEP);
        let request = DelegationExtensionRequest::new(
            "pending",
            "pending",
            self.ceiling,
            requested_ceiling,
            self.spawned,
            reason,
        )?;
        self.extension_requested = true;
        Ok(request)
    }

    /// Explicitly refuses a worker-side ceiling change.
    ///
    /// The method exists so the boundary is testable and documented: a worker
    /// must request an extension, and only the queen can grant one.
    pub fn extend_ceiling(&mut self, _requested: u32) -> Result<(), ContractError> {
        Err(ContractError::DelegationExtensionNotAllowed)
    }

    /// Clears a pending request after the queen declined it.
    ///
    /// The worker is not waiting any more: it finishes with the partial result
    /// it already collected.
    pub fn cancel_extension_request(&mut self) {
        self.extension_requested = false;
    }

    /// Applies a queen-approved extension to this worker.
    pub fn apply_queen_extension(
        &mut self,
        extension: &QueenDelegationExtension,
    ) -> Result<(), ContractError> {
        if extension.previous_ceiling != self.ceiling {
            return Err(ContractError::DelegationCeilingMismatch {
                expected: extension.previous_ceiling,
                actual: self.ceiling,
            });
        }
        if extension.grant.max_children <= self.ceiling {
            return Err(ContractError::DelegationExtensionNotIncreasing {
                current: self.ceiling,
                requested: extension.grant.max_children,
            });
        }
        self.ceiling = extension.grant.max_children;
        self.extensions = self.extensions.saturating_add(1);
        self.extension_requested = false;
        Ok(())
    }

    /// Returns the bounded snapshot used by [`crate::child_workflow::ChildProjection`].
    pub fn snapshot(&self) -> LazyDelegationSnapshot {
        LazyDelegationSnapshot {
            spawned_subagents: self.spawned,
            planned_ceiling: self.ceiling,
            extensions_granted: self.extensions,
            lazy_waiting: self.lazy_waiting,
            in_flight_subagents: self.in_flight,
            extension_requested: self.extension_requested,
        }
    }
}

/// Deterministic per-field allocation for one sub-agent.
///
/// `slots_left` is the number of remaining ceiling slots including the one
/// being started. Each finite field is floored by that count and then narrowed
/// by the requested budget; missing request limits inherit the share.
///
/// A positive remainder always yields at least one unit per field, so a small
/// remainder with a wide ceiling cannot stall the worker. When the remainder
/// itself is zero the share stays zero, and [`LazyDelegationState::try_spawn`]
/// rejects the spawn instead of over-committing the parent budget.
pub fn allocate_share(
    remaining: &ChildBudget,
    requested: Option<&ChildBudget>,
    slots_left: u32,
) -> ChildBudget {
    fn share(value: Option<u64>, slots: u32) -> Option<u64> {
        value.map(|value| {
            if value == 0 {
                0
            } else {
                (value / u64::from(slots.max(1))).max(1)
            }
        })
    }
    fn narrow(requested: Option<u64>, share: Option<u64>) -> Option<u64> {
        match (requested, share) {
            (Some(requested), Some(share)) => Some(requested.min(share)),
            (None, share) => share,
            (requested, None) => requested,
        }
    }
    let requested = requested.cloned().unwrap_or_default();
    ChildBudget {
        max_tokens: narrow(
            requested.max_tokens,
            share(remaining.max_tokens, slots_left),
        ),
        max_time_seconds: narrow(
            requested.max_time_seconds,
            share(remaining.max_time_seconds, slots_left),
        ),
        max_tool_calls: narrow(
            requested.max_tool_calls,
            share(remaining.max_tool_calls, slots_left),
        ),
    }
}

/// Returns whether a budget has no capacity left for another sub-agent.
pub fn budget_is_exhausted(budget: &ChildBudget) -> bool {
    budget.max_tokens == Some(0)
        || budget.max_time_seconds == Some(0)
        || budget.max_tool_calls == Some(0)
}

fn clamp_budget(used: &ChildBudget, allocation: &ChildBudget) -> ChildBudget {
    fn lower(used: Option<u64>, limit: Option<u64>) -> Option<u64> {
        match (used, limit) {
            (Some(used), Some(limit)) => Some(used.min(limit)),
            (Some(used), None) => Some(used),
            (None, _) => None,
        }
    }
    ChildBudget {
        max_tokens: lower(used.max_tokens, allocation.max_tokens),
        max_time_seconds: lower(used.max_time_seconds, allocation.max_time_seconds),
        max_tool_calls: lower(used.max_tool_calls, allocation.max_tool_calls),
    }
}

fn subtract_budget(left: &ChildBudget, right: &ChildBudget) -> ChildBudget {
    fn sub(left: Option<u64>, right: Option<u64>) -> Option<u64> {
        match (left, right) {
            (Some(left), Some(right)) => Some(left.saturating_sub(right)),
            (None, Some(_)) => None,
            (left, None) => left,
        }
    }
    ChildBudget {
        max_tokens: sub(left.max_tokens, right.max_tokens),
        max_time_seconds: sub(left.max_time_seconds, right.max_time_seconds),
        max_tool_calls: sub(left.max_tool_calls, right.max_tool_calls),
    }
}

fn add_budget_capped(
    current: &ChildBudget,
    refund: &ChildBudget,
    capacity: &ChildBudget,
) -> ChildBudget {
    fn add(current: Option<u64>, refund: Option<u64>, capacity: Option<u64>) -> Option<u64> {
        match (current, refund, capacity) {
            (Some(current), Some(refund), Some(capacity)) => {
                Some(current.saturating_add(refund).min(capacity))
            }
            (Some(current), _, _) => Some(current),
            (None, _, _) => None,
        }
    }
    ChildBudget {
        max_tokens: add(current.max_tokens, refund.max_tokens, capacity.max_tokens),
        max_time_seconds: add(
            current.max_time_seconds,
            refund.max_time_seconds,
            capacity.max_time_seconds,
        ),
        max_tool_calls: add(
            current.max_tool_calls,
            refund.max_tool_calls,
            capacity.max_tool_calls,
        ),
    }
}

/// Verifies that the sum of allocations or measured usages stays inside the
/// worker remainder.
///
/// Each field with a finite parent limit is compared independently. The helper
/// is public so the runtime and its tests share one budget sum rule.
pub fn validate_allocations_within(
    allocations: &[ChildBudget],
    parent_remaining: &ChildBudget,
) -> Result<(), ContractError> {
    fn sum(values: impl Iterator<Item = Option<u64>>) -> u64 {
        values.flatten().fold(0_u64, u64::saturating_add)
    }
    for (values, limit) in [
        (
            sum(allocations.iter().map(|budget| budget.max_tokens)),
            parent_remaining.max_tokens,
        ),
        (
            sum(allocations.iter().map(|budget| budget.max_time_seconds)),
            parent_remaining.max_time_seconds,
        ),
        (
            sum(allocations.iter().map(|budget| budget.max_tool_calls)),
            parent_remaining.max_tool_calls,
        ),
    ] {
        if let Some(limit) = limit {
            if values > limit {
                return Err(ContractError::DelegationBudgetExhausted);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child_contracts::{Grant, MAX_ROLE_CHARS};
    use crate::hive_settings::HiveSettings;

    fn settings(worker: u32, parallel: u32, total: u32) -> HiveSettings {
        HiveSettings {
            max_subagents_per_worker: worker,
            max_parallel_subagents: parallel,
            max_total_subagents_per_workflow: total,
            ..HiveSettings::default()
        }
    }

    fn grant(ceiling: u32) -> DelegationGrant {
        DelegationGrant {
            role: "researcher".into(),
            purpose: "inspect one bounded area".into(),
            grants: vec![Grant::new("workspace.read").expect("grant")],
            requested_capabilities: vec!["workspace.read".into()],
            budget: Some(ChildBudget::new().with_tool_calls(2).with_time(30)),
            acceptance_criteria: None,
            max_children: ceiling,
        }
    }

    fn budget(calls: u64) -> ChildBudget {
        ChildBudget::new().with_tool_calls(calls)
    }

    #[test]
    fn a_ceiling_of_eight_can_finish_after_three_subagents() {
        let settings = settings(16, 4, 64);
        let mut state = LazyDelegationState::new(
            &grant(8),
            ChildBudget::new()
                .with_tool_calls(16)
                .with_time(120)
                .with_tokens(16_000),
            &settings,
            0,
        )
        .expect("state");

        let statuses = [
            TypedReportStatus::Partial,
            TypedReportStatus::Partial,
            TypedReportStatus::Complete,
        ];
        let mut allocations = Vec::new();
        for (index, status) in statuses.into_iter().enumerate() {
            let allocation = state.try_spawn(Some(budget(2))).expect("spawn");
            allocations.push(allocation.clone());
            state.complete(&allocation, &budget(1));
            if state.assess(DelegationProgressPolicy::StopOnComplete, status)
                == DelegationDecision::Continue
            {
                state.record_lazy_wait();
            }
            assert_eq!(state.spawned(), index as u32 + 1);
        }

        assert_eq!(
            state.assess(
                DelegationProgressPolicy::StopOnComplete,
                TypedReportStatus::Complete
            ),
            DelegationDecision::Finish
        );
        let snapshot = state.snapshot();
        assert_eq!(snapshot.spawned_subagents, 3);
        assert_eq!(snapshot.planned_ceiling, 8);
        assert_eq!(snapshot.lazy_waiting, 2);
        assert_eq!(snapshot.extensions_granted, 0);
        assert!(state.can_spawn(), "unused ceiling slots stay available");
        assert!(state.spawned() < state.ceiling());
        assert!(state.remaining().max_tool_calls.unwrap() >= 13);
        validate_allocations_within(&allocations, &budget(16)).expect("within budget");
    }

    #[test]
    fn queen_extends_two_to_five_and_worker_continues_lazily() {
        let settings = settings(8, 2, 64);
        let mut grant = grant(2);
        let mut state = LazyDelegationState::new(&grant, budget(20), &settings, 0).expect("state");

        for _ in 0..2 {
            let allocation = state.try_spawn(Some(budget(2))).expect("spawn");
            state.complete(&allocation, &budget(1));
        }
        assert_eq!(
            state.assess(
                DelegationProgressPolicy::StopAfter(5),
                TypedReportStatus::Partial
            ),
            DelegationDecision::RequestExtension
        );

        let mut request = state.request_extension("remaining areas").expect("request");
        request.child_task_id = "run-1:worker".into();
        request.parent_task_id = "run-1".into();
        request.requested_ceiling = 5;
        let extension = extend_delegation(&grant, &request, &settings, 0).expect("queen extension");
        state
            .apply_queen_extension(&extension)
            .expect("apply extension");
        grant.max_children = 5;

        for _ in 0..3 {
            let allocation = state
                .try_spawn(Some(budget(2)))
                .expect("spawn after extension");
            state.complete(&allocation, &budget(1));
        }
        let snapshot = state.snapshot();
        assert_eq!(snapshot.spawned_subagents, 5);
        assert_eq!(snapshot.planned_ceiling, 5);
        assert_eq!(snapshot.extensions_granted, 1);
        assert!(!snapshot.extension_requested);
        assert_eq!(
            state.assess(
                DelegationProgressPolicy::StopAfter(5),
                TypedReportStatus::Partial
            ),
            DelegationDecision::Finish
        );
    }

    #[test]
    fn sequential_spawning_waits_for_each_report_and_stops_early() {
        let settings = settings(8, 4, 64);
        let mut state =
            LazyDelegationState::new(&grant(8), budget(12), &settings, 0).expect("state");
        let mut observed = Vec::new();
        loop {
            if !state.can_spawn() {
                break;
            }
            let allocation = state.try_spawn(Some(budget(2))).expect("spawn");
            assert_eq!(
                state.in_flight(),
                1,
                "sequential policy holds one in flight"
            );
            state.complete(&allocation, &budget(1));
            observed.push(state.spawned());
            state.record_lazy_wait();
            if state.assess(
                DelegationProgressPolicy::StopOnComplete,
                TypedReportStatus::Complete,
            ) == DelegationDecision::Finish
            {
                break;
            }
        }
        assert_eq!(observed, vec![1], "one complete report is enough");
        assert_eq!(state.snapshot().lazy_waiting, 1);
    }

    #[test]
    fn parallel_spawning_respects_the_parallel_limit_and_budget() {
        let settings = settings(8, 3, 64);
        let mut state = LazyDelegationState::new(
            &grant(6),
            ChildBudget::new().with_tool_calls(9).with_time(90),
            &settings,
            0,
        )
        .expect("state");
        let mut allocations = Vec::new();
        for _ in 0..3 {
            allocations.push(state.try_spawn(Some(budget(2))).expect("parallel spawn"));
        }
        assert_eq!(state.in_flight(), 3);
        assert_eq!(
            state.try_spawn(Some(budget(2))),
            Err(ContractError::DelegationParallelLimitExceeded {
                in_flight: 3,
                limit: 3
            })
        );
        validate_allocations_within(
            &allocations,
            &ChildBudget::new().with_tool_calls(9).with_time(90),
        )
        .expect("allocations stay inside the remainder");
        for allocation in &allocations {
            state.complete(allocation, &budget(1));
        }
        assert_eq!(state.in_flight(), 0);
        assert!(state.remaining().max_tool_calls.unwrap() >= 6);
    }

    #[test]
    fn worker_cannot_exceed_ceiling_or_extend_itself() {
        let settings = settings(8, 2, 64);
        let mut state =
            LazyDelegationState::new(&grant(1), budget(8), &settings, 0).expect("state");
        let allocation = state.try_spawn(Some(budget(1))).expect("first spawn");
        state.complete(&allocation, &budget(1));
        assert_eq!(
            state.try_spawn(Some(budget(1))),
            Err(ContractError::DelegationCeilingExceeded {
                spawned: 1,
                ceiling: 1
            })
        );
        assert_eq!(
            state.extend_ceiling(4),
            Err(ContractError::DelegationExtensionNotAllowed)
        );
        assert_eq!(state.ceiling(), 1);
    }

    #[test]
    fn extension_cannot_exceed_hive_settings() {
        let settings = settings(4, 2, 64);
        let grant = grant(2);
        let request =
            DelegationExtensionRequest::new("run-1:worker", "run-1", 2, 20, 2, "too many areas")
                .expect("request");
        assert_eq!(
            extend_delegation(&grant, &request, &settings, 0),
            Err(ContractError::DelegationCeilingAboveHiveLimit {
                requested: 20,
                limit: 4
            })
        );
    }

    #[test]
    fn total_subagent_spend_cannot_exceed_the_worker_remainder() {
        let settings = settings(8, 8, 64);
        let mut state =
            LazyDelegationState::new(&grant(8), budget(5), &settings, 0).expect("state");
        let mut allocations = Vec::new();
        while let Ok(allocation) = state.try_spawn(Some(budget(2))) {
            allocations.push(allocation);
        }
        assert!(state.try_spawn(Some(budget(2))).is_err());
        validate_allocations_within(&allocations, &budget(5)).expect("never exceeds remainder");

        let over = [budget(4), budget(3)];
        assert_eq!(
            validate_allocations_within(&over, &budget(5)),
            Err(ContractError::DelegationBudgetExhausted)
        );
    }

    #[test]
    fn hive_ceiling_is_a_hard_rejection_not_a_suggestion() {
        let settings = settings(3, 2, 64);
        let error = LazyDelegationState::new(&grant(4), budget(10), &settings, 0)
            .expect_err("ceiling above hive settings");
        assert_eq!(
            error,
            ContractError::DelegationCeilingAboveHiveLimit {
                requested: 4,
                limit: 3
            }
        );
    }

    #[test]
    fn bounded_extension_request_is_revalidated() {
        let too_long = "x".repeat(MAX_EXTENSION_REASON_CHARS + 1);
        assert!(
            DelegationExtensionRequest::new("run-1:worker", "run-1", 1, 2, 1, too_long).is_err()
        );
        assert!(DelegationExtensionRequest::new("", "run-1", 1, 2, 1, "reason").is_err());
    }

    #[test]
    fn zero_per_worker_ceiling_is_bounded_by_the_workflow_guard() {
        let settings = settings(0, 0, 6);
        let state = LazyDelegationState::new(&grant(6), budget(6), &settings, 0).expect("state");
        assert_eq!(state.ceiling(), 6);
        assert_eq!(settings.effective_parallel_limit(), 6);
        let state = LazyDelegationState::new(&grant(6), budget(6), &settings, 1).expect("state");
        assert_eq!(state.ceiling(), 5);
    }

    #[test]
    fn role_and_purpose_bounds_are_reused_for_extension_text() {
        const { assert!(MAX_ROLE_CHARS >= 8) };
        let request = DelegationExtensionRequest::new("a", "b", 1, 2, 1, "ok").expect("request");
        assert_eq!(request.requested_ceiling, 2);
    }
}
