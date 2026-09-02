use evohime_core::schema_driven_agent_configuration::*;

#[test]
fn schema_contract_has_typed_layers_and_no_secret_value() {
    let schema = builtin_schema(ConfigurationScope::ApplicationDefaults);
    validate_schema(&schema).unwrap();
    assert_eq!(schema.version, 1);
    assert!(schema.fields.iter().any(|field| field.secret));
    let mut values = serde_json::Map::new();
    values.insert("model_profile".into(), serde_json::json!("model-a"));
    values.insert(
        "provider_credential".into(),
        serde_json::json!("credential-1"),
    );
    let snapshot = effective_snapshot(
        ConfigurationScope::ApplicationDefaults,
        &schema,
        1,
        &[("workspace", &values)],
    )
    .unwrap();
    assert!(!snapshot.values.contains_key("provider_credential"));
    assert_eq!(
        snapshot.secret_states["provider_credential"]["configured"],
        true
    );
    assert!(snapshot.effective_hash.starts_with("sha256:"));
}

#[test]
fn unknown_field_and_reference_are_rejected() {
    let schema = builtin_schema(ConfigurationScope::RunOverride);
    let unknown = ConfigurationPatch {
        kind: PatchKind::SetField,
        field: "executable".into(),
        value_json: Some(serde_json::json!("run")),
    };
    assert!(validate_patches(&schema, &[unknown]).is_err());
    let invalid_ref = ConfigurationPatch {
        kind: PatchKind::BindReference,
        field: "reasoning_effort".into(),
        value_json: Some(serde_json::json!("max")),
    };
    assert_eq!(
        validate_patches(&schema, &[invalid_ref]),
        Err(ConfigurationError::UnavailableReference)
    );
}
