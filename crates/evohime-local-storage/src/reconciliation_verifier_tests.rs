use super::*;

#[test]
fn file_hash_match_is_confirmed() {
    let result = verify_snapshot(
        "snapshot-file",
        &SnapshotOutcome::File(FileSnapshotOutcome {
            expected_hash: "aabb".into(),
            observed_hash: Some("aabb".into()),
            exists: true,
        }),
    )
    .unwrap();
    assert_eq!(result.status, VerificationStatus::Confirmed);
    assert_eq!(result.reason_code, "file_hash_match");
}

#[test]
fn mismatch_is_unconfirmed_without_retry() {
    let result = verify_snapshot(
        "snapshot-file",
        &SnapshotOutcome::File(FileSnapshotOutcome {
            expected_hash: "aabb".into(),
            observed_hash: Some("ccdd".into()),
            exists: true,
        }),
    )
    .unwrap();
    assert_eq!(result.status, VerificationStatus::Unconfirmed);
    assert_eq!(result.reason_code, "file_hash_mismatch");
}

#[test]
fn missing_database_evidence_is_blocked() {
    let result = verify_snapshot(
        "snapshot-db",
        &SnapshotOutcome::Database(DatabaseSnapshotOutcome {
            expected_schema_version: 6,
            observed_schema_version: Some(6),
            expected_content_hash: "aabb".into(),
            observed_content_hash: None,
        }),
    )
    .unwrap();
    assert_eq!(result.status, VerificationStatus::Blocked);
}

#[test]
fn process_generation_mismatch_is_unconfirmed() {
    let result = verify_snapshot(
        "snapshot-process",
        &SnapshotOutcome::Process(ProcessSnapshotOutcome {
            expected_generation: 2,
            observed_generation: Some(3),
            alive: true,
        }),
    )
    .unwrap();
    assert_eq!(result.status, VerificationStatus::Unconfirmed);
    assert_eq!(result.reason_code, "process_generation_mismatch");
}

#[test]
fn oversized_input_is_rejected() {
    let result = verify_snapshot(
        &"x".repeat(MAX_SNAPSHOT_ID_BYTES + 1),
        &SnapshotOutcome::File(FileSnapshotOutcome {
            expected_hash: "aabb".into(),
            observed_hash: Some("aabb".into()),
            exists: true,
        }),
    );
    assert_eq!(result, Err(VerificationError::SnapshotIdTooLong));
}

#[test]
fn verification_serialization_is_deterministic() {
    let outcome = SnapshotOutcome::Process(ProcessSnapshotOutcome {
        expected_generation: 4,
        observed_generation: Some(4),
        alive: true,
    });
    let first =
        serde_json::to_string(&verify_snapshot("snapshot-process", &outcome).unwrap()).unwrap();
    let second =
        serde_json::to_string(&verify_snapshot("snapshot-process", &outcome).unwrap()).unwrap();
    assert_eq!(first, second);
}
