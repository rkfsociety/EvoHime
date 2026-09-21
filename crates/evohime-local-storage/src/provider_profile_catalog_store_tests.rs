use super::*;

fn record(scope: &str, revision: i64) -> ProviderProfileCatalogRecord {
    ProviderProfileCatalogRecord {
        provider_id: "openrouter".into(),
        credential_binding: scope.into(),
        region: "global".into(),
        revision,
        profile_content_hash: "a".repeat(64),
        profile_json: br#"{
              "schema_version":1,
              "provider_id":"openrouter",
              "transport":"openai_compatible",
              "endpoint":"https://openrouter.ai/api/v1",
              "region":"global",
              "credential_binding":"credential:openrouter",
              "revision":1
            }"#
        .to_vec(),
        catalog_content_hash: "b".repeat(64),
        catalog_json: br#"[{"model_id":"provider/model","limits":{"context_tokens":4096}}]"#
            .to_vec(),
        updated_at_ms: 1_000,
        state: "fresh".into(),
        observed_at_ms: 1_000,
        expires_at_ms: 2_000,
        failure_code: None,
    }
}

#[test]
fn scoped_revision_write_is_atomic_and_idempotent() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    assert!(put(&connection, &record("cred:a", 1)).expect("first write"));
    assert!(!put(&connection, &record("cred:a", 1)).expect("duplicate write"));
    assert!(!put(&connection, &record("cred:a", 3)).expect("revision gap"));
    assert!(put(&connection, &record("cred:a", 2)).expect("next revision"));
    assert!(put(&connection, &record("cred:b", 1)).expect("other scope"));
    assert_eq!(count(&connection).expect("count"), 2);
    assert_eq!(
        get(&connection, "openrouter", "cred:a", "global")
            .expect("read")
            .expect("snapshot")
            .revision,
        2
    );
}

#[test]
fn rejects_raw_catalog_material_and_invalid_profile_endpoint() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    let mut unsafe_record = record("credential:a", 1);
    unsafe_record.catalog_json =
        br#"[{"provider_response":"https://provider.test","prompt":"private"}]"#.to_vec();
    assert_eq!(
        put(&connection, &unsafe_record),
        Err("invalid provider profile catalog")
    );

    let mut bad_endpoint = record("credential:a", 1);
    bad_endpoint.profile_json = br#"{
          "provider_id":"openrouter",
          "endpoint":"https://provider.test/v1?api_key=secret"
        }"#
    .to_vec();
    assert_eq!(
        put(&connection, &bad_endpoint),
        Err("invalid provider profile catalog")
    );
    assert_eq!(count(&connection).expect("count"), 0);
}

#[test]
fn rejects_secret_like_scope_and_oversized_model_catalog() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    assert_eq!(
        put(&connection, &record("credential:sk-live", 1)),
        Err("invalid provider profile catalog")
    );

    let mut oversized = record("credential:a", 1);
    oversized.catalog_json = serde_json::to_vec(
        &(0..=MAX_CATALOG_ENTRIES)
            .map(|index| serde_json::json!({"model_id": format!("model-{index}")}))
            .collect::<Vec<_>>(),
    )
    .expect("catalog json");
    assert_eq!(
        put(&connection, &oversized),
        Err("invalid provider profile catalog")
    );
}
