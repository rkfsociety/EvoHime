use evohime_core::sensitive_data_guardrails::{
    default_policy, redact_json, redact_text, Action, GuardrailError, StreamingRedactor,
};

#[test]
fn pinned_policy_is_stable_and_restart_drops_stream_state() {
    let snapshot = default_policy("provider-a");
    let hash = snapshot.policy_hash.clone();
    let mut stream = StreamingRedactor::new(snapshot.clone());
    assert_eq!(stream.push_chunk("token ").unwrap().value, "token ");
    drop(stream);
    let mut fresh = StreamingRedactor::new(snapshot);
    assert_eq!(hash, default_policy("provider-a").policy_hash);
    assert_eq!(fresh.push_chunk("safe").unwrap().value, "safe");
}

#[test]
fn blocked_input_has_metadata_without_raw_value() {
    let error =
        redact_text(&default_policy("tool"), "-----BEGIN PRIVATE KEY-----secret").unwrap_err();
    let GuardrailError::Blocked(metadata) = error else {
        panic!("expected block")
    };
    assert!(metadata.blocked);
    assert!(!metadata.policy_hash.is_empty());
}

#[test]
fn recursive_projection_supports_hash_mask_and_redact() {
    let mut policy = default_policy("model").policy;
    policy.rules[0].action = Action::Redact;
    let snapshot = evohime_core::sensitive_data_guardrails::snapshot(policy).unwrap();
    let (value, metadata) =
        redact_json(&snapshot, &serde_json::json!({"a":["user@example.com"]})).unwrap();
    assert_eq!(metadata.match_count, 1);
    assert!(!value.to_string().contains("user@example.com"));
}
