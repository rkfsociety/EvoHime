use super::*;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE capability_manifests (
                    id TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    version TEXT NOT NULL,
                    risk_class TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    manifest_json BLOB NOT NULL,
                    installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
        )
        .expect("contract fixture creates");
}

fn record(id: &str, version: &str) -> CapabilityManifestRecord {
    CapabilityManifestRecord {
        id: id.into(),
        kind: ManifestKind::Role,
        version: version.into(),
        risk_class: "medium".into(),
        content_hash: "0123456789abcdef0123456789abcdef".into(),
        manifest_json: r#"{"name":"reviewer"}"#.into(),
    }
}

#[test]
fn round_trips_record_without_schema_migration() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let expected = record("reviewer", "1.0.0");

    CapabilityStoreSql::insert(&connection, &expected).expect("manifest inserts");

    assert_eq!(
        CapabilityStoreSql::get_by_id(&connection, "reviewer").expect("manifest reads"),
        Some(expected)
    );
}

#[test]
fn insert_upserts_by_id_for_updates() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    CapabilityStoreSql::insert(&connection, &record("reviewer", "1.0.0")).expect("insert v1");
    CapabilityStoreSql::insert(&connection, &record("reviewer", "2.0.0")).expect("insert v2");

    let all = CapabilityStoreSql::list(&connection, 10).expect("manifests list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].version, "2.0.0");
}

#[test]
fn lists_newest_first_and_deletes() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    CapabilityStoreSql::insert(&connection, &record("planner", "1.0.0")).expect("insert a");
    std::thread::sleep(std::time::Duration::from_millis(5));
    CapabilityStoreSql::insert(&connection, &record("reviewer", "1.0.0")).expect("insert b");

    let all = CapabilityStoreSql::list(&connection, 10).expect("manifests list");
    assert_eq!(
        all.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        ["reviewer", "planner"]
    );

    assert!(CapabilityStoreSql::delete_by_id(&connection, "planner").expect("delete"));
    assert!(!CapabilityStoreSql::delete_by_id(&connection, "missing").expect("missing delete"));
    assert_eq!(CapabilityStoreSql::list(&connection, 10).unwrap().len(), 1);
}

/// A rejected update must never touch the previously installed row: the
/// caller (`evohime_core`) fully validates -- including the Ed25519
/// signature-trust-root check and the hash check -- before this store
/// is ever called, and this store itself validates the record before
/// issuing the SQL write. Either gate failing must leave the prior
/// version installed and functional, which this test proves at the
/// storage layer: a record that fails `CapabilityManifestRecord::validate`
/// never reaches SQL, so the previously installed row survives untouched.
#[test]
fn failed_staged_update_leaves_prior_version_installed_and_functional() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let installed = record("reviewer", "1.0.0");
    CapabilityStoreSql::insert(&connection, &installed).expect("insert v1");

    let mut rejected_candidate = record("reviewer", "2.0.0");
    rejected_candidate.manifest_json = String::new();
    assert_eq!(
        CapabilityStoreSql::insert(&connection, &rejected_candidate),
        Err(CapabilityStoreError::Empty {
            field: "manifest_json"
        })
    );

    assert_eq!(
        CapabilityStoreSql::get_by_id(&connection, "reviewer").expect("manifest reads"),
        Some(installed)
    );
}

#[test]
fn rejects_unbounded_or_empty_contract_fields_before_sql() {
    let mut invalid = record("reviewer", "1.0.0");
    invalid.manifest_json = "x".repeat(MAX_MANIFEST_JSON_BYTES + 1);
    assert!(matches!(
        invalid.validate(),
        Err(CapabilityStoreError::Limit {
            field: "manifest_json",
            ..
        })
    ));

    invalid = record("reviewer", "1.0.0");
    invalid.id.clear();
    assert_eq!(
        invalid.validate(),
        Err(CapabilityStoreError::Empty { field: "id" })
    );
}
