use super::*;

#[test]
fn registry_storage_roundtrip_and_idempotency() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let input = SaveAgentRevisionInput {
        id: "a",
        revision: 1,
        status: "draft",
        content_hash: "h",
        agent_json: br#"{}"#,
        actor: "user",
        now_ms: 1,
    };
    assert!(save_agent_revision(&connection, input).unwrap());
    assert!(
        !save_agent_revision(&connection, SaveAgentRevisionInput { now_ms: 2, ..input }).unwrap()
    );
    assert_eq!(
        load_agent(&connection, "a").unwrap().unwrap(),
        b"{}".to_vec()
    );
    assert!(
        record_command_outcome(&connection, "k", "h", br#"{"ok":true}"#, 1)
            .unwrap()
            .is_none()
    );
    let previous = record_command_outcome(&connection, "k", "h", br#"{"ok":false}"#, 2)
        .unwrap()
        .unwrap();
    assert_eq!(previous.0, "h");
    assert_eq!(previous.1, br#"{"ok":true}"#.to_vec());
}

#[test]
fn stale_assignment_cannot_replace_newer_revision() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let current = SaveAssignmentInput {
        id: "assignment-1",
        revision: 2,
        agent_id: "agent-1",
        status: "active",
        source_kind: "goal",
        source_ref: "goal-1",
        assignment_json: br#"{"revision":2}"#,
        now_ms: 2,
    };
    assert!(save_assignment(&connection, current).unwrap());
    assert!(!save_assignment(
        &connection,
        SaveAssignmentInput {
            revision: 1,
            status: "pending",
            assignment_json: br#"{"revision":1}"#,
            now_ms: 3,
            ..current
        }
    )
    .unwrap());
    assert!(!save_assignment(
        &connection,
        SaveAssignmentInput {
            status: "replaced",
            assignment_json: br#"{"revision":2,"replacement":true}"#,
            now_ms: 4,
            ..current
        }
    )
    .unwrap());
    assert_eq!(
        load_assignment(&connection, "assignment-1").unwrap(),
        Some(br#"{"revision":2}"#.to_vec())
    );
}

#[test]
fn existing_agent_history_without_current_row_is_not_rewritten() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO persistent_agent_revisions
                 (agent_id, revision, content_hash, agent_json, actor, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "agent",
                2_i64,
                "original",
                br#"{"source":"old"}"#,
                "old",
                10_i64
            ],
        )
        .unwrap();
    assert!(!save_agent_revision(
        &connection,
        SaveAgentRevisionInput {
            id: "agent",
            revision: 2,
            status: "active",
            content_hash: "replacement",
            agent_json: br#"{"source":"new"}"#,
            actor: "new",
            now_ms: 20,
        },
    )
    .unwrap());
    let stored: (String, Vec<u8>) = connection
        .query_row(
            "SELECT content_hash, agent_json FROM persistent_agent_revisions
                 WHERE agent_id=?1 AND revision=?2",
            params!["agent", 2_i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, ("original".into(), br#"{"source":"old"}"#.to_vec()));
}

#[test]
fn command_outcome_replay_is_atomic_on_duplicate() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    assert!(
        record_command_outcome(&connection, "key", "hash", br#"{"ok":true}"#, 1)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        record_command_outcome(&connection, "key", "other", br#"{"ok":false}"#, 2).unwrap(),
        Some(("hash".into(), br#"{"ok":true}"#.to_vec()))
    );
}

#[test]
fn reporting_history_is_immutable_on_retry() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    save_reporting_history(
        &connection,
        "agent",
        2,
        Some("parent"),
        "created",
        "user",
        2,
    )
    .unwrap();
    save_reporting_history(&connection, "agent", 2, None, "deleted", "retry", 3).unwrap();
    assert_eq!(
        load_reporting_history(&connection, "agent", 10).unwrap(),
        vec![(2, Some("parent".into()), "created".into(), "user".into(), 2)]
    );
}

#[test]
fn goal_binding_does_not_change_for_same_identity_key() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let input = SaveGoalBindingInput {
        agent_id: "agent",
        goal_id: "goal",
        goal_revision: 2,
        responsibility: "owner",
        scope_json: None,
        binding_json: br#"{"scope":"original"}"#,
        now_ms: 2,
    };
    assert!(save_goal_binding(&connection, input).unwrap());
    assert!(!save_goal_binding(
        &connection,
        SaveGoalBindingInput {
            binding_json: br#"{"scope":"tampered"}"#,
            now_ms: 3,
            ..input
        }
    )
    .unwrap());
    let stored: Vec<u8> = connection
        .query_row(
            "SELECT binding_json FROM persistent_agent_goal_bindings WHERE agent_id='agent'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, br#"{"scope":"original"}"#.to_vec());
}
