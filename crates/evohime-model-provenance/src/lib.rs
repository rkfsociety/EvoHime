//! Канонический контракт model-request provenance (план 05).
//!
//! Этот crate намеренно не знает о SQLite, renderer или provider. Он содержит
//! только bounded logical envelope, JCS canonicalization, typed errors и
//! детерминированные хеши, поэтому те же bytes могут проверить Core и offline
//! verifier.

use evohime_receipts::{canonicalize_json_with_limits, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

pub const CONTRACT_VERSION: u32 = 1;
pub const MODEL_REQUEST_DOMAIN: &[u8] = b"evohime-model-request-v1\0";
pub const CONTEXT_PROJECTION_DOMAIN: &[u8] = b"evohime-context-projection-v1\0";
pub const MAX_REQUEST_ENVELOPE_BYTES: usize = 1_048_576;
pub const MAX_SYSTEM_PROMPT_BYTES: usize = 262_144;
pub const MAX_MESSAGE_BYTES: usize = 262_144;
pub const MAX_TOOL_SCHEMA_BYTES: usize = 262_144;
pub const MAX_TOOL_SET_BYTES: usize = 524_288;
pub const MAX_EVIDENCE_REFS: usize = 4096;
pub const MAX_SOURCE_REFS_PER_ENTRY: usize = 128;
pub const MAX_CONTEXT_PROJECTION_BYTES: usize = 262_144;
pub const MAX_PROVENANCE_DEPTH: usize = 128;
pub const MAX_SHADOW_BYTES_PER_TASK: usize = 8 * 1024 * 1024;
pub const PROVENANCE_RETENTION_DAYS: i64 = 90;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    #[error("REQUEST_PROVENANCE_TOO_LARGE")]
    TooLarge,
    #[error("REQUEST_PROVENANCE_INVALID: {0}")]
    Invalid(String),
    #[error("REQUEST_PROVENANCE_COMMIT_FAILED: {0}")]
    CommitFailed(String),
    #[error("REQUEST_SOURCE_MISSING")]
    SourceMissing,
    #[error("REQUEST_SOURCE_CHANGED")]
    SourceChanged,
    #[error("REQUEST_RECONSTRUCTION_FAILED")]
    ReconstructionFailed,
    #[error("REQUEST_HASH_MISMATCH")]
    HashMismatch,
    #[error("REQUEST_UNSUPPORTED_VERSION")]
    UnsupportedVersion,
    #[error("REQUEST_REDACTED")]
    Redacted,
    #[error("REQUEST_RETENTION_PRUNED")]
    RetentionPruned,
    #[error("REQUEST_LEDGER_MISMATCH")]
    LedgerMismatch,
    #[error("REQUEST_LINEAGE_MISMATCH")]
    LineageMismatch,
    #[error("REQUEST_RECEIPT_LINKAGE_MISMATCH")]
    ReceiptLinkageMismatch,
    #[error("REQUEST_TOOL_LINKAGE_MISMATCH")]
    ToolLinkageMismatch,
    #[error("REQUEST_SHADOW_CONTENT_COMPACTED")]
    ShadowContentCompacted,
    #[error("REQUEST_EVIDENCE_EVICTED")]
    EvidenceEvicted,
}

pub type Result<T> = std::result::Result<T, ProvenanceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Agent,
    PlanReview,
    PlanRevision,
    Memory,
    Child,
    Scheduled,
    Ambient,
    InternalSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Active,
    Completed,
    Failed,
    Interrupted,
    UnknownOutcome,
    Redacted,
    RetentionPruned,
}

