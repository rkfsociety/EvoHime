use evohime_core::guided_calibration_sessions::*;

fn feedback(provenance: &str) -> Feedback {
    Feedback {
        actor_ref: "human:1".into(),
        rating: FeedbackRating::Partial,
        correction_hash: hash("correction"),
        redacted_note: "bounded note".into(),
        provenance_ref: provenance.into(),
    }
}
fn iteration(id: &str, provenance: &str) -> CalibrationIteration {
    CalibrationIteration {
        iteration_id: id.into(),
        task_ref: format!("task-{id}"),
        baseline_hash: hash("baseline"),
        revised_hash: Some(hash("revised")),
        pattern_key: "concise".into(),
        feedback: Some(feedback(provenance)),
    }
}

#[test]
fn durable_contract_keeps_dataset_redacted_and_provenanced() {
    let mut session = new_session(
        "session-1".into(),
        "workspace".into(),
        "role-ref".into(),
        "human:1".into(),
        "policy-hash".into(),
    );
    add_iteration(&mut session, iteration("i1", "event-1")).unwrap();
    add_iteration(&mut session, iteration("i2", "event-2")).unwrap();
    assert!(session.dataset_hash.starts_with("sha256:"));
    let candidate =
        consolidate(&session, "candidate-1", "concise", "prefer concise answers").unwrap();
    assert_eq!(candidate.status, "proposed_for_refinement");
}
