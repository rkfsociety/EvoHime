use serde_json::{json, Value};

pub(crate) const MAX_REDACTION_DEPTH: usize = 64;

pub(crate) fn redact_value(value: Value, depth: usize) -> Value {
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
