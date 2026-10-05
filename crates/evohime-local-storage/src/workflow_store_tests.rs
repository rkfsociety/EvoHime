use super::*;
use sha2::Digest;

#[allow(clippy::too_many_arguments)]
fn update_node_state(
    connection: &Connection,
    run_id: &str,
    node_id: &str,
    state: NodeState,
    attempts: u32,
    output_json: &str,
    error_code: &str,
    error_message: &str,
    now_ms: i64,
) -> Result<(), WorkflowStoreError> {
    super::update_node_state(
        connection,
        UpdateNodeStateInput {
            run_id,
            node_id,
            state,
            attempts,
            output_json,
            error_code,
            error_message,
            now_ms,
        },
    )
}

fn connection() -> Connection {
    let connection = Connection::open_in_memory().expect("memory database");
    connection
        .pragma_update(None, "foreign_keys", true)
        .expect("foreign keys");
    install_schema(&connection).expect("schema");
    install_schema(&connection).expect("schema is idempotent");
    connection
}

fn run(run_id: &str) -> WorkflowRunRecord {
    WorkflowRunRecord {
        run_id: run_id.into(),
        task_id: "task-1".into(),
        template_id: "repository-research".into(),
        template_version: 1,
        graph_id: "template.repository-research".into(),
        graph_version: 1,
        graph_hash: "a".repeat(64),
        graph_json: "{}".into(),
        inputs_json: "{}".into(),
        policy_json: r#"{"workspace_path":"C:\\repo"}"#.into(),
        state: RunState::Pending,
        created_at_ms: 1_000,
        updated_at_ms: 1_000,
        terminal_reason: String::new(),
        cancel_requested: false,
        lease_owner: String::new(),
        lease_expires_at_ms: 0,
    }
}

fn node(run_id: &str, node_id: &str) -> WorkflowNodeRecord {
    WorkflowNodeRecord {
        run_id: run_id.into(),
        node_id: node_id.into(),
        action_kind: "transform".into(),
        state: NodeState::Pending,
        attempts: 0,
        output_json: String::new(),
        error_code: String::new(),
        error_message: String::new(),
        approval_id: String::new(),
        updated_at_ms: 1_000,
    }
}

fn recipe_link(
    run_id: &str,
    idempotency_key: &str,
) -> crate::capability_recipe_store::RecipeRunLink {
    crate::capability_recipe_store::RecipeRunLink {
        run_id: run_id.into(),
        recipe_id: "knowledge-grounding".into(),
        recipe_version: 1,
        recipe_hash: format!("sha256:{}", "b".repeat(64)),
        template_id: "repository-research".into(),
        template_version: 1,
        template_graph_hash: "c".repeat(64),
        run_graph_hash: "a".repeat(64),
        input_hash: hex::encode(sha2::Sha256::digest(b"{}")),
        workspace_hash: hex::encode(sha2::Sha256::digest(b"C:\\repo")),
        idempotency_key: idempotency_key.into(),
        created_at_ms: 1_000,
    }
}

#[test]
fn a_run_is_stored_with_its_nodes_and_read_back_unchanged() {
    let connection = connection();
    let record = run("run-1");
    insert_run(
        &connection,
        &record,
        &[node("run-1", "a"), node("run-1", "b")],
    )
    .expect("insert");
    assert_eq!(get_run(&connection, "run-1").expect("get"), Some(record));
    let nodes = list_nodes(&connection, "run-1").expect("nodes");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].node_id, "a");
}

#[test]
fn guided_run_and_recipe_link_are_committed_together() {
    let connection = connection();
    let record = run("guided-run-1");
    let link = recipe_link(&record.run_id, "recipe-request-1");

    insert_run_with_recipe(&connection, &record, &[node(&record.run_id, "a")], &link)
        .expect("workflow and recipe link commit");

    assert_eq!(
        get_run(&connection, &record.run_id).expect("run"),
        Some(record)
    );
    assert_eq!(
        crate::capability_recipe_store::get_by_run(&connection, &link.run_id).expect("recipe link"),
        Some(link.clone())
    );
    assert_eq!(
        crate::capability_recipe_store::get_by_idempotency_key(
            &connection,
            &link.recipe_id,
            link.recipe_version,
            &link.idempotency_key,
        )
        .expect("idempotency lookup"),
        Some(link)
    );
}

#[test]
fn conflicting_recipe_idempotency_rolls_back_the_new_workflow_run() {
    let connection = connection();
    let first = run("guided-run-1");
    let first_link = recipe_link(&first.run_id, "same-request");
    insert_run_with_recipe(
        &connection,
        &first,
        &[node(&first.run_id, "a")],
        &first_link,
    )
    .expect("first guided run");

    let second = run("guided-run-2");
    let mut conflicting_link = recipe_link(&second.run_id, "same-request");
    conflicting_link.recipe_hash = format!("sha256:{}", "f".repeat(64));
    assert!(insert_run_with_recipe(
        &connection,
        &second,
        &[node(&second.run_id, "a")],
        &conflicting_link,
    )
    .is_err());
    assert_eq!(
        get_run(&connection, &second.run_id).expect("run lookup"),
        None
    );
    assert_eq!(
        crate::capability_recipe_store::get_by_run(&connection, &first.run_id).expect("first link"),
        Some(first_link)
    );
}

