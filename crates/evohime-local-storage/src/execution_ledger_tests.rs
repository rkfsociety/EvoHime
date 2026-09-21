use super::*;
use crate::workflow_store::NodeState;

fn sample_event() -> ExecutionEventV1 {
    ExecutionEventV1 {
        schema_version: 1,
        event_id: "a".repeat(64),
        sequence_id: None,
        run_scope: RunScope::Workflow,
        run_id: "run-1".into(),
        session_id: Some("session-1".into()),
        task_id: "task-1".into(),
        created_at_ms: 1_700_000_000_000,
        state_after: Some(ActionState::Running),
        action_id: Some("action-1".into()),
        tool_call_id: None,
        observation_id: None,
        receipt_id: None,
        failure_id: None,
        workflow_run_id: Some("wf-run-1".into()),
        node_id: Some("node-1".into()),
        attempt_id: Some("attempt-1".into()),
        effect_id: None,
        model_request_id: None,
        body: ExecutionEventBody::ToolCall {
            tool_name: "shell".into(),
            tool_call_hash: "b".repeat(32),
            manifest_hash: None,
        },
        redaction: RedactionMeta::default(),
    }
}

#[test]
fn round_trip_serde_for_each_body_variant() {
    let bodies = vec![
        ExecutionEventBody::ActionRequest {
            action_kind: "shell".into(),
            requested_capability: "fs.write".into(),
        },
        ExecutionEventBody::ToolCall {
            tool_name: "shell".into(),
            tool_call_hash: "hash".into(),
            manifest_hash: Some("mhash".into()),
        },
        ExecutionEventBody::Observation {
            summary_digest: "digest".into(),
            artifact_refs: vec![ArtifactRef {
                content_hash: "chash".into(),
                kind: "file".into(),
            }],
        },
        ExecutionEventBody::ToolReceipt {
            receipt_action_id: "receipt-action".into(),
            receipt_hash: "rhash".into(),
        },
        ExecutionEventBody::TypedFailure {
            error_class: "timeout".into(),
            provider_error_id: Some("prov-1".into()),
        },
        ExecutionEventBody::ApprovalDecision {
            approval_intent_id: "approval-1".into(),
            decision: ApprovalOutcome::Approved,
            snapshot_hash: None,
        },
        ExecutionEventBody::Cancellation {
            reason_class: "user_requested".into(),
        },
        ExecutionEventBody::RecoveryDecision {
            decision: "resume".into(),
            evidence_digest: "evidence".into(),
        },
    ];
    for body in bodies {
        let mut event = sample_event();
        event.body = body;
        let json = serde_json::to_string(&event).expect("serialize");
        let round_tripped: ExecutionEventV1 = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, round_tripped);
        event.validate().expect("valid sample body");
    }
}

#[test]
fn bound_rejects_oversized_field() {
    let mut event = sample_event();
    event.task_id = "x".repeat(MAX_ID_BYTES + 1);
    assert_eq!(
        event.validate(),
        Err(LedgerContractError::Limit {
            field: "task_id",
            max: MAX_ID_BYTES
        })
    );
}

#[test]
fn redaction_meta_has_no_raw_secret_field() {
    let event = sample_event();
    let json = serde_json::to_value(&event).expect("serialize");
    let redaction = json.get("redaction").expect("redaction present");
    let keys: Vec<&str> = redaction
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"secrets_present"));
    assert!(keys.contains(&"digest"));
}

#[test]
fn action_state_set_matches_node_state_modulo_cancelling() {
    let action_names: std::collections::BTreeSet<&str> = ActionState::ALL
        .iter()
        .filter(|state| **state != ActionState::Cancelling)
        .map(|state| state.as_str())
        .collect();
    let node_names: std::collections::BTreeSet<&str> = [
        NodeState::Pending,
        NodeState::Ready,
        NodeState::Running,
        NodeState::WaitingApproval,
        NodeState::Succeeded,
        NodeState::Failed,
        NodeState::TimedOut,
        NodeState::Cancelled,
        NodeState::Blocked,
        NodeState::Denied,
        NodeState::Skipped,
        NodeState::Degraded,
        NodeState::UnknownOutcome,
        NodeState::DeadLetter,
    ]
    .iter()
    .map(|state| state.as_str())
    .collect();
    assert_eq!(action_names, node_names);
}

