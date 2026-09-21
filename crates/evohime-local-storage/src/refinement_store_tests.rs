use super::*;

fn row() -> CandidateRow {
    CandidateRow {
        id: "c1".into(),
        revision: 1,
        owner_scope: "workspace:w1".into(),
        kind: "memory".into(),
        target: "memory".into(),
        status: "proposed".into(),
        pattern_key: "p1".into(),
        title: "bounded".into(),
        rationale: "r".into(),
        content_hash: "h".into(),
        confidence: 80,
        evidence_count: 2,
        conflict_count: 0,
        policy_snapshot_hash: "ph".into(),
        version: 0,
        idempotency_key: "i1".into(),
        error_code: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

#[test]
fn round_trip_and_optimistic_transition() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = RefinementStore::new(&connection);
    store
        .insert_candidate(InsertCandidateInput {
            row: &row(),
            content_json: "{}",
            source_task_ids_json: "[\"t1\"]",
            evidence_json: "[{} , {}]",
            conflicts_json: "[]",
        })
        .unwrap();
    assert_eq!(store.get("c1", 1).unwrap().unwrap().evidence_count, 2);
    let updated = store
        .transition_with_idempotency(TransitionWithIdempotencyInput {
            id: "c1",
            revision: 1,
            expected_version: 0,
            status: "approved",
            error_code: None,
            now_ms: 2,
            idempotency: Some(("action-1", "request-hash")),
        })
        .unwrap();
    assert_eq!(updated.version, 1);
    assert_eq!(
        store
            .replay_idempotency("workspace:w1", "action-1", "request-hash")
            .unwrap()
            .unwrap(),
        updated
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM refinement_events WHERE candidate_id = 'c1'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
    assert!(matches!(
        store.transition("c1", 1, 0, "active", None, 3),
        Err(RefinementStoreError::VersionConflict { .. })
    ));
}

#[test]
fn large_payload_is_rejected_before_write() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = RefinementStore::new(&connection);
    let value = "x".repeat(MAX_JSON_BYTES + 1);
    assert!(matches!(
        store.insert_candidate(InsertCandidateInput {
            row: &row(),
            content_json: &value,
            source_task_ids_json: "[]",
            evidence_json: "[]",
            conflicts_json: "[]",
        }),
        Err(RefinementStoreError::TooLarge)
    ));
}
