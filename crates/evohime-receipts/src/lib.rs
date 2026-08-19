//! Canonical Receipt v1 contract.  This crate deliberately exposes only typed
//! construction and bounded verification; runtime orchestration belongs to
//! later receipt stages.

use ring::signature;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use thiserror::Error;

pub mod key_lifecycle;
pub mod runtime;

pub const RECEIPT_VERSION: u64 = 1;
include!(concat!(env!("OUT_DIR"), "/receipt_limits.rs"));
pub const RESULT_DOMAIN: &[u8] = b"evohime-result-v1\0";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReceiptError {
    #[error("receipt.too_large")]
    TooLarge,
    #[error("receipt.payload_too_large")]
    PayloadTooLarge,
    #[error("receipt.invalid_utf8")]
    InvalidUtf8,
    #[error("receipt.invalid_json")]
    InvalidJson,
    #[error("receipt.duplicate_key")]
    DuplicateKey,
    #[error("receipt.unsupported_version")]
    UnsupportedVersion,
    #[error("receipt.schema_violation")]
    SchemaViolation,
    #[error("receipt.secret_field")]
    SecretField,
    #[error("receipt.non_canonical")]
    NonCanonical,
    #[error("receipt.key_unknown")]
    KeyUnknown,
    #[error("receipt.signature_invalid")]
    SignatureInvalid,
    #[error("receipt.hash_mismatch")]
    HashMismatch,
    #[error("receipt.chain_incomplete")]
    ChainIncomplete,
    #[error("receipt.timestamp_skew")]
    TimestampSkew,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub payload: Value,
    pub key_id: String,
    pub signature_algorithm: String,
    pub signature: String,
}

pub fn canonicalize_json(input: &[u8]) -> Result<Vec<u8>, ReceiptError> {
    if input.len() > MAX_ENVELOPE_BYTES {
        return Err(ReceiptError::TooLarge);
    }
    if std::str::from_utf8(input).is_err() {
        return Err(ReceiptError::InvalidUtf8);
    }
    let text = std::str::from_utf8(input).map_err(|_| ReceiptError::InvalidUtf8)?;
    if serde_json::from_str::<Value>(text).is_err() {
        return Err(ReceiptError::InvalidJson);
    }
    if has_duplicate_keys(text) {
        return Err(ReceiptError::DuplicateKey);
    }
    let value: Value = serde_json::from_str(text).map_err(|_| ReceiptError::InvalidJson)?;
    let mut out = String::new();
    write_jcs(&value, &mut out, 0)?;
    Ok(out.into_bytes())
}

fn write_jcs(value: &Value, out: &mut String, depth: usize) -> Result<(), ReceiptError> {
    if depth > MAX_DEPTH {
        return Err(ReceiptError::SchemaViolation);
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::Number(v) => out.push_str(&v.to_string()),
        Value::String(v) => {
            out.push_str(&serde_json::to_string(v).map_err(|_| ReceiptError::InvalidJson)?)
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_jcs(item, out, depth + 1)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, (key, item)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).map_err(|_| ReceiptError::InvalidJson)?);
                out.push(':');
                write_jcs(item, out, depth + 1)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn has_duplicate_keys(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut stack: Vec<HashSet<String>> = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            let is_key = bytes
                .get(i..)
                .unwrap_or_default()
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .and_then(|p| bytes.get(i + p))
                .copied()
                == Some(b':');
            if is_key {
                let key: String = serde_json::from_str(&input[start..i]).unwrap_or_default();
                if let Some(keys) = stack.last_mut() {
                    if !keys.insert(key) {
                        return true;
                    }
                }
            }
        } else if bytes[i] == b'{' {
            stack.push(HashSet::new());
            i += 1;
        } else if bytes[i] == b'}' {
            stack.pop();
            i += 1;
        } else {
            i += 1;
        }
    }
    false
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn receipt_hash(envelope: &Envelope) -> Result<String, ReceiptError> {
    let bytes = serde_json::to_vec(envelope).map_err(|_| ReceiptError::InvalidJson)?;
    let canonical = canonicalize_json(&bytes)?;
    if canonical.len() > MAX_ENVELOPE_BYTES {
        return Err(ReceiptError::TooLarge);
    }
    Ok(sha256_hex(&canonical))
}

pub fn payload_bytes(payload: &Value) -> Result<Vec<u8>, ReceiptError> {
    let raw = serde_json::to_vec(payload).map_err(|_| ReceiptError::InvalidJson)?;
    let canonical = canonicalize_json(&raw)?;
    if canonical.len() > MAX_PAYLOAD_BYTES {
        return Err(ReceiptError::PayloadTooLarge);
    }
    Ok(canonical)
}

