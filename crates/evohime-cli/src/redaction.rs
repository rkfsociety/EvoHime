use serde_json::{json, Value};

use crate::redaction_policy::redact_value;
use crate::MAX_EVENT_BYTES;

/// Parses and recursively redacts a bounded JSON event payload.
///
/// Oversized and malformed input is replaced by a safe reason-code projection.
pub fn redact_payload(bytes: &[u8]) -> Value {
    if bytes.len() > MAX_EVENT_BYTES {
        return json!({"redacted":true,"reason_code":"event_too_large"});
    }
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return json!({"redacted":true,"reason_code":"non_json_projection"});
    };
    redact_value(value, 0)
}
