use evohime_core::guided_calibration_sessions::*;

#[test]
fn duplicate_iteration_and_closed_session_are_recoverable_typed_outcomes() {
    let mut session = new_session(
        "s".into(),
        "workspace".into(),
        "role".into(),
        "human".into(),
        "policy".into(),
    );
    let iteration = CalibrationIteration {
        iteration_id: "i".into(),
        task_ref: "task".into(),
        baseline_hash: hash("b"),
        revised_hash: None,
        pattern_key: "p".into(),
        feedback: None,
    };
    add_iteration(&mut session, iteration.clone()).unwrap();
    assert_eq!(
        add_iteration(&mut session, iteration),
        Err(CalibrationError::DuplicateOrStale)
    );
    session.status = SessionStatus::Completed;
    assert_eq!(
        add_iteration(
            &mut session,
            CalibrationIteration {
                iteration_id: "i2".into(),
                task_ref: "task2".into(),
                baseline_hash: hash("b"),
                revised_hash: None,
                pattern_key: "p".into(),
                feedback: None
            }
        ),
        Err(CalibrationError::SessionClosed)
    );
}
