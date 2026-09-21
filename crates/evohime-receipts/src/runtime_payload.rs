use crate::runtime::{ActionRequest, MAX_PREVIEW_BYTES};
use crate::validate_uuid_v7;
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

pub(crate) fn redact_secrets(value: &str) -> String {
    const MARKERS: &[&str] = &[
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "authorization",
        "bearer",
    ];
    let mut out = String::with_capacity(value.len());
    let mut redact_next = false;
    for word in value.split_inclusive(char::is_whitespace) {
        let trailing_start = word.trim_end_matches(char::is_whitespace).len();
        let (core, trailing) = word.split_at(trailing_start);
        let lower = core.to_ascii_lowercase();
        let is_marker = MARKERS.iter().any(|marker| lower.contains(marker));
        if is_marker {
            // `key=value` (no space) redacts the value in place; a bare
            // marker word (optionally ending in `:`) redacts the token that
            // follows it, since the secret is a separate whitespace-split word.
            if let Some(delim) = core.find(['=', ':']) {
                if delim + 1 < core.len() {
                    out.push_str(&core[..=delim]);
                    out.push_str("[REDACTED]");
                } else {
                    out.push_str(core);
                    redact_next = true;
                }
            } else {
                out.push_str("[REDACTED]");
                redact_next = true;
            }
        } else if redact_next {
            out.push_str("[REDACTED]");
            redact_next = false;
        } else {
            out.push_str(core);
        }
        out.push_str(trailing);
    }
    out
}

pub(crate) fn bounded_preview(value: &str) -> String {
    let redacted = redact_secrets(value);
    let mut out = String::new();
    for ch in redacted.chars() {
        if out.len() + ch.len_utf8() > MAX_PREVIEW_BYTES.saturating_sub(11) {
            break;
        }
        out.push(ch);
    }
    if out.len() < redacted.len() {
        out.push_str("[truncated]");
    }
    out
}

pub(crate) fn build_payload(
    request: &ActionRequest,
    kind: &str,
    status: &str,
    args_hash: &str,
    previous: Option<&str>,
    result: Option<&str>,
    refusal: Option<&str>,
) -> Value {
    let mut payload = json!({
        "receipt_version": 1, "receipt_id": Uuid::now_v7().to_string(),
        "action_id": request.action_id.to_string(), "receipt_kind": kind,
        "action_status": status, "timestamp": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "task_id": request.task_id.clone(), "run_id": request.run_id.clone(), "tool_name": request.tool_name.clone(),
        "tool_args_hash": args_hash, "policy_id": request.policy_id.clone(),
        "policy_decision": request.policy_decision.as_str()
    });
    let object = payload.as_object_mut().expect("receipt payload is object");
    if let Some(value) = previous {
        object.insert("previous_receipt_hash".into(), Value::String(value.into()));
    }
    if let Some(value) = result {
        object.insert("result_hash".into(), Value::String(value.into()));
    }
    if let Some(value) = refusal {
        object.insert("refusal_code".into(), Value::String(value.into()));
    }
    if let Some(id) = request.approval_id {
        object.insert("approval_id".into(), Value::String(id.to_string()));
    }
    if kind != "post_action" {
        if let Some(parent) = request.parent_approval_ref.as_deref() {
            object.insert("parent_approval_ref".into(), Value::String(parent.into()));
        }
    }
    payload
}

/// Child handoffs carry only the two authenticated parent identifiers. A raw
/// parent payload or arbitrary renderer string is never a valid reference.
pub(crate) fn valid_parent_approval_ref(value: &str) -> bool {
    let mut parts = value.split(':');
    let Some(action_id) = parts.next() else {
        return false;
    };
    let Some(approval_id) = parts.next() else {
        return false;
    };
    parts.next().is_none() && validate_uuid_v7(action_id) && validate_uuid_v7(approval_id)
}

pub(crate) fn parent_approval_ids(value: &str) -> Option<(&str, &str)> {
    let mut parts = value.split(':');
    let action_id = parts.next()?;
    let approval_id = parts.next()?;
    if parts.next().is_some() || !validate_uuid_v7(action_id) || !validate_uuid_v7(approval_id) {
        return None;
    }
    Some((action_id, approval_id))
}