#[test]
fn action_state_to_node_state_round_trips_except_cancelling() {
    for state in ActionState::ALL {
        if *state == ActionState::Cancelling {
            assert!(crate::workflow_store::NodeState::try_from(*state).is_err());
            continue;
        }
        let node_state: NodeState = (*state).try_into().expect("convertible");
        let back: ActionState = node_state.into();
        assert_eq!(*state, back);
    }
}

#[test]
fn action_state_to_run_state_is_exhaustive_and_covers_run_states() {
    use crate::workflow_store::RunState;
    let mut covered = std::collections::BTreeSet::new();
    for state in ActionState::ALL {
        covered.insert(action_state_to_run_state(*state).as_str());
    }
    for run_state in [
        RunState::Pending,
        RunState::Running,
        RunState::WaitingApproval,
        RunState::Completed,
        RunState::Failed,
        RunState::Cancelled,
        RunState::Degraded,
        RunState::Interrupted,
    ] {
        assert!(
            covered.contains(run_state.as_str()),
            "run_state {run_state:?} unreachable from any ActionState"
        );
    }
}

#[test]
fn illegal_transition_is_rejected() {
    assert_eq!(
        validate_transition(ActionState::Succeeded, ActionState::Running),
        Err(LedgerContractError::IllegalTransition {
            from: ActionState::Succeeded,
            to: ActionState::Running,
        })
    );
    assert!(validate_transition(ActionState::Pending, ActionState::Ready).is_ok());
}

#[test]
fn terminal_states_have_no_outgoing_transitions() {
    for state in ActionState::ALL {
        if state.is_terminal() {
            assert!(
                state.allowed_transitions().is_empty(),
                "{state:?} must be terminal-closed"
            );
        }
    }
}

#[test]
fn duplicate_terminal_outcome_for_same_action_is_rejected() {
    let mut first = sample_event();
    first.action_id = Some("action-x".into());
    first.state_after = Some(ActionState::Succeeded);
    let mut second = sample_event();
    second.action_id = Some("action-x".into());
    second.state_after = Some(ActionState::Failed);

    assert_eq!(
        assert_single_terminal(&[first, second]),
        Err(LedgerContractError::DuplicateTerminalOutcome {
            action_id: "action-x".into()
        })
    );
}

#[test]
fn single_terminal_outcome_is_accepted() {
    let mut running = sample_event();
    running.action_id = Some("action-y".into());
    running.state_after = Some(ActionState::Running);
    let mut terminal = sample_event();
    terminal.action_id = Some("action-y".into());
    terminal.state_after = Some(ActionState::Succeeded);

    assert!(assert_single_terminal(&[running, terminal]).is_ok());
}

#[test]
fn run_id_with_incompatible_scope_is_rejected() {
    let mut event = sample_event();
    event.run_scope = RunScope::System;
    event.session_id = None;
    // System scope доступен без workflow correlation, но не с ним.
    assert_eq!(
        event.validate(),
        Err(LedgerContractError::ScopeMismatch {
            field: "workflow_run_id/node_id/attempt_id",
            run_scope: RunScope::System,
        })
    );
}

#[test]
fn missing_session_id_outside_system_or_legacy_is_rejected() {
    let mut event = sample_event();
    event.session_id = None;
    assert_eq!(event.validate(), Err(LedgerContractError::MissingSessionId));
}

#[test]
fn system_scope_without_correlation_is_valid_without_session_id() {
    let mut event = sample_event();
    event.run_scope = RunScope::System;
    event.session_id = None;
    event.run_id = String::new();
    event.action_id = None;
    event.tool_call_id = None;
    event.workflow_run_id = None;
    event.node_id = None;
    event.attempt_id = None;
    assert!(event.validate().is_ok());
}

#[test]
fn too_many_artifact_refs_is_rejected() {
    let mut event = sample_event();
    event.body = ExecutionEventBody::Observation {
        summary_digest: "digest".into(),
        artifact_refs: (0..MAX_ARTIFACT_REFS + 1)
            .map(|i| ArtifactRef {
                content_hash: format!("hash-{i}"),
                kind: "file".into(),
            })
            .collect(),
    };
    assert_eq!(
        event.validate(),
        Err(LedgerContractError::TooManyArtifactRefs(
            MAX_ARTIFACT_REFS + 1
        ))
    );
}

#[test]
fn unknown_body_tag_fails_to_deserialize_without_panicking() {
    let malformed = r#"{"kind":"not_a_real_variant"}"#;
    let result: Result<ExecutionEventBody, _> = serde_json::from_str(malformed);
    assert!(result.is_err());
}

