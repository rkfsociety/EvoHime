use super::*;

#[test]
fn stores_change_set_and_candidate() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    put_change_set(&c, "s", 1, "h", b"{}", 1).unwrap();
    put_candidate(&c, "c", "s", "d", b"{}", 2).unwrap();
    assert_eq!(get_change_set(&c, "s").unwrap(), Some(b"{}".to_vec()));
    assert_eq!(get_candidate(&c, "c").unwrap(), Some(b"{}".to_vec()));
    assert!(put_idempotent(&c, "request-1", b"{}", 3).unwrap());
    assert!(!put_idempotent(&c, "request-1", b"different", 4).unwrap());
    assert_eq!(
        get_idempotent(&c, "request-1").unwrap(),
        Some(b"{}".to_vec())
    );
}

#[test]
fn idempotency_claim_is_single_owner_and_pending_is_not_replayed() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert_eq!(
        claim_idempotent(&c, "request-1", 1).unwrap(),
        IdempotencyClaim::Claimed
    );
    assert_eq!(
        claim_idempotent(&c, "request-1", 2).unwrap(),
        IdempotencyClaim::Pending
    );
    assert!(complete_idempotent(&c, "request-1", b"{}", 3).unwrap());
    assert_eq!(
        claim_idempotent(&c, "request-1", 4).unwrap(),
        IdempotencyClaim::Completed(b"{}".to_vec())
    );
    assert!(!release_idempotent(&c, "request-1").unwrap());
}

#[test]
fn paired_transition_rolls_back_when_candidate_write_is_rejected() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    put_change_set(&c, "s", 1, &"a".repeat(64), b"{}", 1).unwrap();
    let result = update_change_set_and_put_candidate(
        &c,
        "s",
        1,
        1,
        &"b".repeat(64),
        b"updated",
        "candidate",
        &"d".repeat(64),
        &vec![b'x'; MAX_JSON_BYTES + 1],
        2,
    );
    assert!(result.is_err());
    assert_eq!(get_change_set(&c, "s").unwrap(), Some(b"{}".to_vec()));
}

#[test]
fn schema_94_migrates_revision_and_idempotency_atomically() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE agent_git_change_sets (
                    id TEXT PRIMARY KEY,
                    version INTEGER NOT NULL,
                    content_hash TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                CREATE TABLE agent_git_commit_candidates (
                    id TEXT PRIMARY KEY,
                    change_set_id TEXT NOT NULL,
                    diff_hash TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                PRAGMA user_version = 93;",
        )
        .unwrap();
    let transaction = connection.transaction().unwrap();
    crate::migrations::v094::apply(&transaction, 93).unwrap();
    transaction.commit().unwrap();

    let revision: String = connection
            .query_row(
                "SELECT dflt_value FROM pragma_table_info('agent_git_change_sets') WHERE name='revision'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(revision, "1");
    let idempotency_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_git_change_set_idempotency'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(idempotency_table, 1);
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 94);
}
