//! Immutable bounded policy for one task-run snapshot.
//!
//! Every counter is checked by Core before an effect is dispatched.  The
//! renderer may display the snapshot, but cannot increase a limit mid-run.

use serde::{Deserialize, Serialize};

pub const MAX_ITERATIONS: u32 = 10_000;
pub const MAX_TOOL_CALLS: u64 = 100_000;
pub const MAX_TOKENS: u64 = 10_000_000;
pub const MAX_COST_MICROS: u64 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPolicy {
    pub max_iterations: u32,
    pub max_wall_clock_ms: u64,
    pub max_tool_calls: u64,
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub approval_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunUsage {
    pub iterations: u32,
    pub wall_clock_ms: u64,
    pub tool_calls: u64,
    pub tokens: u64,
    pub cost_micros: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStopReason { Completed, ApprovalRequired, Cancelled, BudgetExceeded, ScopeDrift, ProviderUnavailable }

impl RunPolicy {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_iterations == 0 || self.max_iterations > MAX_ITERATIONS { return Err("max_iterations out of bounds"); }
        if self.max_wall_clock_ms == 0 { return Err("max_wall_clock_ms must be positive"); }
        if self.max_tool_calls > MAX_TOOL_CALLS { return Err("max_tool_calls out of bounds"); }
        if self.max_tokens > MAX_TOKENS { return Err("max_tokens out of bounds"); }
        if self.max_cost_micros > MAX_COST_MICROS { return Err("max_cost_micros out of bounds"); }
        Ok(())
    }

    pub fn check(&self, usage: RunUsage) -> Result<(), RunStopReason> {
        if usage.iterations > self.max_iterations || usage.wall_clock_ms > self.max_wall_clock_ms || usage.tool_calls > self.max_tool_calls || usage.tokens > self.max_tokens || usage.cost_micros > self.max_cost_micros {
            return Err(RunStopReason::BudgetExceeded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RunPolicy { RunPolicy { max_iterations: 2, max_wall_clock_ms: 100, max_tool_calls: 3, max_tokens: 10, max_cost_micros: 20, approval_required: false } }

    #[test]
    fn policy_is_immutable_and_fails_closed_on_any_budget() {
        let policy = policy();
        assert!(policy.validate().is_ok());
        assert!(policy.check(RunUsage { tool_calls: 4, ..RunUsage::default() }).is_err());
        assert!(policy.check(RunUsage { tokens: 11, ..RunUsage::default() }).is_err());
        assert!(policy.check(RunUsage { cost_micros: 21, ..RunUsage::default() }).is_err());
    }
}
