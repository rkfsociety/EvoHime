use evohime_core::experience_replay_library::*;

#[test]
fn successful_experience_requires_independent_evidence_and_hash() {
    let mut record = ExperienceRecord {
        id: "e1".into(),
        scope: ExperienceScope::Project,
        scope_id: "p1".into(),
        request_summary: "build failure".into(),
        task_class: None,
        context_fingerprint: None,
        trajectory: vec![ExperienceStep {
            phase: "result".into(),
            plan_summary: Some("fix".into()),
            action_ref: None,
            action_args_projection: None,
            observation_summary: Some("tests passed".into()),
            result_class: "success".into(),
            score_delta: Some(1.0),
        }],
        outcome: Outcome::Success,
        score: ExperienceScore {
            quality: 0.9,
            correctness: Some(1.0),
            efficiency: None,
            security_compliance: 1.0,
            evidence_count: 1,
        },
        evidence_refs: vec!["test-run:1".into()],
        tags: vec!["rust".into()],
        content_hash: String::new(),
        sensitivity: "non_sensitive".into(),
        provenance: "core".into(),
        created_at_ms: 1,
        stale: false,
        pinned: false,
    };
    record.content_hash = content_hash(&record).unwrap();
    assert!(validate_and_write_gate(&record).is_ok());
    record.evidence_refs.clear();
    assert!(validate_and_write_gate(&record).is_err());
}
