use super::*;
#[test]
fn schema_is_additive_and_idempotent() {
    let mut connection = Connection::open_in_memory().unwrap();
    let tx = connection.transaction().unwrap();
    install_schema(&tx).unwrap();
    tx.commit().unwrap();
    let tx = connection.transaction().unwrap();
    install_schema(&tx).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        55
    );
}

#[test]
fn command_idempotency_is_unique() {
    let mut connection = Connection::open_in_memory().unwrap();
    let tx = connection.transaction().unwrap();
    install_schema(&tx).unwrap();
    tx.commit().unwrap();
    assert!(record_command(&connection, "k", "c", "publish", "h", b"{}", 1).unwrap());
    assert!(!record_command(&connection, "k", "c", "publish", "h", b"{}", 2).unwrap());
}

#[test]
fn handoff_acceptance_and_state_commit_together() {
    let mut connection = Connection::open_in_memory().unwrap();
    let tx = connection.transaction().unwrap();
    install_schema(&tx).unwrap();
    tx.commit().unwrap();
    insert_handoff(&connection, "h", "artifact", 1, "producer", "consumer", 1).unwrap();
    assert!(accept_handoff(&mut connection, "h", "accepted", "ok", 2).unwrap());
    let state: String = connection
        .query_row(
            "SELECT state FROM artifact_handoffs WHERE handoff_id='h'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let decision: String = connection
        .query_row(
            "SELECT decision FROM artifact_acceptances WHERE handoff_id='h'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "accepted");
    assert_eq!(decision, "accepted");
}

#[test]
fn duplicate_handoff_replay_keeps_original_metadata() {
    let mut connection = Connection::open_in_memory().unwrap();
    let tx = connection.transaction().unwrap();
    install_schema(&tx).unwrap();
    tx.commit().unwrap();
    insert_handoff(
        &connection,
        "h",
        "artifact-a",
        1,
        "producer-a",
        "consumer-a",
        1,
    )
    .unwrap();
    insert_handoff(
        &connection,
        "h",
        "artifact-b",
        2,
        "producer-b",
        "consumer-b",
        2,
    )
    .unwrap();
    let row: (String, i64, String, String, String) = connection
        .query_row(
            "SELECT artifact_id, artifact_revision, producer_identity, consumer_identity, state
                 FROM artifact_handoffs WHERE handoff_id='h'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "artifact-a".into(),
            1,
            "producer-a".into(),
            "consumer-a".into(),
            "pending".into()
        )
    );
}

#[test]
fn artifact_listing_is_bounded() {
    let mut connection = Connection::open_in_memory().unwrap();
    let tx = connection.transaction().unwrap();
    install_schema(&tx).unwrap();
    tx.commit().unwrap();
    for revision in 0..300_u64 {
        insert_revision_atomic(
            &connection,
            &RegistryRow {
                artifact_id: format!("artifact-{revision:03}"),
                project_id: "project".into(),
                revision: 1,
                state: "published".into(),
                content_locator: format!("artifact://{revision}"),
                content_hash: format!("hash-{revision}"),
                metadata_json: b"{}".to_vec(),
                created_at_ms: revision as i64,
            },
            &[],
        )
        .unwrap();
    }
    assert_eq!(list(&connection, "project", u32::MAX).unwrap().len(), 256);
}
