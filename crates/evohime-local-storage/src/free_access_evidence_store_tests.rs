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

#[test]
fn credential_rotation_deletes_all_old_scoped_evidence_transactionally() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    let binding = "credential:01234567-89ab-4cde-8fab-0123456789ab";
    assert!(put(&connection, &record(binding, 1)).expect("write"));
    let mut second_model = record(binding, 1);
    second_model.model_id = "provider/another-model".into();
    second_model.region = "eu".into();
    assert!(put(&connection, &second_model).expect("write second model and region"));
    assert!(put(
        &connection,
        &record("credential:11234567-89ab-4cde-8fab-0123456789ab", 1)
    )
    .expect("write other credential"));

    assert_eq!(delete_credential_scope(&connection, binding), Ok(2));
    assert_eq!(count(&connection).expect("count"), 1);
    assert!(get(
        &connection,
        "openrouter",
        "provider/model:free",
        "credential:11234567-89ab-4cde-8fab-0123456789ab",
        "global"
    )
    .expect("other credential remains readable")
    .is_some());
    assert_eq!(delete_credential_scope(&connection, binding), Ok(0));
    assert_eq!(
        delete_credential_scope(&connection, "not a binding"),
        Err("invalid credential binding")
    );
}

#[test]
fn latest_observation_is_shared_across_models_but_not_credentials() {
    let connection = Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema");
    let binding = "credential:01234567-89ab-4cde-8fab-0123456789ab";
    let mut first_model = record(binding, 1);
    first_model.observed_at_ms = 1_000;
    first_model.expires_at_ms = 10_000;
    assert!(put(&connection, &first_model).expect("first model"));
    let mut second_model = record(binding, 1);
    second_model.model_id = "provider/another-model".into();
    second_model.observed_at_ms = 2_500;
    second_model.expires_at_ms = 10_000;
    assert!(put(&connection, &second_model).expect("second model"));
    let mut other_credential = record("credential:11234567-89ab-4cde-8fab-0123456789ab", 1);
    other_credential.observed_at_ms = 3_000;
    other_credential.expires_at_ms = 10_000;
    assert!(put(&connection, &other_credential).expect("other credential"));

    assert_eq!(
        latest_observed_at_for_credential(&connection, binding),
        Ok(Some(2_500))
    );
    assert_eq!(
        latest_observed_at_for_credential(
            &connection,
            "credential:21234567-89ab-4cde-8fab-0123456789ab"
        ),
        Ok(None)
    );
}
