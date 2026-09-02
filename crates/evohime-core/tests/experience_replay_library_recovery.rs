use evohime_core::experience_replay_library::*;

#[test]
fn unknown_outcome_is_not_replayed_as_success_and_context_is_bounded() {
    let mut record = ExperienceRecord {
        id: "e1".into(),
        scope: ExperienceScope::Session,
        scope_id: "s1".into(),
        request_summary: "x".into(),
        task_class: None,
        context_fingerprint: None,
        trajectory: vec![],
        outcome: Outcome::UnknownOutcome,
        score: ExperienceScore {
            quality: 0.0,
            correctness: None,
            efficiency: None,
            security_compliance: 1.0,
            evidence_count: 1,
        },
        evidence_refs: vec!["run:1".into()],
        tags: vec![],
        content_hash: String::new(),
        sensitivity: "non_sensitive".into(),
        provenance: "core".into(),
        created_at_ms: 1,
        stale: false,
        pinned: false,
    };
    record.content_hash = content_hash(&record).unwrap();
    assert_eq!(
        validate_and_write_gate(&record),
        Err(ExperienceError::UnknownOutcome)
    );
    assert!(project_context(&[record], 1).unwrap().len() <= 1);
}
