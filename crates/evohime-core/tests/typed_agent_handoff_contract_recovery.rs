use evohime_core::typed_agent_handoff_contract::*;

#[test]
fn ack_nack_lifecycle_and_expiry_are_deterministic() {
    let mut value = propose(
        HandoffPacket {
            version: 1,
            handoff_id: "h".into(),
            from: "a".into(),
            target: "b".into(),
            objective: "o".into(),
            reason_code: "r".into(),
            summary: "s".into(),
            checkpoint_ref: None,
            artifact_refs: vec![],
            evidence_refs: vec![],
            open_questions: vec![],
            blockers: vec![],
            goal_id: None,
            workflow_run_id: "run".into(),
            parent_run_id: None,
            requested_context: ContextTransferSpec {
                max_bytes: 1,
                include_checkpoint: false,
                include_artifacts: false,
                include_evidence: false,
                include_messages: false,
            },
            created_at_ms: 1,
            expires_at_ms: Some(10),
        },
        "event",
    )
    .unwrap();
    transition(&mut value, HandoffState::Accepted, "b", "ack", 1, 2).unwrap();
    assert_eq!(
        transition(&mut value, HandoffState::Active, "b", "start", 2, 11),
        Err(HandoffError::Expired)
    );
}
