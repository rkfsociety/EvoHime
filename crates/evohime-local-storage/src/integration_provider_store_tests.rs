use super::*;

#[test]
fn metadata_round_trips_without_secret_column() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    put_manifest(
        &connection,
        "fixture.echo",
        1,
        &serde_json::json!({"id":"fixture.echo"}),
        "hash",
        1,
    )
    .unwrap();
    let value: Option<serde_json::Value> = get_manifest(&connection, "fixture.echo", 1).unwrap();
    assert_eq!(value.unwrap()["id"], "fixture.echo");
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(integration_provider_credentials)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.iter().any(|column| column.contains("secret")));
}

#[test]
fn published_manifest_version_is_immutable_and_idempotent() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    put_manifest(
        &connection,
        "fixture.echo",
        1,
        &serde_json::json!({"revision":1}),
        "hash-1",
        1,
    )
    .unwrap();
    put_manifest(
        &connection,
        "fixture.echo",
        1,
        &serde_json::json!({"revision":2}),
        "hash-2",
        2,
    )
    .unwrap();
    let value: serde_json::Value = get_manifest(&connection, "fixture.echo", 1)
        .unwrap()
        .expect("manifest exists");
    assert_eq!(value["revision"], 1);
}

#[test]
fn dependency_report_treats_like_wildcards_as_literal_identifier_data() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO integration_provider_bindings
             (binding_id,owner_kind,owner_id,binding_json,status,version,updated_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                "binding-1",
                "task",
                "task-1",
                r#"{"credential_id":"cred%prod"}"#,
                "active",
                1_i64,
                1_i64
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO integration_provider_bindings
             (binding_id,owner_kind,owner_id,binding_json,status,version,updated_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                "binding-2",
                "task",
                "task-2",
                r#"{"credential_id":"credXprod"}"#,
                "active",
                1_i64,
                1_i64
            ],
        )
        .unwrap();
    assert_eq!(
        dependency_report(&connection, "cred%prod").unwrap(),
        vec![("task".to_owned(), "task-1".to_owned())]
    );
    for index in 0..300 {
        connection
            .execute(
                "INSERT INTO integration_provider_bindings
                 (binding_id,owner_kind,owner_id,binding_json,status,version,updated_at_ms)
                 VALUES (?1,'task',?2,?3,'active',1,1)",
                params![
                    format!("binding-extra-{index:03}"),
                    format!("task-extra-{index:03}"),
                    r#"{"credential_id":"cred%prod"}"#
                ],
            )
            .unwrap();
    }
    assert_eq!(
        dependency_report(&connection, "cred%prod").unwrap().len(),
        256
    );
}

#[test]
fn manifest_payload_is_bounded() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let oversized = "x".repeat(MAX_MANIFEST_BYTES);
    assert!(put_manifest(&connection, "fixture.echo", 1, &oversized, "hash", 1).is_err());
}
