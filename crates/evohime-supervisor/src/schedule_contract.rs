//! Bounded schedule and proactive Pulse contract.
//!
//! This module is deliberately side-effect free. It describes scheduling,
//! trigger de-duplication, monitor state, retry/dead-letter decisions and the
//! references needed by the supervisor to enforce the same budgets and
//! approvals as a normal run.

// Модуль — bounded-контракт: часть его поверхности сегодня вызывается только
// собственными тестами, а вызовы из supervisor появятся при wiring этапа,
// которому контракт принадлежит. Удалять её нельзя — это и есть описанный в
// планах интерфейс.
#![allow(dead_code)]

pub const MAX_ID_BYTES: usize = 128;
pub const MAX_ERROR_BYTES: usize = 512;
pub const MAX_ATTEMPTS: u32 = 16;
pub const MAX_BACKOFF_MS: u64 = 300_000;
pub const MAX_BUDGET_UNITS: u64 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleKind {
    Once,
    Interval { every_ms: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerKind {
    Manual,
    Timer,
    Event,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorState {
    Idle,
    Waiting,
    Running,
    Missed,
    DeadLetter,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerDecision {
    Start,
    Duplicate,
    Missed,
    DeadLetter,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureDecision {
    Retry { attempt: u32, next_run_at_ms: u64 },
    DeadLetter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_backoff_ms: u64,
    pub dead_letter_after_exhaustion: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetRef {
    pub max_wall_clock_ms: u64,
    pub max_tool_calls: u64,
    pub max_tokens: u64,
    pub max_cost_units: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyRefs {
    pub permission_ref: &'static str,
    pub approval_ref: &'static str,
    pub cancellation_ref: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadLetter {
    pub trigger_id: String,
    pub attempts: u32,
    pub reason: String,
    pub moved_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRecord {
    pub schedule_id: String,
    pub kind: ScheduleKind,
    pub trigger: TriggerKind,
    pub monitor: MonitorState,
    pub last_checkpoint: Option<String>,
    pub next_run_at_ms: Option<u64>,
    pub retry: RetryPolicy,
    pub attempts: u32,
    pub budgets: BudgetRef,
    pub refs: PolicyRefs,
    pub last_trigger_id: Option<String>,
    pub dead_letter: Option<DeadLetter>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractError {
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    InvalidInterval,
    InvalidRetryPolicy,
    BudgetOutOfBounds,
    AttemptsOutOfBounds,
    InvalidState,
}

impl ScheduleRecord {
    pub fn new(
        schedule_id: impl Into<String>,
        kind: ScheduleKind,
        trigger: TriggerKind,
        retry: RetryPolicy,
        budgets: BudgetRef,
        refs: PolicyRefs,
    ) -> Result<Self, ContractError> {
        let record = Self {
            schedule_id: schedule_id.into(),
            kind,
            trigger,
            monitor: MonitorState::Waiting,
            last_checkpoint: None,
            next_run_at_ms: None,
            retry,
            attempts: 0,
            budgets,
            refs,
            last_trigger_id: None,
            dead_letter: None,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text(&self.schedule_id, "schedule_id", MAX_ID_BYTES)?;
        validate_text(self.refs.permission_ref, "permission_ref", MAX_ID_BYTES)?;
        validate_text(self.refs.approval_ref, "approval_ref", MAX_ID_BYTES)?;
        validate_text(self.refs.cancellation_ref, "cancellation_ref", MAX_ID_BYTES)?;
        if let Some(checkpoint) = &self.last_checkpoint {
            validate_text(checkpoint, "last_checkpoint", MAX_ID_BYTES)?;
        }
        if let Some(trigger_id) = &self.last_trigger_id {
            validate_text(trigger_id, "trigger_id", MAX_ID_BYTES)?;
        }
        if let ScheduleKind::Interval { every_ms } = self.kind {
            if every_ms == 0 {
                return Err(ContractError::InvalidInterval);
            }
        }
        if self.retry.max_attempts == 0
            || self.retry.max_attempts > MAX_ATTEMPTS
            || self.retry.base_backoff_ms > MAX_BACKOFF_MS
        {
            return Err(ContractError::InvalidRetryPolicy);
        }
        if self.attempts > self.retry.max_attempts {
            return Err(ContractError::AttemptsOutOfBounds);
        }
        if self.budgets.max_wall_clock_ms == 0
            || self.budgets.max_tool_calls == 0
            || self.budgets.max_tokens == 0
            || self.budgets.max_cost_units > MAX_BUDGET_UNITS
        {
            return Err(ContractError::BudgetOutOfBounds);
        }
        if let Some(dead_letter) = &self.dead_letter {
            validate_text(
                &dead_letter.trigger_id,
                "dead_letter_trigger_id",
                MAX_ID_BYTES,
            )?;
            validate_text(&dead_letter.reason, "dead_letter_reason", MAX_ERROR_BYTES)?;
        }
        Ok(())
    }

    pub fn on_trigger(
        &mut self,
        trigger_id: &str,
        now_ms: u64,
    ) -> Result<TriggerDecision, ContractError> {
        validate_text(trigger_id, "trigger_id", MAX_ID_BYTES)?;
        self.validate()?;
        if self.monitor == MonitorState::Cancelled {
            return Ok(TriggerDecision::Cancelled);
        }
        if self.monitor == MonitorState::DeadLetter {
            return Ok(TriggerDecision::DeadLetter);
        }
        if self.last_trigger_id.as_deref() == Some(trigger_id) {
            return Ok(TriggerDecision::Duplicate);
        }
        if self.next_run_at_ms.is_some_and(|next| now_ms < next) {
            return Ok(TriggerDecision::Missed);
        }
        self.last_trigger_id = Some(trigger_id.to_owned());
        self.monitor = MonitorState::Running;
        Ok(TriggerDecision::Start)
    }

    pub fn mark_missed(&mut self, next_run_at_ms: u64) -> Result<(), ContractError> {
        if self.monitor == MonitorState::Cancelled || self.monitor == MonitorState::DeadLetter {
            return Err(ContractError::InvalidState);
        }
        self.monitor = MonitorState::Missed;
        self.next_run_at_ms = Some(next_run_at_ms);
        Ok(())
    }

    pub fn complete(
        &mut self,
        checkpoint: impl Into<String>,
        next_run_at_ms: Option<u64>,
    ) -> Result<(), ContractError> {
        let checkpoint = checkpoint.into();
        validate_text(&checkpoint, "last_checkpoint", MAX_ID_BYTES)?;
        if self.monitor != MonitorState::Running {
            return Err(ContractError::InvalidState);
        }
        self.last_checkpoint = Some(checkpoint);
        self.attempts = 0;
        self.next_run_at_ms = next_run_at_ms;
        self.monitor = if next_run_at_ms.is_some() {
            MonitorState::Waiting
        } else {
            MonitorState::Idle
        };
        Ok(())
    }

    pub fn fail(
        &mut self,
        trigger_id: &str,
        now_ms: u64,
        reason: impl Into<String>,
    ) -> Result<FailureDecision, ContractError> {
        validate_text(trigger_id, "trigger_id", MAX_ID_BYTES)?;
        let reason = reason.into();
        validate_text(&reason, "dead_letter_reason", MAX_ERROR_BYTES)?;
        if self.monitor != MonitorState::Running {
            return Err(ContractError::InvalidState);
        }
        if self.attempts >= self.retry.max_attempts {
            self.dead_letter = Some(DeadLetter {
                trigger_id: trigger_id.to_owned(),
                attempts: self.attempts,
                reason,
                moved_at_ms: now_ms,
            });
            self.monitor = MonitorState::DeadLetter;
            return Ok(FailureDecision::DeadLetter);
        }
        self.attempts += 1;
        let multiplier = 1u64 << self.attempts.saturating_sub(1).min(31);
        let delay = self
            .retry
            .base_backoff_ms
            .saturating_mul(multiplier)
            .min(MAX_BACKOFF_MS);
        self.next_run_at_ms = Some(now_ms.saturating_add(delay));
        self.monitor = MonitorState::Waiting;
        Ok(FailureDecision::Retry {
            attempt: self.attempts,
            next_run_at_ms: self.next_run_at_ms.expect("set above"),
        })
    }

    pub fn requeue(&mut self, now_ms: u64) -> Result<(), ContractError> {
        if self.monitor != MonitorState::DeadLetter {
            return Err(ContractError::InvalidState);
        }
        self.dead_letter = None;
        self.attempts = 0;
        self.next_run_at_ms = Some(now_ms);
        self.monitor = MonitorState::Waiting;
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.monitor = MonitorState::Cancelled;
        self.next_run_at_ms = None;
    }

    pub fn deterministic_json(&self) -> String {
        format!(
            "{{\"schedule_id\":\"{}\",\"monitor\":\"{}\",\"attempts\":{},\"next_run_at_ms\":{},\"last_checkpoint\":{},\"last_trigger_id\":{}}}",
            escape(&self.schedule_id),
            monitor_name(self.monitor),
            self.attempts,
            self.next_run_at_ms.map_or_else(|| "null".to_owned(), |value| value.to_string()),
            self.last_checkpoint.as_deref().map_or_else(|| "null".to_owned(), |value| format!("\"{}\"", escape(value))),
            self.last_trigger_id.as_deref().map_or_else(|| "null".to_owned(), |value| format!("\"{}\"", escape(value))),
        )
    }
}

fn validate_text(value: &str, field: &'static str, max_bytes: usize) -> Result<(), ContractError> {
    if value.is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(ContractError::FieldTooLong(field));
    }
    Ok(())
}

fn monitor_name(value: MonitorState) -> &'static str {
    match value {
        MonitorState::Idle => "idle",
        MonitorState::Waiting => "waiting",
        MonitorState::Running => "running",
        MonitorState::Missed => "missed",
        MonitorState::DeadLetter => "dead_letter",
        MonitorState::Cancelled => "cancelled",
    }
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule() -> ScheduleRecord {
        ScheduleRecord::new(
            "pulse-1",
            ScheduleKind::Interval { every_ms: 60_000 },
            TriggerKind::Timer,
            RetryPolicy {
                max_attempts: 2,
                base_backoff_ms: 100,
                dead_letter_after_exhaustion: true,
            },
            BudgetRef {
                max_wall_clock_ms: 10_000,
                max_tool_calls: 10,
                max_tokens: 1_000,
                max_cost_units: 100,
            },
            PolicyRefs {
                permission_ref: "permissions:default",
                approval_ref: "approval:required",
                cancellation_ref: "cancel:pulse-1",
            },
        )
        .expect("valid schedule")
    }

    #[test]
    fn rejects_unbounded_policy_and_invalid_interval() {
        assert_eq!(
            ScheduleRecord::new(
                "pulse-1",
                ScheduleKind::Interval { every_ms: 0 },
                TriggerKind::Timer,
                RetryPolicy {
                    max_attempts: 2,
                    base_backoff_ms: 1,
                    dead_letter_after_exhaustion: true
                },
                BudgetRef {
                    max_wall_clock_ms: 1,
                    max_tool_calls: 1,
                    max_tokens: 1,
                    max_cost_units: 1
                },
                PolicyRefs {
                    permission_ref: "p",
                    approval_ref: "a",
                    cancellation_ref: "c"
                },
            )
            .unwrap_err(),
            ContractError::InvalidInterval
        );
    }

    #[test]
    fn duplicate_and_missed_triggers_are_deterministic() {
        let mut value = schedule();
        value.next_run_at_ms = Some(100);
        assert_eq!(
            value.on_trigger("t-1", 50).unwrap(),
            TriggerDecision::Missed
        );
        value.next_run_at_ms = Some(0);
        assert_eq!(
            value.on_trigger("t-1", 100).unwrap(),
            TriggerDecision::Start
        );
        assert_eq!(
            value.on_trigger("t-1", 100).unwrap(),
            TriggerDecision::Duplicate
        );
    }

    #[test]
    fn retry_backoff_and_dead_letter_requeue_are_bounded() {
        let mut value = schedule();
        value.on_trigger("t-1", 0).unwrap();
        assert_eq!(
            value.fail("t-1", 100, "first").unwrap(),
            FailureDecision::Retry {
                attempt: 1,
                next_run_at_ms: 200
            }
        );
        value.on_trigger("t-2", 200).unwrap();
        assert_eq!(
            value.fail("t-2", 300, "second").unwrap(),
            FailureDecision::Retry {
                attempt: 2,
                next_run_at_ms: 500
            }
        );
        value.on_trigger("t-3", 500).unwrap();
        assert_eq!(
            value.fail("t-3", 600, "third").unwrap(),
            FailureDecision::DeadLetter
        );
        value.requeue(700).unwrap();
        assert_eq!(value.monitor, MonitorState::Waiting);
    }

    #[test]
    fn cancellation_blocks_future_triggers() {
        let mut value = schedule();
        value.cancel();
        assert_eq!(
            value.on_trigger("t-1", 0).unwrap(),
            TriggerDecision::Cancelled
        );
    }

    #[test]
    fn completion_persists_checkpoint_and_serialization_is_stable() {
        let mut value = schedule();
        value.on_trigger("t-1", 0).unwrap();
        value.complete("checkpoint-1", Some(60_000)).unwrap();
        let json = value.deterministic_json();
        assert!(json.contains("checkpoint-1"));
        assert_eq!(json, value.deterministic_json());
        assert_eq!(value.monitor, MonitorState::Waiting);
    }
}