impl RequestStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadMode {
    Full,
    HashOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub source_ref_id: String,
    pub source_kind: String,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    pub classification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionEntry {
    pub projection_entry_id: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<SourceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProjection {
    pub ledger_id: String,
    pub context_ledger_hash: String,
    pub entries: Vec<ProjectionEntry>,
    pub context_projection_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelParameters {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_output_tokens: Option<u32>,
    pub reasoning_mode: Option<String>,
    #[serde(default)]
    pub provider_options: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequestEnvelopeV1 {
    pub version: u32,
    pub request_id: String,
    pub logical_request_id: String,
    pub attempt: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_request_id: Option<String>,
    pub ledger_id: String,
    pub request_kind: RequestKind,
    pub provider: String,
    pub model: String,
    pub route_snapshot_hash: String,
    pub policy_snapshot_hash: String,
    pub route_policy_hash_shared: bool,
    pub system_prompt: String,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ToolSchema>,
    pub model_parameters: ModelParameters,
    pub context_projection: ContextProjection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_request_hash: Option<String>,
}

impl ModelRequestEnvelopeV1 {
    pub fn new_ids() -> (String, String) {
        (Uuid::now_v7().to_string(), Uuid::now_v7().to_string())
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != CONTRACT_VERSION {
            return Err(ProvenanceError::UnsupportedVersion);
        }
        if self.request_id.is_empty() || self.logical_request_id.is_empty() {
            return Err(ProvenanceError::Invalid("request identity is empty".into()));
        }
        if self.attempt == 0 {
            return Err(ProvenanceError::Invalid("attempt starts at one".into()));
        }
        if self.attempt == 1
            && (self.parent_request_id.is_some() || self.previous_request_hash.is_some())
        {
            return Err(ProvenanceError::LineageMismatch);
        }
        if self.attempt > 1
            && (self.parent_request_id.is_none() || self.previous_request_hash.is_none())
        {
            return Err(ProvenanceError::LineageMismatch);
        }
        if self.system_prompt.len() > MAX_SYSTEM_PROMPT_BYTES {
            return Err(ProvenanceError::TooLarge);
        }
        if self
            .messages
            .iter()
            .any(|m| m.content.len() > MAX_MESSAGE_BYTES)
        {
            return Err(ProvenanceError::TooLarge);
        }
        if self.tools.iter().any(|t| {
            serde_json::to_vec(t)
                .map(|bytes| bytes.len() > MAX_TOOL_SCHEMA_BYTES)
                .unwrap_or(true)
        }) {
            return Err(ProvenanceError::TooLarge);
        }
        let mut names: Vec<&str> = self.tools.iter().map(|tool| tool.name.as_str()).collect();
        names.sort_unstable();
        if names.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ProvenanceError::Invalid("duplicate tool name".into()));
        }
        let tool_bytes = canonical_json(&self.tools)?;
        if tool_bytes.len() > MAX_TOOL_SET_BYTES {
            return Err(ProvenanceError::TooLarge);
        }
        if self.route_snapshot_hash.is_empty() || self.policy_snapshot_hash.is_empty() {
            return Err(ProvenanceError::Invalid("snapshot hash is missing".into()));
        }
        if self.context_projection.ledger_id != self.ledger_id {
            return Err(ProvenanceError::LedgerMismatch);
        }
        if self.context_projection.entries.len() > MAX_EVIDENCE_REFS {
            return Err(ProvenanceError::TooLarge);
        }
        let source_count: usize = self
            .context_projection
            .entries
            .iter()
            .map(|entry| entry.source_refs.len())
            .sum();
        if source_count > MAX_EVIDENCE_REFS
            || self
                .context_projection
                .entries
                .iter()
                .any(|entry| entry.source_refs.len() > MAX_SOURCE_REFS_PER_ENTRY)
        {
            return Err(ProvenanceError::TooLarge);
        }
        if self.context_projection.context_projection_hash.is_empty() {
            return Err(ProvenanceError::Invalid(
                "projection hash is missing".into(),
            ));
        }
        if self.context_projection.compute_hash()?
            != self.context_projection.context_projection_hash
        {
            return Err(ProvenanceError::HashMismatch);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut value = serde_json::to_value(self)
            .map_err(|error| ProvenanceError::Invalid(error.to_string()))?;
        normalize_tools(&mut value)?;
        let bytes = canonical_json_value(&value)?;
        if bytes.len() > MAX_REQUEST_ENVELOPE_BYTES {
            return Err(ProvenanceError::TooLarge);
        }
        Ok(bytes)
    }

    pub fn envelope_hash(&self) -> Result<String> {
        let mut input = MODEL_REQUEST_DOMAIN.to_vec();
        input.extend(self.canonical_bytes()?);
        Ok(sha256_hex(&input))
    }
}

impl ContextProjection {
    pub fn from_ledger_parts(
        ledger_id: impl Into<String>,
        context_ledger_hash: impl Into<String>,
        selected_ids: impl IntoIterator<Item = String>,
        summaries: impl IntoIterator<Item = (String, Vec<SourceRef>)>,
        dropped: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self> {
        let mut entries = Vec::new();
        for id in selected_ids {
            entries.push(ProjectionEntry {
                projection_entry_id: id,
                operation: "include".into(),
                source_refs: Vec::new(),
                block_ref_id: None,
                drop_reason: None,
            });
        }
        for (id, refs) in summaries {
            entries.push(ProjectionEntry {
                projection_entry_id: id,
                operation: "summary".into(),
                source_refs: refs,
                block_ref_id: None,
                drop_reason: None,
            });
        }
        for (id, reason) in dropped {
            entries.push(ProjectionEntry {
                projection_entry_id: id,
                operation: "prune".into(),
                source_refs: Vec::new(),
                block_ref_id: None,
                drop_reason: Some(reason),
            });
        }
        let mut projection = Self {
            ledger_id: ledger_id.into(),
            context_ledger_hash: context_ledger_hash.into(),
            entries,
            context_projection_hash: String::new(),
        };
        projection.context_projection_hash = projection.compute_hash()?;
        Ok(projection)
    }

    pub fn compute_hash(&self) -> Result<String> {
        let mut coverage = self.clone();
        coverage.context_projection_hash.clear();
        let coverage = serde_json::to_value(&coverage)
            .map_err(|error| ProvenanceError::Invalid(error.to_string()))?;
        let coverage = canonical_json_value(&coverage)?;
        if coverage.len() > MAX_CONTEXT_PROJECTION_BYTES {
            return Err(ProvenanceError::TooLarge);
        }
        let mut input = CONTEXT_PROJECTION_DOMAIN.to_vec();
        input.extend(hex::decode_hash(&self.context_ledger_hash)?);
        input.extend(coverage);
        Ok(sha256_hex(&input))
    }
}

fn normalize_tools(value: &mut Value) -> Result<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProvenanceError::Invalid("envelope is not object".into()))?;
    let tools = object
        .get_mut("tools")
        .ok_or_else(|| ProvenanceError::Invalid("tools missing".into()))?;
    let array = tools
        .as_array_mut()
        .ok_or_else(|| ProvenanceError::Invalid("tools is not array".into()))?;
    array.sort_by(|left, right| {
        left.get("name")
            .and_then(Value::as_str)
            .cmp(&right.get("name").and_then(Value::as_str))
    });
    Ok(())
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value =
        serde_json::to_value(value).map_err(|error| ProvenanceError::Invalid(error.to_string()))?;
    canonical_json_value(&value)
}

fn canonical_json_value(value: &Value) -> Result<Vec<u8>> {
    canonicalize_json_with_limits(
        &serde_json::to_vec(value).map_err(|error| ProvenanceError::Invalid(error.to_string()))?,
        MAX_REQUEST_ENVELOPE_BYTES,
        MAX_PROVENANCE_DEPTH,
    )
    .map_err(|error| ProvenanceError::Invalid(error.to_string()))
}

mod hex {
    use super::ProvenanceError;
    pub fn decode_hash(value: &str) -> super::Result<Vec<u8>> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(ProvenanceError::Invalid(
                "hash must be lowercase sha256".into(),
            ));
        }
        (0..64)
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&value[index..index + 2], 16)
                    .map_err(|_| ProvenanceError::Invalid("invalid hash".into()))
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointState {
    Committed,
    DispatchMarked,
}

/// Payload variant для существующей signed receipt chain. Он содержит только
/// linkage и digest-и, поэтому receipt не становится вторым хранилищем prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestReceiptV1 {
    pub receipt_version: u32,
    pub payload_version: u32,
    pub receipt_domain: String,
    pub receipt_type: String,
    pub receipt_id: String,
    pub request_id: String,
    pub logical_request_id: String,
    pub attempt: u32,
    pub ledger_id: String,
    pub provider: String,
    pub model: String,
    pub request_envelope_hash: String,
    pub context_projection_hash: String,
    pub route_snapshot_hash: String,
    pub policy_snapshot_hash: String,
    pub previous_receipt_hash: Option<String>,
}

