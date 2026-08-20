//! Bounded scheduler state contract for supervisor lifecycle decisions.
//!
//! The contract is intentionally side-effect free.  The supervisor can persist or
//! transport this state, while the transition helpers keep lifecycle, lease,
//! retry, shutdown and recovery decisions deterministic and bounded.

// Модуль — bounded-контракт: часть его поверхности сегодня вызывается только
// собственными тестами, а вызовы из supervisor появятся при wiring этапа,
// которому контракт принадлежит. Удалять её нельзя — это и есть описанный в
// планах интерфейс.
#![allow(dead_code)]

use std::{convert::TryFrom, time::Duration};

pub const MAX_SCHEDULER_ID_BYTES: usize = 128;
pub const MAX_LEASE_TOKEN_BYTES: usize = 128;
pub const MAX_RETRY_ATTEMPTS: u32 = 16;
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);
pub const MAX_HEARTBEAT_AGE: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Created,
    Starting,
    Running,
    Draining,
    Stopped,
    Recovering,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDisposition {
    Retry,
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownDecision {
    Continue,
    Drain,
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryDecision {
    None,
    Restart,
    Recover,
    Block,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    pub owner: String,
    pub token: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heartbeat {
    pub generation: u64,
    pub observed_at_ms: u64,
    pub sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryState {
    pub attempts: u32,
    pub next_attempt_at_ms: u64,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerState {
    pub scheduler_id: String,
    pub generation: u64,
    pub lifecycle: LifecycleState,
    pub lease: Option<Lease>,
    pub heartbeat: Option<Heartbeat>,
    pub retry: RetryState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    AttemptsOutOfBounds,
    TimestampOverflow,
    InvalidTransition,
    StaleHeartbeat,
    LeaseMismatch,
}

impl SchedulerState {
    pub fn new(scheduler_id: impl Into<String>, generation: u64) -> Self {
        Self {
            scheduler_id: scheduler_id.into(),
            generation,
            lifecycle: LifecycleState::Created,
            lease: None,
            heartbeat: None,
            retry: RetryState {
                attempts: 0,
                next_attempt_at_ms: 0,
                last_error: None,
            },
        }
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_bounded_text(&self.scheduler_id, "scheduler_id", MAX_SCHEDULER_ID_BYTES)?;
        if self.retry.attempts > MAX_RETRY_ATTEMPTS {
            return Err(ValidationError::AttemptsOutOfBounds);
        }
        if let Some(error) = &self.retry.last_error {
            validate_bounded_text(error, "last_error", MAX_SCHEDULER_ID_BYTES)?;
        }
        if let Some(lease) = &self.lease {
            validate_bounded_text(&lease.owner, "lease_owner", MAX_SCHEDULER_ID_BYTES)?;
            validate_bounded_text(&lease.token, "lease_token", MAX_LEASE_TOKEN_BYTES)?;
            if lease.expires_at_ms == 0 {
                return Err(ValidationError::TimestampOverflow);
            }
        }
        if let Some(heartbeat) = &self.heartbeat {
            if heartbeat.generation != self.generation {
                return Err(ValidationError::StaleHeartbeat);
            }
        }
        Ok(())
    }

    pub fn transition(&mut self, next: LifecycleState) -> Result<(), ValidationError> {
        let allowed = matches!(
            (self.lifecycle, next),
            (LifecycleState::Created, LifecycleState::Starting)
                | (LifecycleState::Starting, LifecycleState::Running)
                | (LifecycleState::Starting, LifecycleState::Failed)
                | (LifecycleState::Running, LifecycleState::Draining)
                | (LifecycleState::Running, LifecycleState::Failed)
                | (LifecycleState::Draining, LifecycleState::Stopped)
                | (LifecycleState::Failed, LifecycleState::Recovering)
                | (LifecycleState::Recovering, LifecycleState::Starting)
                | (LifecycleState::Recovering, LifecycleState::Failed)
                | (LifecycleState::Stopped, LifecycleState::Starting)
        );
        if !allowed {
            return Err(ValidationError::InvalidTransition);
        }
        self.lifecycle = next;
        Ok(())
    }

    pub fn acquire_lease(
        &mut self,
        owner: impl Into<String>,
        token: impl Into<String>,
        now_ms: u64,
        ttl: Duration,
    ) -> Result<(), ValidationError> {
        let owner = owner.into();
        let token = token.into();
        validate_bounded_text(&owner, "lease_owner", MAX_SCHEDULER_ID_BYTES)?;
        validate_bounded_text(&token, "lease_token", MAX_LEASE_TOKEN_BYTES)?;
        let ttl_ms =
            u64::try_from(ttl.as_millis()).map_err(|_| ValidationError::TimestampOverflow)?;
        let expires_at_ms = now_ms
            .checked_add(ttl_ms)
            .ok_or(ValidationError::TimestampOverflow)?;
        self.lease = Some(Lease {
            owner,
            token,
            expires_at_ms,
        });
        Ok(())
    }

    pub fn renew_lease(
        &mut self,
        token: &str,
        now_ms: u64,
        ttl: Duration,
    ) -> Result<(), ValidationError> {
        let lease = self.lease.as_mut().ok_or(ValidationError::LeaseMismatch)?;
        if lease.token != token || lease.expires_at_ms < now_ms {
            return Err(ValidationError::LeaseMismatch);
        }
        let ttl_ms =
            u64::try_from(ttl.as_millis()).map_err(|_| ValidationError::TimestampOverflow)?;
        lease.expires_at_ms = now_ms
            .checked_add(ttl_ms)
            .ok_or(ValidationError::TimestampOverflow)?;
        Ok(())
    }

    pub fn record_heartbeat(&mut self, observed_at_ms: u64, sequence: u64) {
        self.heartbeat = Some(Heartbeat {
            generation: self.generation,
            observed_at_ms,
            sequence,
        });
    }

    pub fn heartbeat_is_healthy(&self, now_ms: u64, max_age: Duration) -> bool {
        let age_ms = max_age.as_millis().min(MAX_HEARTBEAT_AGE.as_millis());
        self.heartbeat.as_ref().is_some_and(|heartbeat| {
            heartbeat.generation == self.generation
                && heartbeat.observed_at_ms <= now_ms
                && now_ms.saturating_sub(heartbeat.observed_at_ms) <= age_ms as u64
        })
    }

    pub fn schedule_retry(
        &mut self,
        now_ms: u64,
        base_backoff: Duration,
        error: impl Into<String>,
    ) -> RetryDisposition {
        if self.retry.attempts >= MAX_RETRY_ATTEMPTS {
            return RetryDisposition::Exhausted;
        }
        self.retry.attempts += 1;
        let exponent = self.retry.attempts.saturating_sub(1).min(31);
        let multiplier = 1u128 << exponent;
        let backoff_ms = base_backoff
            .as_millis()
            .saturating_mul(multiplier)
            .min(MAX_BACKOFF.as_millis()) as u64;
        self.retry.next_attempt_at_ms = now_ms.saturating_add(backoff_ms);
        self.retry.last_error = Some(error.into());
        RetryDisposition::Retry
    }

    pub fn shutdown_decision(&self, requested: bool) -> ShutdownDecision {
        match (requested, self.lifecycle) {
            (false, _) => ShutdownDecision::Continue,
            (true, LifecycleState::Running) => ShutdownDecision::Drain,
            (true, LifecycleState::Draining) => ShutdownDecision::Stop,
            (true, LifecycleState::Stopped) => ShutdownDecision::Stop,
            (true, _) => ShutdownDecision::Stop,
        }
    }

    pub fn recovery_decision(&self, now_ms: u64) -> RecoveryDecision {
        match self.lifecycle {
            LifecycleState::Failed => RecoveryDecision::Recover,
            LifecycleState::Running if !self.heartbeat_is_healthy(now_ms, MAX_HEARTBEAT_AGE) => {
                RecoveryDecision::Restart
            }
            LifecycleState::Recovering => RecoveryDecision::Recover,
            _ => RecoveryDecision::None,
        }
    }
}

fn validate_bounded_text(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError::EmptyField(field));
    }
    if value.len() > max_bytes {
        return Err(ValidationError::FieldTooLong(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_transitions_are_bounded() {
        let mut state = SchedulerState::new("main", 4);
        assert_eq!(state.lifecycle, LifecycleState::Created);
        state.transition(LifecycleState::Starting).unwrap();
        state.transition(LifecycleState::Running).unwrap();
        assert_eq!(
            state.transition(LifecycleState::Created),
            Err(ValidationError::InvalidTransition)
        );
        state.transition(LifecycleState::Draining).unwrap();
        state.transition(LifecycleState::Stopped).unwrap();
    }

    #[test]
    fn lease_and_generation_bound_heartbeats() {
        let mut state = SchedulerState::new("main", 8);
        state
            .acquire_lease("supervisor", "secret-token", 100, Duration::from_secs(5))
            .unwrap();
        assert!(state
            .renew_lease("secret-token", 200, Duration::from_secs(5))
            .is_ok());
        assert_eq!(
            state.renew_lease("wrong", 201, Duration::from_secs(5)),
            Err(ValidationError::LeaseMismatch)
        );
        state.record_heartbeat(500, 1);
        assert!(state.heartbeat_is_healthy(1_000, Duration::from_secs(5)));
        state.generation = 9;
        assert!(!state.heartbeat_is_healthy(1_000, Duration::from_secs(5)));
    }

    #[test]
    fn retries_backoff_and_exhaust_without_unbounded_growth() {
        let mut state = SchedulerState::new("main", 1);
        for _ in 0..MAX_RETRY_ATTEMPTS {
            assert_eq!(
                state.schedule_retry(10, Duration::from_secs(1), "failure"),
                RetryDisposition::Retry
            );
        }
        assert_eq!(
            state.schedule_retry(10, Duration::from_secs(1), "failure"),
            RetryDisposition::Exhausted
        );
        assert_eq!(state.retry.attempts, MAX_RETRY_ATTEMPTS);
        assert!(state.retry.next_attempt_at_ms <= 10 + MAX_BACKOFF.as_millis() as u64);
    }

    #[test]
    fn validation_rejects_oversized_and_stale_values() {
        let mut state = SchedulerState::new("x".repeat(MAX_SCHEDULER_ID_BYTES + 1), 1);
        assert_eq!(
            state.validate(),
            Err(ValidationError::FieldTooLong("scheduler_id"))
        );
        state.scheduler_id = "ok".into();
        state.record_heartbeat(10, 1);
        state.generation = 2;
        assert_eq!(state.validate(), Err(ValidationError::StaleHeartbeat));
    }

    #[test]
    fn shutdown_and_recovery_decisions_are_deterministic() {
        let mut state = SchedulerState::new("main", 1);
        state.transition(LifecycleState::Starting).unwrap();
        state.transition(LifecycleState::Running).unwrap();
        assert_eq!(state.shutdown_decision(true), ShutdownDecision::Drain);
        assert_eq!(state.recovery_decision(100), RecoveryDecision::Restart);
        state.transition(LifecycleState::Failed).unwrap();
        assert_eq!(state.recovery_decision(100), RecoveryDecision::Recover);
    }
}
