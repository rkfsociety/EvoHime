use super::*;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE capability_selections (
                    task_id TEXT PRIMARY KEY NOT NULL,
                    origin TEXT NOT NULL,
                    manifest_name TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
        )
        .expect("contract fixture creates");
}

fn record(
    task_id: &str,
    origin: SelectionOrigin,
    manifest_name: &str,
) -> CapabilitySelectionRecord {
    CapabilitySelectionRecord {
        task_id: task_id.into(),
        origin,
        manifest_name: manifest_name.into(),
        state_json: r#"{"selection":{"manifest_name":"reviewer"}}"#.into(),
    }
}

#[test]
fn round_trips_pinned_selection_and_survives_reconnect_simulation() {
    let path = std::env::temp_dir().join(format!(
        "evohime-capability-selection-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    {
        let connection = Connection::open(&path).expect("sqlite opens");
        schema(&connection);
        CapabilitySelectionStoreSql::upsert(
            &connection,
            &record("task-1", SelectionOrigin::Pinned, "reviewer"),
        )
        .expect("pin persists");
    }

    // Simulate reconnect: reopen the same file with a fresh connection.
    let connection = Connection::open(&path).expect("sqlite reopens");
    let loaded = CapabilitySelectionStoreSql::get_by_task_id(&connection, "task-1")
        .expect("selection reads")
        .expect("selection present after reconnect");
    assert_eq!(loaded.origin, SelectionOrigin::Pinned);
    assert_eq!(loaded.manifest_name, "reviewer");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn upsert_overwrites_prior_choice_for_same_task() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    CapabilitySelectionStoreSql::upsert(
        &connection,
        &record("task-1", SelectionOrigin::Auto, "reviewer"),
    )
    .unwrap();
    CapabilitySelectionStoreSql::upsert(
        &connection,
        &record("task-1", SelectionOrigin::Replaced, "planner"),
    )
    .unwrap();

    let loaded = CapabilitySelectionStoreSql::get_by_task_id(&connection, "task-1")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.origin, SelectionOrigin::Replaced);
    assert_eq!(loaded.manifest_name, "planner");
}

#[test]
fn deletes_and_rejects_unbounded_or_empty_fields() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    CapabilitySelectionStoreSql::upsert(
        &connection,
        &record("task-1", SelectionOrigin::Pinned, "reviewer"),
    )
    .unwrap();
    assert!(CapabilitySelectionStoreSql::delete_by_task_id(&connection, "task-1").unwrap());
    assert!(
        CapabilitySelectionStoreSql::get_by_task_id(&connection, "task-1")
            .unwrap()
            .is_none()
    );

    let mut invalid = record("task-1", SelectionOrigin::Auto, "reviewer");
    invalid.task_id.clear();
    assert_eq!(
        invalid.validate(),
        Err(CapabilitySelectionStoreError::Empty { field: "task_id" })
    );

    invalid = record("task-1", SelectionOrigin::Auto, "reviewer");
    invalid.state_json = "x".repeat(MAX_STATE_JSON_BYTES + 1);
    assert!(matches!(
        invalid.validate(),
        Err(CapabilitySelectionStoreError::Limit {
            field: "state_json",
            ..
        })
    ));
}
