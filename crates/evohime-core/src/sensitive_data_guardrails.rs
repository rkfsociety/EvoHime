//! Core-owned, bounded sensitive-data detection and redaction.
//!
//! This module deliberately has no network, filesystem, or provider dependency.
//! It is used at every less-trusted boundary; callers receive the transformed
//! value and bounded metadata, never the detector's raw match values.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_JSON_DEPTH: usize = 16;
pub const MAX_JSON_NODES: usize = 512;
pub const STREAM_CARRY_CHARS: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Detector {
    Email,
    SecretToken,
    BearerToken,
    PrivateKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Redact,
    Mask,
    Hash,
    Block,
}

impl Action {
    fn rank(self) -> u8 {
        match self {
            Self::Block => 4,
            Self::Hash => 3,
            Self::Mask => 2,
            Self::Redact => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveDataRule {
    pub id: String,
    pub version: u32,
    pub detector: Detector,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub version: u32,
    pub destination: String,
    pub rules: Vec<SensitiveDataRule>,
    pub max_input_bytes: usize,
    pub max_json_depth: usize,
    pub max_json_nodes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub policy: Policy,
    pub policy_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionMetadata {
    pub contract_version: u32,
    pub policy_hash: String,
    pub destination: String,
    pub action: Option<Action>,
    pub rule_ids: Vec<String>,
    pub match_count: usize,
    pub blocked: bool,
    pub output_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionResult {
    pub value: String,
    pub metadata: RedactionMetadata,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GuardrailError {
    #[error("unsupported guardrail contract version")]
    UnsupportedVersion,
    #[error("invalid guardrail policy")]
    InvalidPolicy,
    #[error("guardrail input exceeds bound")]
    InputTooLarge,
    #[error("structured payload exceeds traversal bound")]
    StructuredPayloadTooLarge,
    #[error("sensitive data blocked by policy")]
    Blocked(RedactionMetadata),
    #[error("malformed structured payload")]
    MalformedStructuredPayload,
}

pub fn default_policy(destination: impl Into<String>) -> PolicySnapshot {
    let policy = Policy {
        version: CONTRACT_VERSION,
        destination: destination.into(),
        rules: vec![
            SensitiveDataRule {
                id: "email".into(),
                version: 1,
                detector: Detector::Email,
                action: Action::Mask,
            },
            SensitiveDataRule {
                id: "secret_token".into(),
                version: 1,
                detector: Detector::SecretToken,
                action: Action::Hash,
            },
            SensitiveDataRule {
                id: "bearer_token".into(),
                version: 1,
                detector: Detector::BearerToken,
                action: Action::Redact,
            },
            SensitiveDataRule {
                id: "private_key".into(),
                version: 1,
                detector: Detector::PrivateKey,
                action: Action::Block,
            },
        ],
        max_input_bytes: MAX_INPUT_BYTES,
        max_json_depth: MAX_JSON_DEPTH,
        max_json_nodes: MAX_JSON_NODES,
    };
    snapshot(policy).expect("default guardrail policy is valid")
}

pub fn snapshot(policy: Policy) -> Result<PolicySnapshot, GuardrailError> {
    if policy.version != CONTRACT_VERSION
        || policy.destination.is_empty()
        || policy.destination.len() > 128
        || policy.rules.is_empty()
        || policy.rules.len() > 32
        || policy.max_input_bytes == 0
        || policy.max_input_bytes > MAX_INPUT_BYTES
        || policy.max_json_depth == 0
        || policy.max_json_depth > MAX_JSON_DEPTH
        || policy.max_json_nodes == 0
        || policy.max_json_nodes > MAX_JSON_NODES
        || policy.rules.iter().any(|rule| {
            rule.version != CONTRACT_VERSION || rule.id.is_empty() || rule.id.len() > 64
        })
    {
        return Err(GuardrailError::InvalidPolicy);
    }
    let bytes = serde_json::to_vec(&policy).map_err(|_| GuardrailError::InvalidPolicy)?;
    let mut hasher = Sha256::new();
    hasher.update(b"evohime-sensitive-guardrails-v1\0");
    hasher.update(bytes);
    Ok(PolicySnapshot {
        policy,
        policy_hash: hex::encode(hasher.finalize()),
    })
}

pub fn redact_text(
    snapshot: &PolicySnapshot,
    input: &str,
) -> Result<RedactionResult, GuardrailError> {
    if input.len() > snapshot.policy.max_input_bytes {
        return Err(GuardrailError::InputTooLarge);
    }
    let mut matches = Vec::new();
    for rule in &snapshot.policy.rules {
        for (start, end) in find_matches(input, rule.detector) {
            matches.push((start, end, rule));
        }
    }
    matches.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.action.rank().cmp(&a.2.action.rank()))
    });
    let mut selected = Vec::new();
    for candidate in matches {
        if selected
            .iter()
            .any(|(start, end, _): &(usize, usize, &SensitiveDataRule)| {
                candidate.0 < *end && candidate.1 > *start
            })
        {
            continue;
        }
        selected.push(candidate);
    }
    selected.sort_by_key(|item| item.0);
    let action = selected
        .iter()
        .map(|item| item.2.action)
        .max_by_key(|action| action.rank());
    let rule_ids = selected
        .iter()
        .map(|item| item.2.id.clone())
        .collect::<Vec<_>>();
    let match_count = selected.len();
    let metadata = |blocked: bool, output_bytes: usize| RedactionMetadata {
        contract_version: CONTRACT_VERSION,
        policy_hash: snapshot.policy_hash.clone(),
        destination: snapshot.policy.destination.clone(),
        action,
        rule_ids: rule_ids.clone(),
        match_count,
        blocked,
        output_bytes,
    };
    if selected.iter().any(|item| item.2.action == Action::Block) {
        return Err(GuardrailError::Blocked(metadata(true, 0)));
    }
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    for (start, end, rule) in selected {
        output.push_str(&input[cursor..start]);
        let value = &input[start..end];
        match rule.action {
            Action::Redact => output.push_str(&format!("<redacted:{}>", rule.id)),
            Action::Mask => output.push_str(&mask(value)),
            Action::Hash => output.push_str(&format!("<sha256:{}>", sha256(value))),
            Action::Block => unreachable!(),
        }
        cursor = end;
    }
    output.push_str(&input[cursor..]);
    let output_bytes = output.len();
    let result = RedactionResult {
        value: output,
        metadata: metadata(false, output_bytes),
    };
    Ok(result)
}

pub fn redact_json(
    snapshot: &PolicySnapshot,
    value: &serde_json::Value,
) -> Result<(serde_json::Value, RedactionMetadata), GuardrailError> {
    fn visit(
        snapshot: &PolicySnapshot,
        value: &serde_json::Value,
        depth: usize,
        nodes: &mut usize,
        metadata: &mut Vec<RedactionMetadata>,
    ) -> Result<serde_json::Value, GuardrailError> {
        *nodes += 1;
        if *nodes > snapshot.policy.max_json_nodes || depth > snapshot.policy.max_json_depth {
            return Err(GuardrailError::StructuredPayloadTooLarge);
        }
        match value {
            serde_json::Value::String(text) => {
                let result = redact_text(snapshot, text)?;
                metadata.push(result.metadata);
                Ok(serde_json::Value::String(result.value))
            }
            serde_json::Value::Array(items) => Ok(serde_json::Value::Array(
                items
                    .iter()
                    .map(|item| visit(snapshot, item, depth + 1, nodes, metadata))
                    .collect::<Result<_, _>>()?,
            )),
            serde_json::Value::Object(items) => Ok(serde_json::Value::Object(
                items
                    .iter()
                    .map(|(key, item)| {
                        Ok((
                            key.clone(),
                            visit(snapshot, item, depth + 1, nodes, metadata)?,
                        ))
                    })
                    .collect::<Result<_, GuardrailError>>()?,
            )),
            other => Ok(other.clone()),
        }
    }
    let mut nodes = 0;
    let mut metadata = Vec::new();
    let value = visit(snapshot, value, 0, &mut nodes, &mut metadata)?;
    let mut rule_ids = metadata
        .iter()
        .flat_map(|item| item.rule_ids.clone())
        .collect::<Vec<_>>();
    rule_ids.sort();
    rule_ids.dedup();
    let match_count = metadata.iter().map(|item| item.match_count).sum();
    let output_bytes = serde_json::to_vec(&value)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    Ok((
        value,
        RedactionMetadata {
            contract_version: CONTRACT_VERSION,
            policy_hash: snapshot.policy_hash.clone(),
            destination: snapshot.policy.destination.clone(),
            action: metadata
                .iter()
                .filter_map(|item| item.action)
                .max_by_key(|action| action.rank()),
            rule_ids,
            match_count,
            blocked: false,
            output_bytes,
        },
    ))
}

pub struct StreamingRedactor {
    snapshot: PolicySnapshot,
    carry: String,
}

impl StreamingRedactor {
    pub fn new(snapshot: PolicySnapshot) -> Self {
        Self {
            snapshot,
            carry: String::new(),
        }
    }

    pub fn push_chunk(&mut self, chunk: &str) -> Result<RedactionResult, GuardrailError> {
        if self.carry.len().saturating_add(chunk.len()) > self.snapshot.policy.max_input_bytes {
            return Err(GuardrailError::InputTooLarge);
        }
        self.carry.push_str(chunk);
        let has_sensitive_prefix = ["@", "sk-", "ghp_", "xoxb-", "AKIA", "Bearer ", "-----BEGIN"]
            .iter()
            .any(|prefix| self.carry.contains(prefix));
        if !has_sensitive_prefix {
            let result = redact_text(&self.snapshot, &self.carry)?;
            self.carry.clear();
            return Ok(result);
        }
        if self.carry.ends_with(char::is_whitespace) {
            let result = redact_text(&self.snapshot, &self.carry)?;
            self.carry.clear();
            return Ok(result);
        }
        if self.carry.chars().count() <= STREAM_CARRY_CHARS {
            return Ok(empty_result(&self.snapshot));
        }
        let split = self
            .carry
            .char_indices()
            .nth(self.carry.chars().count() - STREAM_CARRY_CHARS)
            .map(|(index, _)| index)
            .unwrap_or(0);
        let stable = self.carry[..split].to_owned();
        self.carry = self.carry[split..].to_owned();
        redact_text(&self.snapshot, &stable)
    }

    pub fn finish(mut self) -> Result<RedactionResult, GuardrailError> {
        let result = redact_text(&self.snapshot, &self.carry)?;
        self.carry.clear();
        Ok(result)
    }
}

fn empty_result(snapshot: &PolicySnapshot) -> RedactionResult {
    RedactionResult {
        value: String::new(),
        metadata: RedactionMetadata {
            contract_version: CONTRACT_VERSION,
            policy_hash: snapshot.policy_hash.clone(),
            destination: snapshot.policy.destination.clone(),
            action: None,
            rule_ids: Vec::new(),
            match_count: 0,
            blocked: false,
            output_bytes: 0,
        },
    }
}
fn sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}
fn mask(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 4 {
        return "****".into();
    }
    format!(
        "{}{}{}",
        chars[..2].iter().collect::<String>(),
        "*".repeat(chars.len().saturating_sub(4)),
        chars[chars.len() - 2..].iter().collect::<String>()
    )
}

fn find_matches(input: &str, detector: Detector) -> Vec<(usize, usize)> {
    match detector {
        Detector::PrivateKey => input
            .find("-----BEGIN")
            .and_then(|start| {
                input[start..]
                    .find("PRIVATE KEY-----")
                    .map(|end| (start, start + end + "PRIVATE KEY-----".len()))
            })
            .into_iter()
            .collect(),
        Detector::BearerToken => input
            .match_indices("Bearer ")
            .map(|(start, _)| {
                let end = input[start..]
                    .find(char::is_whitespace)
                    .map(|offset| start + offset)
                    .unwrap_or(input.len());
                (start, end)
            })
            .filter(|(start, end)| end.saturating_sub(*start) > 7)
            .collect(),
        Detector::SecretToken => ["sk-", "ghp_", "xoxb-", "AKIA"]
            .iter()
            .flat_map(|prefix| {
                input.match_indices(prefix).map(move |(start, _)| {
                    let end = input[start..]
                        .find(char::is_whitespace)
                        .map(|offset| start + offset)
                        .unwrap_or(input.len());
                    (start, end)
                })
            })
            .collect(),
        Detector::Email => input
            .match_indices('@')
            .filter_map(|(at, _)| {
                let left = input[..at]
                    .rfind(char::is_whitespace)
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let right = input[at..]
                    .find(char::is_whitespace)
                    .map(|index| at + index)
                    .unwrap_or(input.len());
                if at > left && right > at + 1 && input[at + 1..right].contains('.') {
                    Some((left, right))
                } else {
                    None
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_hash_is_deterministic() {
        assert_eq!(
            default_policy("provider").policy_hash,
            default_policy("provider").policy_hash
        );
    }
    #[test]
    fn all_actions_are_supported() {
        let base = default_policy("test");
        for action in [Action::Redact, Action::Mask, Action::Hash] {
            let mut policy = base.policy.clone();
            policy.rules[0].action = action;
            let result = redact_text(&snapshot(policy).unwrap(), "mail user@example.com").unwrap();
            assert_ne!(result.value, "mail user@example.com");
        }
    }
    #[test]
    fn private_key_is_blocked() {
        let error =
            redact_text(&default_policy("provider"), "-----BEGIN PRIVATE KEY-----").unwrap_err();
        assert!(matches!(error, GuardrailError::Blocked(_)));
    }
    #[test]
    fn structured_payload_is_recursive() {
        let (value, metadata) = redact_json(
            &default_policy("provider"),
            &serde_json::json!({"nested":["user@example.com"]}),
        )
        .unwrap();
        assert_eq!(metadata.match_count, 1);
        assert_ne!(value["nested"][0], "user@example.com");
    }
    #[test]
    fn stream_detects_match_across_chunks() {
        let mut stream = StreamingRedactor::new(default_policy("provider"));
        assert!(stream.push_chunk("user@").unwrap().value.is_empty());
        let result = stream
            .push_chunk(&format!("{} ", "example.com".repeat(10)))
            .unwrap();
        let tail = stream.finish().unwrap();
        assert!(
            !result.value.contains("user@example.com") && !tail.value.contains("user@example.com")
        );
    }
    #[test]
    fn restart_is_ephemeral() {
        let first = StreamingRedactor::new(default_policy("one"));
        drop(first);
        let second = StreamingRedactor::new(default_policy("one"));
        assert!(second.carry.is_empty());
    }
}
