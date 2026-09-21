use super::*;

fn record(scope: &str, revision: i64) -> FreeAccessEvidenceRecord {
    FreeAccessEvidenceRecord {
        provider_id: "openrouter".into(),
        model_id: "provider/model:free".into(),
        credential_binding: scope.into(),
        region: "global".into(),
        revision,
        content_hash: "a".repeat(64),
        evidence_json: br#"{"observed_state":"verified_free_limited"}"#.to_vec(),
        observed_at_ms: 1_000,
        expires_at_ms: 2_000,
        invalidation: None,
    }
}

#[test]
fn scoped_revision_write_is_fenced_and_idempotent() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    assert!(put(&connection, &record("cred:a", 1)).expect("first write"));
    assert!(!put(&connection, &record("cred:a", 1)).expect("duplicate write"));
    assert!(!put(&connection, &record("cred:a", 3)).expect("revision gap"));
    assert!(put(&connection, &record("cred:a", 2)).expect("next revision"));
    assert!(put(&connection, &record("cred:b", 1)).expect("other scope"));
    assert_eq!(count(&connection).expect("count"), 2);
}

#[test]
fn raw_provider_material_is_rejected_before_storage() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    let mut unsafe_record = record("cred:a", 1);
    unsafe_record.evidence_json =
        br#"{"provider_response":"https://provider.test","prompt":"private"}"#.to_vec();
    assert_eq!(
        put(&connection, &unsafe_record),
        Err("invalid free access evidence")
    );
    assert_eq!(count(&connection).expect("count"), 0);
}

#[test]
fn secret_like_scope_is_rejected() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    let unsafe_record = record("sk-live-secret", 1);
    assert_eq!(
        put(&connection, &unsafe_record),
        Err("invalid free access evidence")
    );
}
