//! Immutable bounded policy for one task-run snapshot.
//!
//! Every counter is checked by Core before an effect is dispatched.  The
//! renderer may display the snapshot, but cannot increase a limit mid-run.

use serde::{Deserialize, Serialize};

/// Upper bound accepted for the iteration budget.
pub const MAX_ITERATIONS: u32 = 10_000;
/// Upper bound accepted for the tool-call budget.
pub const MAX_TOOL_CALLS: u64 = 100_000;
/// Upper bound accepted for the token budget.
pub const MAX_TOKENS: u64 = 10_000_000;
/// Upper bound accepted for the cost budget in micro-units.
pub const MAX_COST_MICROS: u64 = 100_000_000;

/// Immutable per-run limits checked before dispatching effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPolicy {
    /// Maximum number of agent iterations.
    pub max_iterations: u32,
    /// Maximum elapsed wall-clock time in milliseconds.
    pub max_wall_clock_ms: u64,
    /// Maximum number of dispatched tool calls.
    pub max_tool_calls: u64,
    /// Maximum number of model tokens consumed.
    pub max_tokens: u64,
    /// Maximum model cost in micro-units.
    pub max_cost_micros: u64,
    /// Whether effects require approval before dispatch.
    pub approval_required: bool,
}

/// Resource usage accumulated by one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunUsage {
    /// Number of iterations used.
    pub iterations: u32,
    /// Elapsed wall-clock time in milliseconds.
    pub wall_clock_ms: u64,
    /// Number of tool calls dispatched.
    pub tool_calls: u64,
    /// Number of model tokens consumed.
    pub tokens: u64,
    /// Model cost accumulated in micro-units.
    pub cost_micros: u64,
}

/// Terminal or gating condition reported for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStopReason {
    /// The run completed its requested work.
    Completed,
    /// The run is waiting for required user approval.
    ApprovalRequired,
    /// The run was cancelled.
    Cancelled,
    /// At least one configured resource limit was exceeded.
    BudgetExceeded,
    /// The run's authorized scope no longer matches.
    ScopeDrift,
    /// The selected model provider is unavailable.
    ProviderUnavailable,
}

impl RunPolicy {
    /// Checks that all configured limits are valid and within implementation bounds.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_iterations == 0 || self.max_iterations > MAX_ITERATIONS {
            return Err("max_iterations out of bounds");
        }
        if self.max_wall_clock_ms == 0 {
            return Err("max_wall_clock_ms must be positive");
        }
        if self.max_tool_calls > MAX_TOOL_CALLS {
            return Err("max_tool_calls out of bounds");
        }
        if self.max_tokens > MAX_TOKENS {
            return Err("max_tokens out of bounds");
        }
        if self.max_cost_micros > MAX_COST_MICROS {
            return Err("max_cost_micros out of bounds");
        }
        Ok(())
    }

    /// Rejects usage that exceeds any configured per-run limit.
    pub fn check(&self, usage: RunUsage) -> Result<(), RunStopReason> {
        if usage.iterations > self.max_iterations
            || usage.wall_clock_ms > self.max_wall_clock_ms
            || usage.tool_calls > self.max_tool_calls
            || usage.tokens > self.max_tokens
            || usage.cost_micros > self.max_cost_micros
        {
            return Err(RunStopReason::BudgetExceeded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RunPolicy {
        RunPolicy {
            max_iterations: 2,
            max_wall_clock_ms: 100,
            max_tool_calls: 3,
            max_tokens: 10,
            max_cost_micros: 20,
            approval_required: false,
        }
    }

    #[test]
    fn policy_is_immutable_and_fails_closed_on_any_budget() {
        let policy = policy();
        assert!(policy.validate().is_ok());
        assert!(policy
            .check(RunUsage {
                tool_calls: 4,
                ..RunUsage::default()
            })
            .is_err());
        assert!(policy
            .check(RunUsage {
                tokens: 11,
                ..RunUsage::default()
            })
            .is_err());
        assert!(policy
            .check(RunUsage {
                cost_micros: 21,
                ..RunUsage::default()
            })
            .is_err());
    }
}
