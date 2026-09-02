use evohime_core::typed_agent_handoff_contract::*;

fn packet() -> HandoffPacket {
    HandoffPacket {
        version: 1,
        handoff_id: "handoff".into(),
        from: "coder".into(),
        target: "reviewer".into(),
        objective: "review change".into(),
        reason_code: "security_review".into(),
        summary: "bounded summary".into(),
        checkpoint_ref: Some("checkpoint-1".into()),
        artifact_refs: vec!["artifact-1".into()],
        evidence_refs: vec!["event-1".into()],
        open_questions: vec!["question".into()],
        blockers: vec![],
        goal_id: None,
        workflow_run_id: "run-1".into(),
        parent_run_id: None,
        requested_context: ContextTransferSpec {
            max_bytes: 4096,
            include_checkpoint: true,
            include_artifacts: true,
            include_evidence: true,
            include_messages: false,
        },
        created_at_ms: 1,
        expires_at_ms: Some(10),
    }
}

#[test]
fn packet_is_versioned_and_provenance_bound() {
    let value = propose(packet(), "source-event").unwrap();
    assert_eq!(value.state, HandoffState::Proposed);
    assert_eq!(value.provenance["target_run"], "run-1");
}

#[test]
fn capabilities_are_not_a_packet_field_and_invalid_bounds_fail() {
    let mut value = packet();
    value.requested_context.max_bytes = MAX_CONTEXT_BYTES + 1;
    assert!(matches!(
        validate_packet(&value),
        Err(HandoffError::Invalid("packet"))
    ));
}