impl ModelRequestReceiptV1 {
    pub fn validate(&self) -> Result<()> {
        if self.receipt_version != 1 || self.payload_version != 1 {
            return Err(ProvenanceError::UnsupportedVersion);
        }
        if self.receipt_domain != "model_request" || self.receipt_type != "request_commit" {
            return Err(ProvenanceError::Invalid(
                "unknown request receipt variant".into(),
            ));
        }
        for hash in [
            &self.request_envelope_hash,
            &self.context_projection_hash,
            &self.route_snapshot_hash,
            &self.policy_snapshot_hash,
        ] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(ProvenanceError::Invalid("request receipt hash".into()));
            }
        }
        if self.attempt == 0 || self.request_id.is_empty() || self.ledger_id.is_empty() {
            return Err(ProvenanceError::Invalid("request receipt identity".into()));
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn digest(&self) -> Result<String> {
        Ok(sha256_hex(&self.canonical_bytes()?))
    }
}

pub fn context_projection_hash(ledger_hash: &str, content_coverage: &Value) -> Result<String> {
    let ledger_bytes = hex::decode_hash(ledger_hash)?;
    let coverage = canonical_json_value(content_coverage)?;
    let mut bytes = CONTEXT_PROJECTION_DOMAIN.to_vec();
    bytes.extend(ledger_bytes);
    bytes.extend(coverage);
    Ok(sha256_hex(&bytes))
}

