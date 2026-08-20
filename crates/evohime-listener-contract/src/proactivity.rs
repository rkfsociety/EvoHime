use crate::{error::ContractError, policy::AmbientPolicy};
use serde::{Deserialize, Serialize};

pub const MAX_PROPOSALS_PER_HOUR: u32 = 3;
pub const MAX_PROPOSALS_PER_DAY: u32 = 10;
pub const MIN_PROPOSAL_INTERVAL_MS: u64 = 10 * 60 * 1000;

const HARD_MAX_PER_HOUR: u32 = 12;
const HARD_MAX_PER_DAY: u32 = 48;

/// Immutable snapshot of the proactivity ceiling, by the same rule as
/// `evohime_core::run_policy::RunPolicy`: the renderer may display it, nothing
/// may raise it at runtime.
///
/// Current counters are deliberately **not** part of the snapshot — they live
/// in `AmbientProactivityRegistry` inside Core and are passed in as
/// [`ProactivityCounters`], so this type stays a pure bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProactivityBudget {
    pub max_per_hour: u32,
    pub max_per_day: u32,
    pub min_interval_ms: u64,
}

/// Counters owned by Core and handed to the budget for a decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProactivityCounters {
    pub hour_count: u32,
    pub day_count: u32,
    /// Unix millis of the last proposal shown, if any.
    pub last_proposed_at_ms: Option<u64>,
}

/// Why a proposal was dropped.  A proposal over the ceiling is discarded with
/// a counter, never queued: an hour of silence must not turn into ten cards.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProactivityDenial {
    Paused,
    QuietHours,
    HourlyCapReached,
    DailyCapReached,
    TooSoon { retry_after_ms: u64 },
}

impl ProactivityBudget {
    pub const DEFAULT: ProactivityBudget = ProactivityBudget {
        max_per_hour: MAX_PROPOSALS_PER_HOUR,
        max_per_day: MAX_PROPOSALS_PER_DAY,
        min_interval_ms: MIN_PROPOSAL_INTERVAL_MS,
    };

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.max_per_hour == 0 || self.max_per_hour > HARD_MAX_PER_HOUR {
            return Err(ContractError::BudgetOutOfBounds("max_per_hour"));
        }
        if self.max_per_day < self.max_per_hour || self.max_per_day > HARD_MAX_PER_DAY {
            return Err(ContractError::BudgetOutOfBounds("max_per_day"));
        }
        if self.min_interval_ms == 0 {
            return Err(ContractError::BudgetOutOfBounds("min_interval_ms"));
        }
        Ok(())
    }

    /// Budget half of the decision: caps and spacing only.
    pub fn check(
        &self,
        counters: ProactivityCounters,
        now_ms: u64,
    ) -> Result<(), ProactivityDenial> {
        if counters.hour_count >= self.max_per_hour {
            return Err(ProactivityDenial::HourlyCapReached);
        }
        if counters.day_count >= self.max_per_day {
            return Err(ProactivityDenial::DailyCapReached);
        }
        if let Some(last) = counters.last_proposed_at_ms {
            // A clock that moved backwards must not unlock the interval.
            let elapsed = now_ms.saturating_sub(last);
            if now_ms < last || elapsed < self.min_interval_ms {
                return Err(ProactivityDenial::TooSoon {
                    retry_after_ms: self.min_interval_ms.saturating_sub(elapsed),
                });
            }
        }
        Ok(())
    }

    /// Full decision: policy first (pause and quiet hours suppress a proposal
    /// outright), then the budget.
    pub fn decide(
        &self,
        policy: &AmbientPolicy,
        counters: ProactivityCounters,
        now_ms: u64,
        minute_of_day: u32,
    ) -> Result<(), ProactivityDenial> {
        if policy.paused {
            return Err(ProactivityDenial::Paused);
        }
        if policy.is_quiet_at(minute_of_day) {
            return Err(ProactivityDenial::QuietHours);
        }
        self.check(counters, now_ms)
    }
}

impl Default for ProactivityBudget {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::QuietHours;

    const HOUR_MS: u64 = 3_600_000;

    #[test]
    fn documented_ceiling_is_three_per_hour_ten_per_day_ten_minutes_apart() {
        let budget = ProactivityBudget::DEFAULT;
        assert_eq!(budget.max_per_hour, 3);
        assert_eq!(budget.max_per_day, 10);
        assert_eq!(budget.min_interval_ms, 600_000);
        assert_eq!(budget.validate(), Ok(()));
        assert_eq!(
            budget.check(ProactivityCounters::default(), HOUR_MS),
            Ok(())
        );
    }

    #[test]
    fn caps_and_spacing_are_enforced() {
        let budget = ProactivityBudget::DEFAULT;
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 3,
                    day_count: 3,
                    last_proposed_at_ms: None,
                },
                HOUR_MS
            ),
            Err(ProactivityDenial::HourlyCapReached)
        );
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 0,
                    day_count: 10,
                    last_proposed_at_ms: None,
                },
                HOUR_MS
            ),
            Err(ProactivityDenial::DailyCapReached)
        );
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 1,
                    day_count: 1,
                    last_proposed_at_ms: Some(HOUR_MS),
                },
                HOUR_MS + 599_999
            ),
            Err(ProactivityDenial::TooSoon { retry_after_ms: 1 })
        );
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 1,
                    day_count: 1,
                    last_proposed_at_ms: Some(HOUR_MS),
                },
                HOUR_MS + 600_000
            ),
            Ok(())
        );
    }

    #[test]
    fn backwards_clock_does_not_unlock_the_interval() {
        let budget = ProactivityBudget::DEFAULT;
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 0,
                    day_count: 0,
                    last_proposed_at_ms: Some(2 * HOUR_MS),
                },
                HOUR_MS
            ),
            Err(ProactivityDenial::TooSoon {
                retry_after_ms: 600_000
            })
        );
    }

    #[test]
    fn pause_and_quiet_hours_suppress_proposals() {
        let budget = ProactivityBudget::DEFAULT;
        let paused = AmbientPolicy {
            paused: true,
            ..AmbientPolicy::default()
        };
        assert_eq!(
            budget.decide(&paused, ProactivityCounters::default(), HOUR_MS, 12 * 60),
            Err(ProactivityDenial::Paused)
        );
        let quiet = AmbientPolicy {
            quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
            ..AmbientPolicy::default()
        };
        assert_eq!(
            budget.decide(&quiet, ProactivityCounters::default(), HOUR_MS, 2 * 60),
            Err(ProactivityDenial::QuietHours)
        );
        assert_eq!(
            budget.decide(&quiet, ProactivityCounters::default(), HOUR_MS, 12 * 60),
            Ok(())
        );
    }

    #[test]
    fn absurd_budgets_are_rejected() {
        for (budget, field) in [
            (
                ProactivityBudget {
                    max_per_hour: 0,
                    ..ProactivityBudget::DEFAULT
                },
                "max_per_hour",
            ),
            (
                ProactivityBudget {
                    max_per_hour: 100,
                    ..ProactivityBudget::DEFAULT
                },
                "max_per_hour",
            ),
            (
                ProactivityBudget {
                    max_per_day: 1,
                    ..ProactivityBudget::DEFAULT
                },
                "max_per_day",
            ),
            (
                ProactivityBudget {
                    min_interval_ms: 0,
                    ..ProactivityBudget::DEFAULT
                },
                "min_interval_ms",
            ),
        ] {
            assert_eq!(
                budget.validate(),
                Err(ContractError::BudgetOutOfBounds(field))
            );
        }
    }
}
