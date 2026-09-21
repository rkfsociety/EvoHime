use super::*;

#[test]
fn metadata_and_restore_journal_round_trip() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let record = WorkspaceCheckpointRecord {
        checkpoint_id: "cp-1".into(),
        workspace_id: "ws-1".into(),
        task_id: Some("t-1".into()),
        snapshot_hash: "a".repeat(64),
        manifest_json: br#"{"version":1}"#.to_vec(),
        created_at_ms: 1,
        pinned: false,
    };
    insert_checkpoint(&connection, &record).unwrap();
    assert_eq!(get_checkpoint(&connection, "cp-1").unwrap(), Some(record));
    append_restore_journal(
        &connection,
        &RestoreJournalRecord {
            operation_id: "op-1".into(),
            checkpoint_id: "cp-1".into(),
            operation: "workspace".into(),
            state: "completed".into(),
            detail_json: b"{}".to_vec(),
            created_at_ms: 2,
        },
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM workspace_state_restore_journal",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
