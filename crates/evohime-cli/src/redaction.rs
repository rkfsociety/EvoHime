use serde_json::{json, Value};

use crate::MAX_EVENT_BYTES;

const MAX_REDACTION_DEPTH: usize = 64;

pub fn redact_payload(bytes: &[u8]) -> Value {
    if bytes.len() > MAX_EVENT_BYTES {
        return json!({"redacted":true,"reason_code":"event_too_large"});
    }
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return json!({"redacted":true,"reason_code":"non_json_projection"});
    };
    redact_value(value, 0)
}

fn redact_value(value: Value, depth: usize) -> Value {
    if depth >= MAX_REDACTION_DEPTH {
        return json!({"redacted":true,"reason_code":"projection_depth_exceeded"});
    }
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    if is_sensitive_key(&lower) {
                        None
                    } else {
                        Some((key, redact_value(value, depth + 1)))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|value| redact_value(value, depth + 1))
                .collect(),
        ),
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    key.contains("secret")
        || key.contains("credential")
        || key.contains("prompt")
        || key.contains("reasoning")
        || key.contains("token")
        || key == "raw_output"
        || key.contains("password")
        || key.contains("api_key")
        || key.contains("access_key")
        || key.contains("private_key")
        || key.contains("authorization")
        || key.contains("cookie")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_credential_aliases() {
        let value = redact_payload(
            br#"{"api_key":"a","password":"b","authorization":"c","cookie":"d","status":"done","nested":{"private_key":"e","ok":true}}"#,
        );
        assert_eq!(value, json!({"status":"done","nested":{"ok":true}}));
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
}