#[test]
fn a_terminal_node_cannot_be_moved_back_into_work() {
    let connection = connection();
    insert_run(&connection, &run("run-1"), &[node("run-1", "a")]).expect("insert");
    update_node_state(
        &connection,
        "run-1",
        "a",
        NodeState::Succeeded,
        1,
        "{}",
        "",
        "",
        2_000,
    )
    .expect("terminal");
    let error = update_node_state(
        &connection,
        "run-1",
        "a",
        NodeState::Running,
        2,
        "",
        "",
        "",
        3_000,
    )
    .expect_err("no resurrection");
    assert!(matches!(error, WorkflowStoreError::UnknownNode { .. }));
}

#[test]
fn event_sequence_is_monotonic_and_replayable_from_any_point() {
    let connection = connection();
    insert_run(&connection, &run("run-1"), &[node("run-1", "a")]).expect("insert");
    for index in 0..5 {
        let sequence = append_event(
            &connection,
            "run-1",
            "a",
            "",
            "workflow.node_started",
            &format!("{{\"index\":{index}}}"),
            2_000 + index,
        )
        .expect("event");
        assert_eq!(sequence, index);
    }
    let tail = list_events(&connection, "run-1", 2, 10).expect("replay");
    assert_eq!(tail.len(), 2);
    assert_eq!(tail[0].run_sequence, 3);
    assert_eq!(tail[1].run_sequence, 4);
}

#[test]
fn a_crash_after_the_dispatch_marker_leaves_an_unknown_outcome() {
    let connection = connection();
    insert_run(&connection, &run("run-1"), &[node("run-1", "a")]).expect("insert");
    begin_attempt(
        &connection,
        &WorkflowAttemptRecord {
            attempt_id: "attempt-1".into(),
            run_id: "run-1".into(),
            node_id: "a".into(),
            attempt: 1,
            graph_hash: "a".repeat(64),
            input_hash: "b".repeat(64),
            dispatched_at_ms: 2_000,
            completed_at_ms: None,
            outcome: String::new(),
            error_code: String::new(),
        },
    )
    .expect("dispatch marker");
    update_run_state(&connection, "run-1", RunState::Running, "", 2_000).expect("running");

    let outcome = recover_after_restart(&connection, 5_000).expect("recovery");
    assert_eq!(outcome.unknown_attempts, vec!["attempt-1".to_string()]);
    assert_eq!(outcome.interrupted_runs, vec!["run-1".to_string()]);
    let nodes = list_nodes(&connection, "run-1").expect("nodes");
    assert_eq!(nodes[0].state, NodeState::UnknownOutcome);
    assert_eq!(
        get_run(&connection, "run-1").expect("run").unwrap().state,
        RunState::Interrupted
    );
    // Слепого повтора не будет: узел терминальный.
    assert!(update_node_state(
        &connection,
        "run-1",
        "a",
        NodeState::Running,
        2,
        "",
        "",
        "",
        6_000
    )
    .is_err());
}

#[test]
fn a_crash_before_the_dispatch_marker_leaves_nothing_to_recover() {
    let connection = connection();
    insert_run(&connection, &run("run-1"), &[node("run-1", "a")]).expect("insert");
    let outcome = recover_after_restart(&connection, 5_000).expect("recovery");
    assert!(outcome.unknown_attempts.is_empty());
    assert!(outcome.interrupted_runs.is_empty());
    let nodes = list_nodes(&connection, "run-1").expect("nodes");
    assert_eq!(nodes[0].state, NodeState::Pending);
}

#[test]
fn a_lease_is_exclusive_until_it_expires() {
    let connection = connection();
    insert_run(&connection, &run("run-1"), &[node("run-1", "a")]).expect("insert");
    assert!(acquire_lease(&connection, "run-1", "core-a", 10_000, 1_000).expect("lease"));
    assert!(!acquire_lease(&connection, "run-1", "core-b", 20_000, 2_000).expect("busy"));
    assert!(acquire_lease(&connection, "run-1", "core-b", 30_000, 11_000).expect("expired"));
    release_lease(&connection, "run-1", "core-b", 12_000).expect("release");
    assert_eq!(
        get_run(&connection, "run-1")
            .expect("run")
            .unwrap()
            .lease_owner,
        ""
    );
}

#[test]
fn oversized_payloads_are_rejected_instead_of_truncated() {
    let connection = connection();
    let mut record = run("run-1");
    record.graph_json = "x".repeat(MAX_GRAPH_JSON_BYTES + 1);
    assert_eq!(
        insert_run(&connection, &record, &[]).expect_err("limit"),
        WorkflowStoreError::Limit {
            field: "graph_json",
            max: MAX_GRAPH_JSON_BYTES
        }
    );
}

#[test]
fn cancellation_is_recorded_only_while_the_run_is_still_live() {
    let connection = connection();
    insert_run(&connection, &run("run-1"), &[node("run-1", "a")]).expect("insert");
    assert!(request_cancel(&connection, "run-1", 2_000).expect("cancel"));
    update_run_state(
        &connection,
        "run-1",
        RunState::Cancelled,
        "cancelled",
        3_000,
    )
    .expect("terminal");
    assert!(!request_cancel(&connection, "run-1", 4_000).expect("already terminal"));
}
