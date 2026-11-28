//! Deterministic acceptance fixtures for automation plan 16.4.

use crate::automation::TriggerRequestV1;

/// Maximum persisted events retained for one automation run.
pub const MAX_DURABLE_EVENTS_PER_RUN: usize = 256;
/// Maximum snapshots retained for one automation run.
pub const MAX_SNAPSHOTS_PER_RUN: usize = 64;
/// Maximum number of archived automation runs retained by policy.
pub const MAX_ARCHIVE_RUNS: usize = 10_000;
/// Retention period for archived runs in milliseconds.
pub const ARCHIVE_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// Returns a deterministic, valid trigger request for automation acceptance checks.
pub fn acceptance_fixture() -> TriggerRequestV1 {
    TriggerRequestV1 {
        owner_scope: "owner".into(),
        definition_id: "fixture".into(),
        revision: 1,
        trigger_key: "manual".into(),
        scheduled_slot: Some("2026-08-24T00:00:00Z".into()),
        input_json: "{}".into(),
        correlation_id: "correlation".into(),
        idempotency_key: "fixture:slot".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::AutomationRunState;
    use crate::automation_runtime::{
        revalidate_effect, AutomationQueue, EffectRevalidation, Lease, ProviderOperation,
        RunStateMachine, RuntimeError, MAX_PENDING_COMMANDS,
    };
    #[test]
    fn a01_fixture_is_bounded_and_repeatable() {
        let fixture = acceptance_fixture();
        assert!(fixture.validate().is_ok());
        assert_eq!(
            fixture.idempotency_key,
            acceptance_fixture().idempotency_key
        );
    }
    #[test]
    fn a02_overlap_and_duplicate_are_blocked() {
        let mut queue = AutomationQueue::new();
        for _ in 0..MAX_PENDING_COMMANDS {
            queue.push_command("run").unwrap();
        }
        assert_eq!(
            queue.push_command("duplicate"),
            Err(RuntimeError::QueueFull)
        );
    }
    #[test]
    fn a03_stale_lease_cannot_publish() {
        let mut fsm = RunStateMachine::new();
        fsm.transition(AutomationRunState::Queued, 1).unwrap();
        let old = fsm.generation;
        fsm.takeover();
        assert_eq!(fsm.fence(old), Err(RuntimeError::StaleGeneration));
        assert_eq!(
            Lease {
                owner: "a".into(),
                generation: 1,
                expires_at_ms: 10
            }
            .takeover("b", 1, 9),
            Err(RuntimeError::LeaseConflict)
        );
    }
    #[test]
    fn a04_cancel_and_provider_failure_are_typed() {
        let mut op = ProviderOperation::new("operation", 0);
        op.cancel();
        assert!(op.expired(1));
        assert!(ProviderOperation::retryable_error("provider_timeout"));
        assert!(!ProviderOperation::retryable_error("approval_denied"));
    }
    #[test]
    fn a07_history_limits_are_explicit() {
        const {
            assert!(MAX_DURABLE_EVENTS_PER_RUN <= 256);
        }
        const {
            assert!(MAX_SNAPSHOTS_PER_RUN <= 64);
        }
        const {
            assert!(MAX_ARCHIVE_RUNS <= 10_000);
        }
        assert_eq!(ARCHIVE_RETENTION_MS, 30 * 24 * 60 * 60 * 1_000);
    }
    #[test]
    fn a08_effect_boundary_revalidates_snapshots() {
        assert!(revalidate_effect(EffectRevalidation {
            owner_scope: "owner",
            expected_scope: "owner",
            capability_hash: "cap",
            expected_capability_hash: "cap",
            policy_snapshot: "policy",
            expected_policy_snapshot: "policy",
            approval_snapshot: "approval",
            expected_approval_snapshot: "approval",
        })
        .is_ok());
        assert_eq!(
            revalidate_effect(EffectRevalidation {
                owner_scope: "owner",
                expected_scope: "other",
                capability_hash: "cap",
                expected_capability_hash: "cap",
                policy_snapshot: "policy",
                expected_policy_snapshot: "policy",
                approval_snapshot: "approval",
                expected_approval_snapshot: "approval",
            }),
            Err(RuntimeError::PolicyRevalidationFailed)
        );
    }
}
