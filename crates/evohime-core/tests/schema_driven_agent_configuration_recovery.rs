use evohime_core::schema_driven_agent_configuration::*;

#[test]
fn effective_snapshot_is_immutable_by_revision_and_reproducible() {
    let schema = builtin_schema(ConfigurationScope::ConversationDefaults);
    let mut first = serde_json::Map::new();
    first.insert("reasoning_effort".into(), serde_json::json!("high"));
    let a = effective_snapshot(
        ConfigurationScope::ConversationDefaults,
        &schema,
        4,
        &[("conversation", &first)],
    )
    .unwrap();
    let mut second = first.clone();
    second.insert("reasoning_effort".into(), serde_json::json!("low"));
    let b = effective_snapshot(
        ConfigurationScope::ConversationDefaults,
        &schema,
        5,
        &[("conversation", &second)],
    )
    .unwrap();
    assert_ne!(a.effective_hash, b.effective_hash);
    assert_eq!(a.values["reasoning_effort"], "high");
}
