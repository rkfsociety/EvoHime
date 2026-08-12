//! Wires the bounded `scheduler_state` and `schedule_contract` state machines into an
//! in-memory runtime loop that the supervisor drives once per core-process generation
//! and once per health-check tick.
//!
//! This module stays side-effect free like the two contracts it drives: it never
//! touches the filesystem, the clock, or a process handle directly. Callers (the
//! Windows supervisor's tick loop) supply `now_ms` and observed facts (is the core
//! process alive? is its heartbeat fresh?) and get back the bounded state machine's
//! decisions plus a list of events to log. State lives only for the supervisor
//! process's lifetime; there is no persistence backing it yet.

use crate::schedule_contract::{
    BudgetRef, ContractError, FailureDecision, PolicyRefs, RetryPolicy, ScheduleKind,
    ScheduleRecord, TriggerDecision, TriggerKind,
};
use crate::scheduler_state::{
    LifecycleState, RecoveryDecision, RetryDisposition, SchedulerState, ValidationError,
};
use std::time::Duration;

/// Errors that can occur while constructing or driving a [`SupervisorRuntime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    Scheduler(ValidationError),
    Schedule(ContractError),
}

impl From<ValidationError> for RuntimeError {
    fn from(value: ValidationError) -> Self {
        RuntimeError::Scheduler(value)
    }
}

impl From<ContractError> for RuntimeError {
    fn from(value: ContractError) -> Self {
        RuntimeError::Schedule(value)
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RuntimeError {}

/// A single observable event produced while driving the two contracts. The
/// supervisor's tick loop turns these into structured log lines using its
/// existing logger, matching how the rest of the crate logs `core.*` events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickEvent {
    LifecycleTransition {
        from: LifecycleState,
        to: LifecycleState,
    },
    LeaseAcquired,
    LeaseRenewed,
    LeaseLost,
    HeartbeatRecorded {
        sequence: u64,
    },
    RecoveryDecision(RecoveryDecision),
    RetryScheduled {
        attempts: u32,
        next_attempt_at_ms: u64,
    },
    RetryExhausted,
    TriggerDecision(TriggerDecision),
    ScheduleCompleted {
        next_run_at_ms: Option<u64>,
    },
    ScheduleFailed(FailureDecision),
    ScheduleDeadLetter,
    ScheduleRequeued,
}

/// Outcome of finishing a core-process generation, used by the caller to decide
/// whether/when to spawn the next generation. The runtime does not make the
/// spawn decision itself -- it only reports what the bounded contracts decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationOutcome {
    pub events: Vec<TickEvent>,
    pub dead_lettered: bool,
    pub next_attempt_at_ms: Option<u64>,
}

const WATCHDOG_INTERVAL_MS: u64 = 1_000;
const WATCHDOG_BASE_BACKOFF_MS: u64 = 250;

/// Drives `scheduler_state::SchedulerState` (lifecycle/lease/heartbeat/recovery) and
/// `schedule_contract::ScheduleRecord` (trigger/retry/dead-letter) for a single
/// supervised core process across restarts, in-memory, for the supervisor process's
/// lifetime.
pub struct SupervisorRuntime {
    pub scheduler: SchedulerState,
    pub watchdog: ScheduleRecord,
    generation: u64,
    heartbeat_sequence: u64,
    lease_ttl: Duration,
    lease_owner: String,
}

impl SupervisorRuntime {
    pub fn new(
        scheduler_id: impl Into<String>,
        schedule_id: impl Into<String>,
        lease_owner: impl Into<String>,
        lease_ttl: Duration,
        max_attempts: u32,
    ) -> Result<Self, RuntimeError> {
        let scheduler = SchedulerState::new(scheduler_id, 1);
        scheduler.validate()?;
        let watchdog = ScheduleRecord::new(
            schedule_id,
            ScheduleKind::Interval {
                every_ms: WATCHDOG_INTERVAL_MS,
            },
            TriggerKind::Timer,
            RetryPolicy {
                max_attempts: max_attempts.max(1),
                base_backoff_ms: WATCHDOG_BASE_BACKOFF_MS,
                dead_letter_after_exhaustion: true,
            },
            BudgetRef {
                max_wall_clock_ms: u64::MAX / 2,
                max_tool_calls: u64::MAX / 2,
                max_tokens: u64::MAX / 2,
                max_cost_units: crate::schedule_contract::MAX_BUDGET_UNITS,
            },
            PolicyRefs {
                permission_ref: "supervisor:core-process",
                approval_ref: "supervisor:auto-restart",
                cancellation_ref: "supervisor:shutdown",
            },
        )?;
        Ok(Self {
            scheduler,
            watchdog,
            generation: 0,
            heartbeat_sequence: 0,
            lease_ttl,
            lease_owner: lease_owner.into(),
        })
    }

