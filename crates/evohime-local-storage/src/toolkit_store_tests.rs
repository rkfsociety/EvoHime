use super::*;

fn record(version: &str) -> ToolkitRecord {
    ToolkitRecord {
        toolkit_id: "builtin.filesystem".into(),
        version: version.into(),
        manifest_hash: format!("sha256:{version}"),
        source: "builtin".into(),
        package_hash: None,
        license: Some("MIT".into()),
        status: "available".into(),
        compatible_core: ">=0.1".into(),
        manifest_json: br#"{"kind":"tool/manifest/v1"}"#.to_vec(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[test]
fn lifecycle_survives_reopen_and_records_rollback_history() {
    let path =
        std::env::temp_dir().join(format!("evohime-toolkit-store-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let connection = Connection::open(&path).unwrap();
    install_schema(&connection).unwrap();
    discover(&connection, &record("1.0.0")).unwrap();
    discover(&connection, &record("2.0.0")).unwrap();
    transition(
        &connection,
        "builtin.filesystem",
        "1.0.0",
        "enabled",
        "initial enable",
    )
    .unwrap();
    transition(
        &connection,
        "builtin.filesystem",
        "1.0.0",
        "disabled",
        "rollback",
    )
    .unwrap();
    assert_eq!(list(&connection, 10).unwrap().len(), 2);
    assert_eq!(
        audit(&connection, "builtin.filesystem", 10).unwrap().len(),
        2
    );
    drop(connection);
    let reopened = Connection::open(&path).unwrap();
    assert_eq!(
        list(&reopened, 10)
            .unwrap()
            .iter()
            .find(|r| r.version == "1.0.0")
            .unwrap()
            .status,
        "disabled"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn quarantine_cannot_become_enabled_implicitly() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    discover(&connection, &record("1.0.0")).unwrap();
    transition(
        &connection,
        "builtin.filesystem",
        "1.0.0",
        "quarantined",
        "hash mismatch",
    )
    .unwrap();
    assert!(transition(
        &connection,
        "builtin.filesystem",
        "1.0.0",
        "enabled",
        "enable"
    )
    .is_err());
}

#[test]
fn rediscovery_does_not_replace_same_version_metadata() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    discover(&connection, &record("1.0.0")).unwrap();
    let replacement = ToolkitRecord {
        manifest_hash: "sha256:tampered".into(),
        package_hash: Some("sha256:tampered".into()),
        manifest_json: br#"{"tampered":true}"#.to_vec(),
        ..record("1.0.0")
    };
    discover(&connection, &replacement).unwrap();
    let stored = list(&connection, 10).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].manifest_hash, "sha256:1.0.0");
    assert_eq!(stored[0].manifest_json, record("1.0.0").manifest_json);
}
