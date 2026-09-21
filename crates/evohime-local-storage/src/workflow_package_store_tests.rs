use super::*;

#[test]
fn stores_metadata_without_package_bytes() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let record = PackageImportRecord {
        import_id: "import-1".into(),
        package_hash: "a".repeat(64),
        source_fingerprint: "b".repeat(64),
        local_workflow_id: "local-1".into(),
        local_workflow_version: 1,
        phase: ImportPhase::Pending,
        provenance_json: "{}".into(),
        redaction_summary_json: "{}".into(),
        updated_at_ms: 1,
    };
    insert_pending(&connection, &record).unwrap();
    assert!(find_committed_by_hash(&connection, &record.package_hash)
        .unwrap()
        .is_none());
    assert_eq!(list_pending(&connection, 10).unwrap().len(), 1);
    assert!(finish(&connection, "import-1", ImportPhase::Committed, 2).unwrap());
    assert!(find_committed_by_hash(&connection, &record.package_hash)
        .unwrap()
        .is_some());
}