    /// Called once when a new core-process generation is spawned. Advances the
    /// lifecycle into `Starting`, acquires a fresh lease, and records the
    /// watchdog trigger for this generation.
    pub fn start_generation(&mut self, now_ms: u64) -> Result<Vec<TickEvent>, RuntimeError> {
        let mut events = Vec::new();
        self.generation += 1;

        let from = self.scheduler.lifecycle;
        self.scheduler.transition(LifecycleState::Starting)?;
        events.push(TickEvent::LifecycleTransition {
            from,
            to: LifecycleState::Starting,
        });

        self.scheduler.acquire_lease(
            self.lease_owner.clone(),
            format!("gen-{}", self.generation),
            now_ms,
            self.lease_ttl,
        )?;
        events.push(TickEvent::LeaseAcquired);

        let trigger_id = format!("gen-{}", self.generation);
        let decision = self.watchdog.on_trigger(&trigger_id, now_ms)?;
        events.push(TickEvent::TriggerDecision(decision));

        Ok(events)
    }

    /// Called once per health-check tick (matching the supervisor's existing
    /// 1-second poll interval in `wait_for_core`) while a generation is alive.
    /// Advances lease/heartbeat state and reports the bounded recovery decision;
    /// it does not kill the process itself -- the caller still owns that action.
    pub fn observe_tick(
        &mut self,
        now_ms: u64,
        heartbeat_fresh: bool,
    ) -> Result<Vec<TickEvent>, RuntimeError> {
        let mut events = Vec::new();

        if self.scheduler.lifecycle == LifecycleState::Starting && heartbeat_fresh {
            let from = self.scheduler.lifecycle;
            self.scheduler.transition(LifecycleState::Running)?;
            events.push(TickEvent::LifecycleTransition {
                from,
                to: LifecycleState::Running,
            });
        }

        if heartbeat_fresh {
            self.heartbeat_sequence += 1;
            self.scheduler
                .record_heartbeat(now_ms, self.heartbeat_sequence);
            events.push(TickEvent::HeartbeatRecorded {
                sequence: self.heartbeat_sequence,
            });

            let lease_token = format!("gen-{}", self.generation);
            match self
                .scheduler
                .renew_lease(&lease_token, now_ms, self.lease_ttl)
            {
                Ok(()) => events.push(TickEvent::LeaseRenewed),
                Err(_) => events.push(TickEvent::LeaseLost),
            }
        }

        if self.scheduler.lifecycle == LifecycleState::Running {
            let decision = self.scheduler.recovery_decision(now_ms);
            if decision != RecoveryDecision::None {
                events.push(TickEvent::RecoveryDecision(decision));
            }
        }

        Ok(events)
    }

    /// Called once when a core-process generation exits. `success` mirrors the
    /// process exit status; `reason` is used for the dead-letter/retry record on
    /// failure. Returns the bounded retry/dead-letter decision for the caller to
    /// act on (it does not decide whether to actually respawn).
    pub fn complete_generation(
        &mut self,
        now_ms: u64,
        success: bool,
        reason: impl Into<String>,
    ) -> Result<GenerationOutcome, RuntimeError> {
        let mut events = Vec::new();
        let trigger_id = format!("gen-{}", self.generation);

        if success {
            self.watchdog
                .complete(format!("checkpoint-{}", self.generation), None)?;
            events.push(TickEvent::ScheduleCompleted {
                next_run_at_ms: None,
            });

            if self.scheduler.lifecycle == LifecycleState::Running {
                let from = self.scheduler.lifecycle;
                self.scheduler.transition(LifecycleState::Draining)?;
                events.push(TickEvent::LifecycleTransition {
                    from,
                    to: LifecycleState::Draining,
                });
            }
            let from = self.scheduler.lifecycle;
            self.scheduler.transition(LifecycleState::Stopped)?;
            events.push(TickEvent::LifecycleTransition {
                from,
                to: LifecycleState::Stopped,
            });

            return Ok(GenerationOutcome {
                events,
                dead_lettered: false,
                next_attempt_at_ms: None,
            });
        }

        let reason = reason.into();
        let failure = self.watchdog.fail(&trigger_id, now_ms, reason.clone())?;
        events.push(TickEvent::ScheduleFailed(failure));

        if self.scheduler.lifecycle == LifecycleState::Running
            || self.scheduler.lifecycle == LifecycleState::Starting
        {
            let from = self.scheduler.lifecycle;
            self.scheduler.transition(LifecycleState::Failed)?;
            events.push(TickEvent::LifecycleTransition {
                from,
                to: LifecycleState::Failed,
            });
        }

        let dead_lettered = matches!(failure, FailureDecision::DeadLetter);
        let next_attempt_at_ms = match failure {
            FailureDecision::Retry { next_run_at_ms, .. } => {
                let disposition = self.scheduler.schedule_retry(
                    now_ms,
                    Duration::from_millis(WATCHDOG_BASE_BACKOFF_MS),
                    reason,
                );
                match disposition {
                    RetryDisposition::Retry => events.push(TickEvent::RetryScheduled {
                        attempts: self.scheduler.retry.attempts,
                        next_attempt_at_ms: self.scheduler.retry.next_attempt_at_ms,
                    }),
                    RetryDisposition::Exhausted => events.push(TickEvent::RetryExhausted),
                }
                Some(next_run_at_ms)
            }
            FailureDecision::DeadLetter => {
                events.push(TickEvent::ScheduleDeadLetter);
                None
            }
        };

        if !dead_lettered && self.scheduler.lifecycle == LifecycleState::Failed {
            let from = self.scheduler.lifecycle;
            self.scheduler.transition(LifecycleState::Recovering)?;
            events.push(TickEvent::LifecycleTransition {
                from,
                to: LifecycleState::Recovering,
            });
        }

        Ok(GenerationOutcome {
            events,
            dead_lettered,
            next_attempt_at_ms,
        })
    }

