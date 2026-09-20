use serde_json::{json, Value};

use crate::redaction_policy::redact_value;
use crate::MAX_EVENT_BYTES;

pub fn redact_payload(bytes: &[u8]) -> Value {
    if bytes.len() > MAX_EVENT_BYTES {
        return json!({"redacted":true,"reason_code":"event_too_large"});
    }
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return json!({"redacted":true,"reason_code":"non_json_projection"});
    };
    redact_value(value, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction_policy::MAX_REDACTION_DEPTH;

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
}
