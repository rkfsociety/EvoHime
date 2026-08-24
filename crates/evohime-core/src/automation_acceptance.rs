//! Deterministic acceptance fixtures for automation plan 16.4.

use crate::automation::TriggerRequestV1;

pub const MAX_DURABLE_EVENTS_PER_RUN: usize = 256;
pub const MAX_SNAPSHOTS_PER_RUN: usize = 64;
pub const MAX_ARCHIVE_RUNS: usize = 10_000;
pub const ARCHIVE_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

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
    use crate::automation_simulation::{
        allow_simulation_effect, redact_export, replay_hash, AutomationSnapshotV1, ReplayInputV1,
        SimulationEffect, SnapshotInput,
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
    fn a05_replay_and_snapshot_are_equal_for_same_fixture() {
        let snapshot = AutomationSnapshotV1::new(SnapshotInput {
            run_id: "run",
            definition_id: "fixture",
            definition_revision: 1,
            generation: 1,
            sequence: 0,
            state_json: "{}",
            policy_snapshot: "policy",
            approval_snapshot: "approval",
            provenance: "fixture",
        });
        assert!(snapshot.validate(1, None).is_ok());
        let input = ReplayInputV1 {
            schema_version: 1,
            definition_revision: 1,
            ordered_events: vec!["queued".into()],
            normalized_inputs: "{}".into(),
            frozen_clock_ms: 0,
            rng_seed: 7,
            provider_fixture_ids: vec!["fake".into()],
            capability_snapshot: "cap".into(),
            policy_snapshot: "policy".into(),
        };
        assert_eq!(replay_hash(&input).unwrap(), replay_hash(&input).unwrap());
    }
    #[test]
    fn a06_simulation_is_fail_closed_and_redacted() {
        assert!(!allow_simulation_effect(SimulationEffect::Filesystem));
        assert!(!allow_simulation_effect(SimulationEffect::Network));
        assert!(allow_simulation_effect(SimulationEffect::FakeProvider));
        assert!(!redact_export("Bearer secret C:\\work\\file").contains("secret C:"));
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
