//! Curated, stateless developer utilities.
//!
//! These operations deliberately live behind the normal ToolRegistry.  They
//! do not read the workspace, spawn processes, access the network or create
//! persistent state.  Every operation has a bounded JSON contract and returns
//! provenance-safe metadata alongside its value.

use crate::{ToolContext, ToolError, ToolResult};
use base64::{engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}, Engine};
use rand::RngCore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256, Sha512};
use std::time::Duration;
use uuid::Uuid;

pub const VERSION: &str = "1.0.0";
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;

pub const BASE64_ENCODE: &str = "utility.base64.encode";
pub const BASE64_DECODE: &str = "utility.base64.decode";
pub const HASH_SHA256: &str = "utility.hash.sha256";
pub const HASH_SHA512: &str = "utility.hash.sha512";
pub const JSON_FORMAT: &str = "utility.json.format";
pub const JSON_MINIFY: &str = "utility.json.minify";
pub const UUID_V4: &str = "utility.uuid.v4";
pub const TOKEN_GENERATE: &str = "utility.token.generate";
pub const TEXT_CASE: &str = "utility.text.case_convert";

pub const ALL_NAMES: &[&str] = &[
    BASE64_ENCODE, BASE64_DECODE, HASH_SHA256, HASH_SHA512, JSON_FORMAT, JSON_MINIFY, UUID_V4,
    TOKEN_GENERATE, TEXT_CASE,
];

pub const DESCRIPTION: &str =
    "Bounded local developer utility; no shell, filesystem, network or model call";
pub const PERMISSIONS: &[evohime_permissions::Permission] = &[];
pub const TIMEOUT: Duration = Duration::from_secs(2);

fn text_input<'a>(name: &str, input: &'a Value) -> Result<&'a str, ToolError> {
    let text = input.get("text").and_then(Value::as_str).ok_or_else(|| ToolError::InvalidInput {
        tool: name.into(),
        message: "text is required".into(),
    })?;
    if text.len() > MAX_INPUT_BYTES {
        return Err(ToolError::InvalidInput { tool: name.into(), message: "input_too_large".into() });
    }
    Ok(text)
}

fn result(name: &str, value: Value, output: String, kind: &str) -> Result<ToolResult, ToolError> {
    if output.len() > MAX_OUTPUT_BYTES {
        return Err(ToolError::InvalidInput { tool: name.into(), message: "output_too_large".into() });
    }
    Ok(ToolResult {
        output,
        structured: json!({
            "utility_id": name,
            "utility_version": VERSION,
            "implementation_revision": "developer-utilities-v1",
            "execution_kind": kind,
            "value": value,
        }),
    })
}