pub fn is_secret_field(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "api_key"
            | "apikey"
            | "authorization"
            | "cookie"
            | "password"
            | "private_key"
            | "secret"
            | "token"
            | "access_token"
            | "refresh_token"
    )
}

pub fn validate_no_credentials(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_secret_field(key) {
                    return Err(ProvenanceError::Invalid(
                        "credential field is not model-visible provenance".into(),
                    ));
                }
                validate_no_credentials(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_no_credentials(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn canonical_args_hash(value: &Value) -> Result<String> {
    validate_no_credentials(value)?;
    Ok(sha256_hex(&canonical_json_value(value)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection() -> ContextProjection {
        let mut value = ContextProjection {
            ledger_id: "ledger".into(),
            context_ledger_hash: "a".repeat(64),
            entries: vec![ProjectionEntry {
                projection_entry_id: "item-1".into(),
                operation: "include".into(),
                source_refs: vec![],
                block_ref_id: Some("block-1".into()),
                drop_reason: None,
            }],
            context_projection_hash: String::new(),
        };
        value.context_projection_hash = value.compute_hash().unwrap();
        value
    }

    fn envelope() -> ModelRequestEnvelopeV1 {
        ModelRequestEnvelopeV1 {
            version: 1,
            request_id: Uuid::now_v7().to_string(),
            logical_request_id: "logical".into(),
            attempt: 1,
            parent_request_id: None,
            ledger_id: "ledger".into(),
            request_kind: RequestKind::Agent,
            provider: "mock".into(),
            model: "model".into(),
            route_snapshot_hash: "b".repeat(64),
            policy_snapshot_hash: "c".repeat(64),
            route_policy_hash_shared: false,
            system_prompt: "system".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            tools: vec![
                ToolSchema {
                    name: "z".into(),
                    description: "z".into(),
                    input_schema: serde_json::json!({"type":"object"}),
                },
                ToolSchema {
                    name: "a".into(),
                    description: "a".into(),
                    input_schema: serde_json::json!({"type":"object"}),
                },
            ],
            model_parameters: ModelParameters {
                temperature: None,
                top_p: None,
                max_output_tokens: Some(10),
                reasoning_mode: None,
                provider_options: Map::new(),
            },
            context_projection: projection(),
            previous_request_hash: None,
        }
    }

    #[test]
    fn canonical_hash_is_stable_and_tool_order_is_normalized() {
        let one = envelope();
        let mut two = one.clone();
        two.tools.reverse();
        assert_eq!(
            one.canonical_bytes().unwrap(),
            two.canonical_bytes().unwrap()
        );
        assert_eq!(one.envelope_hash().unwrap(), two.envelope_hash().unwrap());
    }

    #[test]
    fn retry_requires_lineage_and_hash_changes_with_attempt() {
        let one = envelope();
        let mut two = one.clone();
        two.attempt = 2;
        two.parent_request_id = Some(one.request_id.clone());
        two.previous_request_hash = Some(one.envelope_hash().unwrap());
        two.request_id = Uuid::now_v7().to_string();
        assert!(two.validate().is_ok());
        assert_ne!(one.envelope_hash().unwrap(), two.envelope_hash().unwrap());
    }

    #[test]
    fn secret_fields_are_rejected() {
        assert!(validate_no_credentials(&serde_json::json!({"authorization":"x"})).is_err());
    }

    #[test]
    fn duplicate_tools_are_rejected() {
        let mut value = envelope();
        value.tools[1].name = value.tools[0].name.clone();
        assert!(value.validate().is_err());
    }

    #[test]
    fn canonical_bytes_use_model_request_budget_not_receipt_budget() {
        let mut value = envelope();
        value.system_prompt = "x".repeat(9 * 1024);
        let bytes = value
            .canonical_bytes()
            .expect("model request envelopes may exceed receipt size");
        assert!(bytes.len() > 8192);
        assert!(bytes.len() <= MAX_REQUEST_ENVELOPE_BYTES);
    }

    #[test]
    fn known_answer_vector_is_stable() {
        let mut value = envelope();
        value.request_id = "00000000-0000-7000-8000-000000000001".into();
        value.logical_request_id = "logical-known-answer".into();
        assert_eq!(
            value.envelope_hash().unwrap(),
            "ca9dcbafac4fa5ca8006245326a606cbbc8439bd7cf2cec8f9ca07a8b3197a60"
        );
    }
}
