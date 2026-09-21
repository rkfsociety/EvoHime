use super::*;

fn schema(connection: &Connection) {
    connection
            .execute_batch(
                "CREATE TABLE child_handoffs (
                    handoff_id TEXT PRIMARY KEY NOT NULL,
                    task_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    from_role TEXT NOT NULL,
                    to_role TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    envelope_json BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE child_task_requests (
                    child_task_id TEXT PRIMARY KEY NOT NULL,
                    parent_task_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    request_json BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                 CREATE TABLE child_reports (
                    child_task_id TEXT PRIMARY KEY NOT NULL,
                    parent_task_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    confidence_percent INTEGER NOT NULL,
                    report_json BLOB NOT NULL,
                     accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 );
                 CREATE TABLE coordinator_child_checkpoint (
                     schema_version INTEGER NOT NULL DEFAULT 1,
                     child_task_id TEXT NOT NULL,
                     parent_task_id TEXT NOT NULL,
                     revision INTEGER NOT NULL,
                     state TEXT NOT NULL CHECK(state IN ('created','queued','running','validating','waiting_parent_acceptance','accepted','rejected','failed','cancelled','timed_out','aborted','revise_plan')),
                     failure_reason TEXT,
                     dead_letter INTEGER NOT NULL DEFAULT 0,
                     report_json BLOB,
                     evidence_locators_json BLOB,
                     provenance_hashes_json BLOB,
                     parent_sequence INTEGER NOT NULL,
                     lease_deadline_monotonic_ms INTEGER,
                     lease_created_monotonic_ms INTEGER,
                     lease_clock_boot_id TEXT,
                     lease_holder_process_id TEXT,
                     last_transition_event TEXT NOT NULL,
                     last_transition_at_ms INTEGER NOT NULL,
                     created_at_ms INTEGER NOT NULL,
                     PRIMARY KEY(child_task_id, revision)
                 );
                 CREATE TABLE child_parent_sequences (
                     parent_task_id TEXT PRIMARY KEY NOT NULL,
                     next_sequence INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .expect("contract fixture creates");
}

fn handoff(id: &str, sequence: u64) -> HandoffRecord {
    HandoffRecord {
        handoff_id: id.into(),
        task_id: "task-1".into(),
        kind: "delegate".into(),
        status: "pending".into(),
        from_role: "coordinator".into(),
        to_role: "researcher".into(),
        sequence,
        envelope_json: r#"{"handoff_id":"h"}"#.into(),
    }
}

fn request(id: &str) -> ChildTaskRequestRecord {
    ChildTaskRequestRecord {
        child_task_id: id.into(),
        parent_task_id: "task-1".into(),
        role: "researcher".into(),
        kind: "code_search".into(),
        request_json: r#"{"child_task_id":"child-1"}"#.into(),
    }
}

fn report(id: &str) -> ChildReportRecord {
    ChildReportRecord {
        child_task_id: id.into(),
        parent_task_id: "task-1".into(),
        status: "complete".into(),
        confidence_percent: 90,
        report_json: r#"{"child_task_id":"child-1"}"#.into(),
    }
}

#[test]
fn round_trips_handoff_without_schema_migration() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let expected = handoff("h-1", 1);

    ChildStoreSql::insert_handoff(&connection, &expected).expect("handoff inserts");

    let listed =
        ChildStoreSql::list_handoffs_by_task(&connection, "task-1", 10).expect("list reads");
    assert_eq!(listed, vec![expected]);
}

#[test]
fn lists_handoffs_in_sequence_order() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    ChildStoreSql::insert_handoff(&connection, &handoff("h-2", 2)).expect("insert 2");
    ChildStoreSql::insert_handoff(&connection, &handoff("h-1", 1)).expect("insert 1");

    let listed =
        ChildStoreSql::list_handoffs_by_task(&connection, "task-1", 10).expect("list reads");
    assert_eq!(
        listed
            .iter()
            .map(|item| item.handoff_id.as_str())
            .collect::<Vec<_>>(),
        ["h-1", "h-2"]
    );
}

#[test]
fn round_trips_request_and_report_and_upserts_by_id() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    ChildStoreSql::insert_child_task_request(&connection, &request("child-1"))
        .expect("request inserts");
    assert_eq!(
        ChildStoreSql::get_child_task_request(&connection, "child-1").expect("request reads"),
        Some(request("child-1"))
    );

    let mut updated = request("child-1");
    updated.role = "planner".into();
    ChildStoreSql::insert_child_task_request(&connection, &updated).expect("request upserts");
    assert_eq!(
        ChildStoreSql::get_child_task_request(&connection, "child-1")
            .expect("request reads")
            .map(|record| record.role),
        Some("planner".to_string())
    );

    ChildStoreSql::insert_child_report(&connection, &report("child-1")).expect("report inserts");
    assert_eq!(
        ChildStoreSql::get_child_report(&connection, "child-1").expect("report reads"),
        Some(report("child-1"))
    );
}

#[test]
fn rejects_unbounded_or_empty_contract_fields_before_sql() {
    let mut invalid = handoff("h-1", 1);
    invalid.envelope_json = "x".repeat(MAX_ENVELOPE_JSON_BYTES + 1);
    assert!(matches!(
        invalid.validate(),
        Err(ChildStoreError::Limit {
            field: "envelope_json",
            ..
        })
    ));

    let mut invalid_request = request("child-1");
    invalid_request.child_task_id.clear();
    assert_eq!(
        invalid_request.validate(),
        Err(ChildStoreError::Empty {
            field: "child_task_id"
        })
    );

    let mut invalid_report = report("child-1");
    invalid_report.status.clear();
    assert_eq!(
        invalid_report.validate(),
        Err(ChildStoreError::Empty { field: "status" })
    );
}

#[test]
fn sequences_are_atomic_and_checkpoint_round_trips() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    assert_eq!(
        ChildStoreSql::next_parent_sequence(&connection, "parent").unwrap(),
        1
    );
    assert_eq!(
        ChildStoreSql::next_parent_sequence(&connection, "parent").unwrap(),
        2
    );
    let record = CoordinatorCheckpointRecord {
        schema_version: 1,
        child_task_id: "child".into(),
        parent_task_id: "parent".into(),
        revision: 0,
        state: "created".into(),
        failure_reason: None,
        dead_letter: false,
        report_json: None,
        evidence_locators_json: None,
        provenance_hashes_json: None,
        parent_sequence: 2,
        lease_deadline_monotonic_ms: Some(100),
        lease_created_monotonic_ms: Some(1),
        lease_clock_boot_id: Some("boot".into()),
        lease_holder_process_id: Some("pid".into()),
        last_transition_event: "created".into(),
        last_transition_at_ms: 2,
        created_at_ms: 1,
    };
    ChildStoreSql::upsert_coordinator_checkpoint(&connection, &record).unwrap();
    assert_eq!(
        ChildStoreSql::latest_coordinator_checkpoint(&connection, "child").unwrap(),
        Some(record)
    );
}