#[test]
fn legacy_event_id_is_deterministic_and_bounded() {
    let a = legacy_event_id(1, "task", "type", b"payload", "2024-01-01T00:00:00Z");
    let b = legacy_event_id(1, "task", "type", b"payload", "2024-01-01T00:00:00Z");
    assert_eq!(a, b);
    assert_eq!(a.len(), LEGACY_EVENT_ID_HEX_LEN);
    let different = legacy_event_id(2, "task", "type", b"payload", "2024-01-01T00:00:00Z");
    assert_ne!(a, different);
}

#[test]
fn legacy_event_id_differs_on_payload_boundary_shift() {
    // "ab"+"c" и "a"+"bc" не должны совпасть — length-prefixing защищает
    // от конкатенационных коллизий на границе полей.
    let a = legacy_event_id(1, "ab", "c", b"", "");
    let b = legacy_event_id(1, "a", "bc", b"", "");
    assert_ne!(a, b);
}

fn open_test_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory connection");
    connection
        .pragma_update(None, "foreign_keys", true)
        .expect("enable foreign keys");
    connection
}

#[test]
fn install_schema_adds_event_columns_idempotently() {
    let connection = open_test_connection();
    connection
            .execute_batch(
                "CREATE TABLE events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                INSERT INTO events(task_id, event_type, payload) VALUES ('legacy-task', 'legacy.type', x'00');",
            )
            .expect("legacy events table with a row");

    install_schema(&connection).expect("first install");
    install_schema(&connection).expect("second install is a no-op");

    let (task_id, event_id): (String, Option<String>) = connection
        .query_row(
            "SELECT task_id, event_id FROM events WHERE task_id = 'legacy-task'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("legacy row survives");
    assert_eq!(task_id, "legacy-task");
    assert_eq!(event_id, None);
}

#[test]
fn install_schema_rebuilds_workflow_run_nodes_check_and_keeps_rows() {
    let connection = open_test_connection();
    connection
        .execute_batch(
            "CREATE TABLE events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE workflow_runs (
                    run_id TEXT PRIMARY KEY NOT NULL,
                    state TEXT NOT NULL
                );
                CREATE TABLE workflow_run_nodes (
                    run_id TEXT NOT NULL REFERENCES workflow_runs(run_id) ON DELETE CASCADE,
                    node_id TEXT NOT NULL,
                    action_kind TEXT NOT NULL,
                    state TEXT NOT NULL CHECK(state IN
                        ('pending','ready','running','waiting_approval','succeeded','failed',
                         'timed_out','cancelled','blocked','denied','skipped','degraded',
                         'unknown_outcome','dead_letter')),
                    attempts INTEGER NOT NULL DEFAULT 0,
                    output_json TEXT NOT NULL DEFAULT '',
                    error_code TEXT NOT NULL DEFAULT '',
                    error_message TEXT NOT NULL DEFAULT '',
                    approval_id TEXT NOT NULL DEFAULT '',
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(run_id, node_id)
                );
                INSERT INTO workflow_runs(run_id, state) VALUES ('run-1', 'running');
                INSERT INTO workflow_run_nodes(run_id, node_id, action_kind, state, updated_at_ms)
                    VALUES ('run-1', 'node-1', 'shell', 'running', 1);",
        )
        .expect("legacy workflow tables with a row");

    install_schema(&connection).expect("first install rebuilds CHECK");
    install_schema(&connection).expect("second install is a no-op");

    let state: String = connection
        .query_row(
            "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
            [],
            |row| row.get(0),
        )
        .expect("existing row survives rebuild");
    assert_eq!(state, "running");

    connection
            .execute(
                "UPDATE workflow_run_nodes SET state = 'cancelling' WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
            )
            .expect("CHECK now accepts cancelling");
}

#[test]
fn reconcile_action_state_only_flags_open_dispatch_marker() {
    assert_eq!(
        reconcile_action_state(
            ActionState::Running,
            DispatchMarkerStatus::StartedNotCompleted
        ),
        Some(ActionState::UnknownOutcome)
    );
    assert_eq!(
        reconcile_action_state(ActionState::Running, DispatchMarkerStatus::Absent),
        None
    );
    assert_eq!(
        reconcile_action_state(ActionState::Pending, DispatchMarkerStatus::Absent),
        None
    );
    assert_eq!(
        reconcile_action_state(
            ActionState::WaitingApproval,
            DispatchMarkerStatus::StartedNotCompleted
        ),
        None
    );
    assert_eq!(
        reconcile_action_state(
            ActionState::Succeeded,
            DispatchMarkerStatus::StartedNotCompleted
        ),
        None
    );
}