pub async fn execute(_ctx: &ToolContext, name: &str, input: Value) -> Result<ToolResult, ToolError> {
    match name {
        BASE64_ENCODE => {
            let text = text_input(name, &input)?;
            let value = STANDARD.encode(text.as_bytes());
            result(name, json!({"encoding":"base64","value":value}), value, "pure_deterministic")
        }
        BASE64_DECODE => {
            let text = text_input(name, &input)?;
            let bytes = STANDARD.decode(text).map_err(|_| ToolError::InvalidInput { tool: name.into(), message: "invalid_encoding".into() })?;
            let value = String::from_utf8(bytes).map_err(|_| ToolError::InvalidInput { tool: name.into(), message: "invalid_utf8".into() })?;
            result(name, json!({"encoding":"utf8","value":value}), value, "pure_deterministic")
        }
        HASH_SHA256 => {
            let text = text_input(name, &input)?;
            let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
            result(name, json!({"algorithm":"sha256","digest":digest}), digest, "pure_deterministic")
        }
        HASH_SHA512 => {
            let text = text_input(name, &input)?;
            let digest = format!("{:x}", Sha512::digest(text.as_bytes()));
            result(name, json!({"algorithm":"sha512","digest":digest}), digest, "pure_deterministic")
        }
        JSON_FORMAT | JSON_MINIFY => {
            let text = text_input(name, &input)?;
            let parsed: Value = serde_json::from_str(text).map_err(|error| ToolError::InvalidInput {
                tool: name.into(), message: format!("parse_error:{}", error.line()),
            })?;
            let value = if name == JSON_FORMAT {
                serde_json::to_string_pretty(&parsed).map_err(|error| ToolError::Execution(error.to_string()))?
            } else {
                serde_json::to_string(&parsed).map_err(|error| ToolError::Execution(error.to_string()))?
            };
            result(name, json!({"valid":true,"value":value}), value, "pure_deterministic")
        }
        UUID_V4 => {
            if !input.as_object().is_some_and(|object| object.is_empty()) {
                return Err(ToolError::InvalidInput { tool: name.into(), message: "no_options_allowed".into() });
            }
            let value = Uuid::new_v4().to_string();
            result(name, json!({"value":value,"random":true}), value, "secure_random")
        }
        TOKEN_GENERATE => {
            let length = input.get("bytes").and_then(Value::as_u64).unwrap_or(32);
            if !(1..=128).contains(&length) || input.as_object().map(|object| object.keys().any(|key| key != "bytes")).unwrap_or(true) {
                return Err(ToolError::InvalidInput { tool: name.into(), message: "invalid_byte_length".into() });
            }
            let mut bytes = vec![0; length as usize];
            rand::thread_rng().fill_bytes(&mut bytes);
            let value = URL_SAFE_NO_PAD.encode(bytes);
            result(name, json!({"value":value,"bytes":length,"telemetry":"redacted"}), value, "secure_random")
        }
        TEXT_CASE => {
            let text = text_input(name, &input)?;
            let mode = input.get("mode").and_then(Value::as_str).unwrap_or("lower");
            let value = match mode {
                "lower" => text.to_lowercase(),
                "upper" => text.to_uppercase(),
                "snake" => text.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join("_"),
                "kebab" => text.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join("-"),
                "camel" => text.split_whitespace().enumerate().map(|(index, word)| {
                    let mut chars = word.chars();
                    let first = chars.next().unwrap_or_default();
                    if index == 0 {
                        word.to_lowercase()
                    } else {
                        let first_upper: String = first.to_uppercase().collect();
                        format!("{}{}", first_upper, chars.as_str().to_lowercase())
                    }
                }).collect::<String>(),
                _ => return Err(ToolError::InvalidInput { tool: name.into(), message: "unsupported_mode".into() }),
            };
            result(name, json!({"mode":mode,"value":value}), value, "pure_deterministic")
        }
        _ => Err(ToolError::UnknownTool(name.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ToolContext {
        ToolContext { workspace_root: ".".into(), task_id: Uuid::new_v4(), session_id: None, progress_tx: None }
    }

    #[tokio::test]
    async fn pure_operations_are_stable_and_bounded() {
        let first = execute(&context(), HASH_SHA256, json!({"text":"hello"})).await.unwrap();
        let second = execute(&context(), HASH_SHA256, json!({"text":"hello"})).await.unwrap();
        assert_eq!(first.structured, second.structured);
        assert_eq!(first.output, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[tokio::test]
    async fn malformed_and_oversized_inputs_fail_closed() {
        assert!(execute(&context(), BASE64_DECODE, json!({"text":"%%%"})).await.is_err());
        assert!(execute(&context(), HASH_SHA256, json!({"text":"x".repeat(MAX_INPUT_BYTES + 1)})).await.is_err());
        assert!(execute(&context(), JSON_FORMAT, json!({"text":"{"})).await.is_err());
    }

    #[tokio::test]
    async fn random_outputs_are_marked_and_not_reused_as_deterministic() {
        let value = execute(&context(), TOKEN_GENERATE, json!({"bytes":16})).await.unwrap();
        assert_eq!(value.structured["execution_kind"], "secure_random");
        assert_eq!(value.structured["telemetry"], "redacted");
    }
}