pub fn verify_ed25519(envelope: &Envelope, public_key: &[u8]) -> Result<(), ReceiptError> {
    if envelope.signature_algorithm != "Ed25519" {
        return Err(ReceiptError::SchemaViolation);
    }
    validate_payload_v1(&envelope.payload)?;
    let payload = payload_bytes(&envelope.payload)?;
    let signature_bytes =
        decode_base64url(&envelope.signature).ok_or(ReceiptError::SignatureInvalid)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&payload, &signature_bytes)
        .map_err(|_| ReceiptError::SignatureInvalid)
}

/// Runtime v1 verification: the Ed25519 message is the raw SHA-256 digest of
/// canonical payload bytes.  `verify_ed25519` remains available for the
/// contract vectors whose historical signature message is canonical payload.
pub fn verify_runtime_signature(envelope: &Envelope, public_key: &[u8]) -> Result<(), ReceiptError> {
    if envelope.signature_algorithm != "Ed25519" { return Err(ReceiptError::SchemaViolation); }
    validate_payload_v1(&envelope.payload)?;
    let digest = Sha256::digest(payload_bytes(&envelope.payload)?);
    let signature_bytes = decode_base64url(&envelope.signature).ok_or(ReceiptError::SignatureInvalid)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&digest, &signature_bytes)
        .map_err(|_| ReceiptError::SignatureInvalid)
}

fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    if value.contains('=')
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() * 3 / 4 + 3);
    let mut acc = 0u32;
    let mut bits = 0u8;
    for b in value.bytes() {
        let n = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | n;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
        }
    }
    if bits > 0 && (acc & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(bytes)
}

pub fn result_hash(projection: &Value) -> Result<String, ReceiptError> {
    let canonical =
        canonicalize_json(&serde_json::to_vec(projection).map_err(|_| ReceiptError::InvalidJson)?)?;
    let mut bytes = RESULT_DOMAIN.to_vec();
    bytes.extend(canonical);
    Ok(sha256_hex(&bytes))
}

pub fn validate_typed_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 1
        && bytes.len() <= MAX_IDENTIFIER_BYTES
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

pub fn validate_payload_v1(payload: &Value) -> Result<(), ReceiptError> {
    let object = payload.as_object().ok_or(ReceiptError::SchemaViolation)?;
    if object.get("receipt_version") != Some(&Value::from(1)) {
        return Err(ReceiptError::UnsupportedVersion);
    }
    let allowed = [
        "receipt_version",
        "receipt_id",
        "action_id",
        "receipt_kind",
        "action_status",
        "refusal_code",
        "timestamp",
        "task_id",
        "run_id",
        "tool_name",
        "tool_args_hash",
        "result_hash",
        "policy_id",
        "policy_decision",
        "approval_id",
        "parent_approval_ref",
        "previous_receipt_hash",
        "context_ledger_hash",
        "model_route",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ReceiptError::SchemaViolation);
    }
    for key in object.keys() {
        if key.eq_ignore_ascii_case("secret")
            || [
                "token",
                "password",
                "api_key",
                "apikey",
                "authorization",
                "cookie",
                "private_key",
            ]
            .iter()
            .any(|part| key.to_ascii_lowercase().contains(part))
        {
            return Err(ReceiptError::SecretField);
        }
    }
    let required = [
        "receipt_id",
        "action_id",
        "receipt_kind",
        "action_status",
        "timestamp",
        "task_id",
        "run_id",
    ];
    if required.iter().any(|key| !object.contains_key(*key)) {
        return Err(ReceiptError::SchemaViolation);
    }
    for key in ["receipt_id", "action_id"] {
        if !object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(validate_uuid_v7)
        {
            return Err(ReceiptError::SchemaViolation);
        }
    }
    for key in ["task_id", "run_id", "tool_name", "policy_id"] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            if !validate_typed_identifier(value) {
                return Err(ReceiptError::SchemaViolation);
            }
        }
    }
    let kind = object
        .get("receipt_kind")
        .and_then(Value::as_str)
        .ok_or(ReceiptError::SchemaViolation)?;
    let status = object
        .get("action_status")
        .and_then(Value::as_str)
        .ok_or(ReceiptError::SchemaViolation)?;
    if !matches!(
        (kind, status),
        ("pre_action", "prepared")
            | ("post_action", "succeeded" | "failed" | "cancelled")
            | ("refusal", "refused")
    ) {
        return Err(ReceiptError::SchemaViolation);
    }
    if kind == "refusal" && object.get("refusal_code").and_then(Value::as_str).is_none() {
        return Err(ReceiptError::SchemaViolation);
    }
    if kind != "refusal" && object.contains_key("refusal_code") {
        return Err(ReceiptError::SchemaViolation);
    }
    if kind == "post_action" && !object.contains_key("result_hash") {
        return Err(ReceiptError::SchemaViolation);
    }
    if let Some(decision) = object.get("policy_decision").and_then(Value::as_str) {
        if !matches!(decision, "allow" | "deny" | "approval_required" | "allowed") {
            return Err(ReceiptError::SchemaViolation);
        }
    } else {
        return Err(ReceiptError::SchemaViolation);
    }
    if let Some(approval_id) = object.get("approval_id") {
        if !approval_id.as_str().is_some_and(validate_uuid_v7) {
            return Err(ReceiptError::SchemaViolation);
        }
    }
    if kind == "post_action" && object.contains_key("parent_approval_ref") {
        return Err(ReceiptError::SchemaViolation);
    }
    if object
        .get("timestamp")
        .and_then(Value::as_str)
        .is_none_or(|value| !valid_timestamp(value))
    {
        return Err(ReceiptError::SchemaViolation);
    }
    for key in [
        "tool_args_hash",
        "result_hash",
        "previous_receipt_hash",
        "context_ledger_hash",
    ] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            if !is_hash(value) {
                return Err(ReceiptError::SchemaViolation);
            }
        }
    }
    if matches!(kind, "pre_action" | "post_action" | "refusal")
        && (!object.contains_key("tool_name")
            || !object.contains_key("tool_args_hash")
            || !object.contains_key("policy_id")
            || !object.contains_key("policy_decision"))
    {
        return Err(ReceiptError::SchemaViolation);
    }
    if let Some(route) = object.get("model_route") {
        let route = route.as_object().ok_or(ReceiptError::SchemaViolation)?;
        if route.len() != 2
            || !route
                .get("provider_id")
                .and_then(Value::as_str)
                .is_some_and(validate_typed_identifier)
            || !route
                .get("model_id")
                .and_then(Value::as_str)
                .is_some_and(validate_typed_identifier)
        {
            return Err(ReceiptError::SchemaViolation);
        }
    }
    Ok(())
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_timestamp(value: &str) -> bool {
    value.len() == 24
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
        && value.as_bytes().get(19) == Some(&b'.')
        && value.ends_with('Z')
        && value
            .bytes()
            .enumerate()
            .all(|(i, byte)| [4, 7, 10, 13, 16, 19, 23].contains(&i) || byte.is_ascii_digit())
}

