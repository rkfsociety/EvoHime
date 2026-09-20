use crate::redact_payload;
use crate::redaction_policy::{redact_value, MAX_REDACTION_DEPTH};
use serde_json::json;

#[test]
fn redacts_common_credential_aliases() {
    let value = redact_payload(
        br#"{"api_key":"a","password":"b","authorization":"c","cookie":"d","status":"done","nested":{"private_key":"e","ok":true}}"#,
    );
    assert_eq!(value, json!({"status":"done","nested":{"ok":true}}));
}

#[test]
fn redacts_sensitive_projection_keys() {
    let value = redact_payload(br#"{"prompt":"x","secret":"y","status":"done"}"#);
    assert_eq!(value, json!({"status":"done"}));
}

#[test]
fn bounds_nested_projection_depth() {
    let mut value = json!("safe");
    for _ in 0..=MAX_REDACTION_DEPTH {
        value = json!({"nested": value});
    }
    let redacted = redact_value(value, 0);
    let mut cursor = &redacted;
    for _ in 0..MAX_REDACTION_DEPTH {
        cursor = &cursor["nested"];
    }
    assert_eq!(cursor["redacted"], true);
    assert_eq!(cursor["reason_code"], "projection_depth_exceeded");
}