    /// Requeues a dead-lettered watchdog record so the next `start_generation`
    /// call can proceed again. Exposed for operators/future tooling; the
    /// supervisor does not call this automatically.
    pub fn requeue(&mut self, now_ms: u64) -> Result<Vec<TickEvent>, RuntimeError> {
        self.watchdog.requeue(now_ms)?;
        Ok(vec![TickEvent::ScheduleRequeued])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> SupervisorRuntime {
        SupervisorRuntime::new(
            "core-watchdog",
            "core-watchdog-schedule",
            "supervisor",
            Duration::from_secs(5),
            3,
        )
        .expect("valid runtime")
    }

    #[test]
    fn two_ticks_advance_lease_and_heartbeat_state() {
        let mut runtime = runtime();
        let start_events = runtime.start_generation(0).unwrap();
        assert!(start_events
            .iter()
            .any(|event| matches!(event, TickEvent::LeaseAcquired)));
        assert_eq!(runtime.scheduler.lifecycle, LifecycleState::Starting);

        let tick1 = runtime.observe_tick(100, true).unwrap();
        assert!(tick1.iter().any(|event| matches!(
            event,
            TickEvent::LifecycleTransition {
                to: LifecycleState::Running,
                ..
            }
        )));
        assert_eq!(runtime.scheduler.lifecycle, LifecycleState::Running);
        assert_eq!(runtime.scheduler.heartbeat.as_ref().unwrap().sequence, 1);

        let tick2 = runtime.observe_tick(1_100, true).unwrap();
        assert!(tick2
            .iter()
            .any(|event| matches!(event, TickEvent::HeartbeatRecorded { sequence: 2 })));
        assert_eq!(runtime.scheduler.heartbeat.as_ref().unwrap().sequence, 2);
        assert!(runtime
            .scheduler
            .heartbeat_is_healthy(1_100, Duration::from_secs(5)));
    }

    #[test]
    fn stale_heartbeat_reports_restart_recovery_decision() {
        let mut runtime = runtime();
        runtime.start_generation(0).unwrap();
        runtime.observe_tick(100, true).unwrap();

        // MAX_HEARTBEAT_AGE (scheduler_state) is 300s; push well past it.
        let tick = runtime.observe_tick(400_000, false).unwrap();
        assert!(tick.iter().any(|event| matches!(
            event,
            TickEvent::RecoveryDecision(RecoveryDecision::Restart)
        )));
    }

    #[test]
    fn generation_failure_schedules_retry_then_recovering() {
        let mut runtime = runtime();
        runtime.start_generation(0).unwrap();
        runtime.observe_tick(100, true).unwrap();

        let outcome = runtime
            .complete_generation(200, false, "core crashed")
            .unwrap();
        assert!(!outcome.dead_lettered);
        assert!(outcome.next_attempt_at_ms.is_some());
        assert_eq!(runtime.scheduler.lifecycle, LifecycleState::Recovering);
        assert_eq!(runtime.scheduler.retry.attempts, 1);

        // Next generation starts from Recovering -> Starting, which is allowed.
        let events = runtime.start_generation(300).unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            TickEvent::LifecycleTransition {
                to: LifecycleState::Starting,
                ..
            }
        )));
    }

    #[test]
    fn repeated_failures_eventually_dead_letter_the_watchdog() {
        let mut runtime = runtime();
        let mut now = 0u64;
        for _ in 0..4 {
            runtime.start_generation(now).unwrap();
            runtime.observe_tick(now + 10, true).unwrap();
            let outcome = runtime
                .complete_generation(now + 20, false, "core crashed")
                .unwrap();
            now += 10_000;
            if outcome.dead_lettered {
                assert_eq!(runtime.scheduler.lifecycle, LifecycleState::Failed);
                let requeue_events = runtime.requeue(now).unwrap();
                assert!(requeue_events
                    .iter()
                    .any(|event| matches!(event, TickEvent::ScheduleRequeued)));
                return;
            }
        }
        panic!("expected watchdog to dead-letter after exhausting retries");
    }

    #[test]
    fn successful_generation_drains_and_stops() {
        let mut runtime = runtime();
        runtime.start_generation(0).unwrap();
        runtime.observe_tick(100, true).unwrap();

        let outcome = runtime
            .complete_generation(200, true, "clean exit")
            .unwrap();
        assert!(!outcome.dead_lettered);
        assert_eq!(runtime.scheduler.lifecycle, LifecycleState::Stopped);
    }
}