fn validate_uuid_v7(value: &str) -> bool {
    value.len() == 36
        && value.as_bytes().iter().enumerate().all(|(i, byte)| {
            if [8, 13, 18, 23].contains(&i) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
        && value.as_bytes().get(14) == Some(&b'7')
        && matches!(value.as_bytes().get(19), Some(b'8' | b'9' | b'a' | b'b'))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorts_utf16_code_units() {
        assert_eq!(
            canonicalize_json(br#"{"\uD800\uDC00":1,"\uE000":2}"#).unwrap(),
            "{\"𐀀\":1,\"\":2}".as_bytes()
        );
    }
    #[test]
    fn rejects_duplicate_keys() {
        assert_eq!(
            canonicalize_json(br#"{"a":1,"a":2}"#),
            Err(ReceiptError::DuplicateKey)
        );
    }
    #[test]
    fn fingerprint_input_version_matches_permissions_crate() {
        assert_eq!(
            FINGERPRINT_INPUT_VERSION,
            evohime_permissions::FINGERPRINT_INPUT_VERSION,
            "contracts/receipts/v1/limits.json fingerprint_input_version must stay in sync with evohime_permissions::FINGERPRINT_INPUT_VERSION"
        );
    }
    #[test]
    fn result_uses_domain() {
        assert_ne!(
            result_hash(&serde_json::json!({"status":"succeeded"})).unwrap(),
            sha256_hex(br#"{"status":"succeeded"}"#)
        );
    }
    #[test]
    fn shared_positive_vector_matches_bytes_hash_and_signature() {
        let vector: Value =
            serde_json::from_str(include_str!("../../../contracts/receipts/v1/vectors.json"))
                .unwrap();
        let positive = &vector["positive"][0];
        let payload = positive["payload"].clone();
        let payload_bytes = payload_bytes(&payload).unwrap();
        assert_eq!(
            hex::encode(&payload_bytes),
            positive["canonical_payload_hex"]
        );
        assert_eq!(sha256_hex(&payload_bytes), positive["payload_sha256_hex"]);
        let envelope = Envelope {
            payload,
            key_id: positive["key_id"].as_str().unwrap().into(),
            signature_algorithm: "Ed25519".into(),
            signature: positive["signature"].as_str().unwrap().into(),
        };
        let envelope_bytes = canonicalize_json(&serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert_eq!(
            hex::encode(envelope_bytes),
            positive["canonical_envelope_hex"]
        );
        assert_eq!(
            receipt_hash(&envelope).unwrap(),
            positive["receipt_hash_hex"]
        );
        let key = decode_base64url(positive["public_key_base64url"].as_str().unwrap()).unwrap();
        verify_ed25519(&envelope, &key).unwrap();
    }
}
