use super::*;

fn policy() -> PolicyRecord {
    PolicyRecord {
        policy_id: "p1".into(),
        revision: 1,
        owner_scope: "w1".into(),
        actor: "user".into(),
        enabled: true,
        canonical_json: br#"{}"#.to_vec(),
        content_hash: "h1".into(),
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

fn run() -> RunRecord {
    RunRecord {
        run_id: "r1".into(),
        idempotency_key: "i1".into(),
        task_id: "t1".into(),
        prompt: None,
        workspace_path: None,
        owner_scope: "w1".into(),
        policy_id: "p1".into(),
        policy_revision: 1,
        policy_hash: "h1".into(),
        goal_id: None,
        goal_version: None,
        state: "running".into(),
        continuation_index: 0,
        max_continuations: 2,
        max_model_turns: 2,
        used_model_turns: 0,
        token_budget: Some(100),
        token_used: 0,
        cost_budget_micros: Some(10),
        cost_used_micros: 0,
        stop_reason: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

#[test]
fn schema_and_reservation_are_idempotent() {
    let mut connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    save_policy(&connection, &policy()).unwrap();
    create_run(&connection, &run()).unwrap();
    assert_eq!(
        get_run_by_idempotency(&connection, "w1", "i1")
            .unwrap()
            .unwrap()
            .run_id,
        "r1"
    );
    assert!(attach_task_context(&connection, "t1", "redacted prompt", "workspace", 2).unwrap());
    assert_eq!(
        get_run(&connection, "r1")
            .unwrap()
            .unwrap()
            .prompt
            .as_deref(),
        Some("redacted prompt")
    );
    assert!(!attach_task_context(&connection, "t1", "other", "workspace", 3).unwrap());
    assert!(reserve_attempt(&mut connection, "r1", "g1", "f1", 20, 2, 2).unwrap());
    assert!(!reserve_attempt(&mut connection, "r1", "g1", "f1", 20, 2, 3).unwrap());
    assert_eq!(get_run(&connection, "r1").unwrap().unwrap().token_used, 20);
    assert!(finish_attempt(&connection, "r1", 1, "passed", br#"{}"#, 4).unwrap());
}

#[test]
fn stop_is_compare_and_set() {
    let mut connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    save_policy(&connection, &policy()).unwrap();
    create_run(&connection, &run()).unwrap();
    let input = TransitionActionInput {
        run_id: "r1",
        idempotency_key: "stop-1",
        action: "stop",
        expected_state: "running",
        next_state: "stopped",
        stop_reason: "user_stop",
        now_ms: 2,
    };
    let first = apply_transition_action(&mut connection, input).unwrap();
    let duplicate = apply_transition_action(
        &mut connection,
        TransitionActionInput { now_ms: 3, ..input },
    )
    .unwrap();
    assert_eq!(first, duplicate);
    assert!(!stop_run(&connection, "r1", "running", "again", 4).unwrap());
}

#[test]
fn gate_result_is_immutable_for_same_attempt() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    save_policy(&connection, &policy()).unwrap();
    create_run(&connection, &run()).unwrap();
    record_gate_result(
        &connection,
        &GateResultRecord {
            run_id: "r1".into(),
            gate_id: "g1".into(),
            attempt_index: 1,
            status: "passed".into(),
            evidence_ref: Some("evidence-1".into()),
            error_code: None,
            created_at_ms: 2,
        },
    )
    .unwrap();
    record_gate_result(
        &connection,
        &GateResultRecord {
            status: "failed".into(),
            evidence_ref: None,
            error_code: Some("tampered".into()),
            created_at_ms: 3,
            run_id: "r1".into(),
            gate_id: "g1".into(),
            attempt_index: 1,
        },
    )
    .unwrap();
    let stored: (String, Option<String>) = connection
        .query_row(
            "SELECT status,evidence_ref FROM continuation_gate_results WHERE run_id='r1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, ("passed".into(), Some("evidence-1".into())));
}
