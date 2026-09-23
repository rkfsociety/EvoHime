#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![deny(missing_docs)]
//! Канонический контракт model-request provenance (план 05).
//!
//! Этот crate намеренно не знает о SQLite, renderer или provider. Он содержит
//! только bounded logical envelope, JCS canonicalization, typed errors и
//! детерминированные хеши, поэтому те же bytes могут проверить Core и offline
//! verifier.
//!
//! A projection digest is derived from a ledger digest and bounded entries:
//!
//! ```
//! use evohime_model_provenance::ContextProjection;
//!
//! let projection = ContextProjection::from_ledger_parts(
//!     "ledger-1",
//!     "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
//!     ["item-1".to_owned()],
//!     std::iter::empty(),
//!     std::iter::empty(),
//! ).unwrap();
//! assert_eq!(projection.compute_hash().unwrap(), projection.context_projection_hash);
//! ```

use evohime_receipts::{canonicalize_json_with_limits, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

/// Версия контракта provenance запроса модели.
pub const CONTRACT_VERSION: u32 = 1;
/// Domain-разделитель для хеша model-request envelope.
pub const MODEL_REQUEST_DOMAIN: &[u8] = b"evohime-model-request-v1\0";
/// Domain-разделитель для хеша проекции контекста.
pub const CONTEXT_PROJECTION_DOMAIN: &[u8] = b"evohime-context-projection-v1\0";
/// Максимальный canonical-размер сериализованного envelope.
pub const MAX_REQUEST_ENVELOPE_BYTES: usize = 1_048_576;
/// Максимальный размер system prompt в UTF-8 bytes.
pub const MAX_SYSTEM_PROMPT_BYTES: usize = 262_144;
/// Максимальный размер одного сообщения в UTF-8 bytes.
pub const MAX_MESSAGE_BYTES: usize = 262_144;
/// Максимальный сериализованный размер одной tool schema.
pub const MAX_TOOL_SCHEMA_BYTES: usize = 262_144;
/// Максимальный canonical-размер полного набора tool schemas.
pub const MAX_TOOL_SET_BYTES: usize = 524_288;
/// Максимальное суммарное число записей и source refs в проекции.
pub const MAX_EVIDENCE_REFS: usize = 4096;
/// Максимальное количество source refs одной записи проекции.
pub const MAX_SOURCE_REFS_PER_ENTRY: usize = 128;
/// Максимальный canonical-размер сериализованной проекции контекста.
pub const MAX_CONTEXT_PROJECTION_BYTES: usize = 262_144;
/// Максимальная глубина вложения canonical JSON.
pub const MAX_PROVENANCE_DEPTH: usize = 128;
/// Максимальный объём сохраняемого shadow content на одну задачу.
pub const MAX_SHADOW_BYTES_PER_TASK: usize = 8 * 1024 * 1024;
/// Срок хранения request provenance в днях.
pub const PROVENANCE_RETENTION_DAYS: i64 = 90;

/// Ошибка валидации, canonicalization или целостности provenance.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    /// Превышен установленный предел размера или глубины.
    #[error("REQUEST_PROVENANCE_TOO_LARGE")]
    TooLarge,
    /// Вход повреждён или нарушает контракт provenance.
    #[error("REQUEST_PROVENANCE_INVALID: {0}")]
    Invalid(String),
    /// Не удалось durable commit provenance.
    #[error("REQUEST_PROVENANCE_COMMIT_FAILED: {0}")]
    CommitFailed(String),
    /// Ссылка на исходную запись больше не существует.
    #[error("REQUEST_SOURCE_MISSING")]
    SourceMissing,
    /// Исходная запись изменилась после фиксации запроса.
    #[error("REQUEST_SOURCE_CHANGED")]
    SourceChanged,
    /// Невозможно восстановить запрос из сохранённых источников.
    #[error("REQUEST_RECONSTRUCTION_FAILED")]
    ReconstructionFailed,
    /// Вычисленный digest не совпал с сохранённым.
    #[error("REQUEST_HASH_MISMATCH")]
    HashMismatch,
    /// Версия payload не поддерживается.
    #[error("REQUEST_UNSUPPORTED_VERSION")]
    UnsupportedVersion,
    /// Необходимое содержимое запроса удалено redaction-политикой.
    #[error("REQUEST_REDACTED")]
    Redacted,
    /// Provenance удалён политикой retention.
    #[error("REQUEST_RETENTION_PRUNED")]
    RetentionPruned,
    /// Связь запроса с durable ledger не совпала.
    #[error("REQUEST_LEDGER_MISMATCH")]
    LedgerMismatch,
    /// Нарушена цепочка parent, attempt или предыдущего digest.
    #[error("REQUEST_LINEAGE_MISMATCH")]
    LineageMismatch,
    /// Signed receipt не связан с этим запросом.
    #[error("REQUEST_RECEIPT_LINKAGE_MISMATCH")]
    ReceiptLinkageMismatch,
    /// Tool execution не связан с зафиксированным запросом.
    #[error("REQUEST_TOOL_LINKAGE_MISMATCH")]
    ToolLinkageMismatch,
    /// Shadow content уплотнён и не может быть восстановлен.
    #[error("REQUEST_SHADOW_CONTENT_COMPACTED")]
    ShadowContentCompacted,
    /// Evidence, необходимый для проверки, больше недоступен.
    #[error("REQUEST_EVIDENCE_EVICTED")]
    EvidenceEvicted,
}

/// Result type for provenance validation and canonicalization.
pub type Result<T> = std::result::Result<T, ProvenanceError>;

/// Origin or execution context of a model request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// User-initiated agent task.
    Agent,
    /// Read-only plan review.
    PlanReview,
    /// Plan revision.
    PlanRevision,
    /// Memory extraction or update.
    Memory,
    /// Delegated child-agent request.
    Child,
    /// Scheduled task request.
    Scheduled,
    /// Ambient/listener-originated request.
    Ambient,
    /// Internal summarization request.
    InternalSummary,
}

/// Progress or terminal state captured for a logical request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    /// Request may receive additional attempts or events.
    Active,
    /// Request completed successfully.
    Completed,
    /// Request failed with a known outcome.
    Failed,
    /// Request stopped before a known terminal result.
    Interrupted,
    /// External dispatch may have happened, but the outcome is unknown.
    UnknownOutcome,
    /// Payload was redacted and cannot be reconstructed.
    Redacted,
    /// Payload or evidence was pruned by retention.
    RetentionPruned,
}

impl RequestStatus {
    /// Returns `false` only for [`Self::Active`].
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Active)
    }
}

/// Selects whether request content is retained or represented only by digests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadMode {
    /// Retain bounded canonical request bytes for reconstruction.
    Full,
    /// Retain hashes and metadata without request content.
    HashOnly,
}

/// Provider tool declaration included in a captured request envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Stable tool name sent to the provider.
    pub name: String,
    /// Provider-visible tool description.
    pub description: String,
    /// JSON schema describing the tool arguments.
    pub input_schema: Value,
}

/// One role/content pair included in the provider request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMessage {
    /// Provider role name, such as `system`, `user`, or `assistant`.
    pub role: String,
    /// Message body captured at dispatch time.
    pub content: String,
}

/// Bounded reference from a projection entry to an upstream source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Identifier unique within the containing provenance record.
    pub source_ref_id: String,
    /// Source category, for example memory, tool output, or file context.
    pub source_kind: String,
    /// Identifier of the referenced source record.
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Captured source revision, if the source is versioned.
    pub source_version: Option<String>,
    /// Privacy/trust classification used for the source.
    pub classification: String,
}

/// One include, summary, prune, or other operation in the context projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionEntry {
    /// Identifier for the projected item or block.
    pub projection_entry_id: String,
    /// Projection operation, such as `include`, `summary`, or `prune`.
    pub operation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Source records summarized into this entry.
    pub source_refs: Vec<SourceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Source block identifier, when the operation refers to one block.
    pub block_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Bounded reason the source was removed, when this is a prune entry.
    pub drop_reason: Option<String>,
}

/// Hash-linked projection describing the context supplied to one request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProjection {
    /// Identifier of the context ledger that produced this projection.
    pub ledger_id: String,
    /// Digest of the complete source context ledger.
    pub context_ledger_hash: String,
    /// Ordered operations for selected, summarized, or dropped context.
    pub entries: Vec<ProjectionEntry>,
    /// Digest computed by [`ContextProjection::compute_hash`].
    pub context_projection_hash: String,
}

/// Generation parameters captured alongside the request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelParameters {
    /// Sampling temperature, when configured.
    pub temperature: Option<f64>,
    /// Nucleus sampling threshold, when configured.
    pub top_p: Option<f64>,
    /// Maximum provider output token count, when configured.
    pub max_output_tokens: Option<u32>,
    /// Provider-specific reasoning mode, when configured.
    pub reasoning_mode: Option<String>,
    #[serde(default)]
    /// Additional bounded provider options with credentials excluded.
    pub provider_options: Map<String, Value>,
}

/// Validated logical request and frozen routing/policy context for one attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequestEnvelopeV1 {
    /// Must equal [`CONTRACT_VERSION`].
    pub version: u32,
    /// Unique identifier for this individual provider attempt.
    pub request_id: String,
    /// Identifier shared by retries of the same logical request.
    pub logical_request_id: String,
    /// One-based attempt number for this logical request.
    pub attempt: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Parent request identifier for a retry or child, when applicable.
    pub parent_request_id: Option<String>,
    /// Identifier of the context ledger snapshot.
    pub ledger_id: String,
    /// Request origin category.
    pub request_kind: RequestKind,
    /// Selected provider identifier.
    pub provider: String,
    /// Selected model identifier.
    pub model: String,
    /// Digest of the frozen provider route snapshot.
    pub route_snapshot_hash: String,
    /// Digest of the effective policy snapshot.
    pub policy_snapshot_hash: String,
    /// Whether route-policy data was shared with the provider.
    pub route_policy_hash_shared: bool,
    /// System instruction sent with the request.
    pub system_prompt: String,
    /// Ordered messages sent to the provider.
    pub messages: Vec<ModelMessage>,
    /// Available provider tool schemas.
    pub tools: Vec<ToolSchema>,
    /// Model generation parameters captured for this attempt.
    pub model_parameters: ModelParameters,
    /// Bounded context projection used to compose this request.
    pub context_projection: ContextProjection,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Digest of the prior attempt envelope when this is a retry.
    pub previous_request_hash: Option<String>,
}

impl ModelRequestEnvelopeV1 {
    /// Generates a fresh request ID and logical request ID using UUIDv7.
    pub fn new_ids() -> (String, String) {
        (Uuid::now_v7().to_string(), Uuid::now_v7().to_string())
    }

    /// Checks version, lineage, bounds, uniqueness, projection linkage, and hash.
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

    /// Returns deterministic canonical JSON bytes after successful validation.
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

    /// Returns the domain-separated digest of the canonical request envelope.
    pub fn envelope_hash(&self) -> Result<String> {
        let mut input = MODEL_REQUEST_DOMAIN.to_vec();
        input.extend(self.canonical_bytes()?);
        Ok(sha256_hex(&input))
    }
}

impl ContextProjection {
    /// Builds include, summary, and prune entries and computes their digest.
    ///
    /// `context_ledger_hash` must be lowercase SHA-256 hexadecimal.
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

    /// Computes the projection digest, excluding its existing digest field.
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

/// Stage recorded for a model-dispatch checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointState {
    /// Durable request receipt was committed.
    Committed,
    /// Dispatch was marked before an external provider call.
    DispatchMarked,
}

/// Linkage-only payload for the existing signed receipt chain.
///
/// The payload contains identifiers and digests, not the prompt or provider
/// response, so the receipt does not become a second content store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestReceiptV1 {
    /// Receipt schema version; currently `1`.
    pub receipt_version: u32,
    /// Payload schema version; currently `1`.
    pub payload_version: u32,
    /// Receipt domain; currently `model_request`.
    pub receipt_domain: String,
    /// Receipt subtype; currently `request_commit`.
    pub receipt_type: String,
    /// Identifier of this signed receipt.
    pub receipt_id: String,
    /// Identifier of the linked provider attempt.
    pub request_id: String,
    /// Identifier shared by retries of the logical request.
    pub logical_request_id: String,
    /// One-based attempt number.
    pub attempt: u32,
    /// Identifier of the linked context ledger.
    pub ledger_id: String,
    /// Provider selected for the request.
    pub provider: String,
    /// Model selected for the request.
    pub model: String,
    /// Digest of the canonical request envelope.
    pub request_envelope_hash: String,
    /// Digest of the context projection.
    pub context_projection_hash: String,
    /// Digest of the frozen route snapshot.
    pub route_snapshot_hash: String,
    /// Digest of the effective policy snapshot.
    pub policy_snapshot_hash: String,
    /// Prior receipt digest when this attempt belongs to a retry chain.
    pub previous_receipt_hash: Option<String>,
}

impl ModelRequestReceiptV1 {
    /// Validates versions, subtype, hash encodings, and request identity.
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

    /// Returns canonical JSON bytes after successful validation.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }

    /// Returns SHA-256 of the canonical receipt payload.
    pub fn digest(&self) -> Result<String> {
        Ok(sha256_hex(&self.canonical_bytes()?))
    }
}

/// Computes the context digest from a ledger digest and coverage JSON value.
pub fn context_projection_hash(ledger_hash: &str, content_coverage: &Value) -> Result<String> {
    let ledger_bytes = hex::decode_hash(ledger_hash)?;
    let coverage = canonical_json_value(content_coverage)?;
    let mut bytes = CONTEXT_PROJECTION_DOMAIN.to_vec();
    bytes.extend(ledger_bytes);
    bytes.extend(coverage);
    Ok(sha256_hex(&bytes))
}

/// Returns whether a JSON key names a credential that must not be retained.
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

/// Rejects credential-named keys recursively in an arbitrary JSON value.
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

/// Computes a canonical SHA-256 hash after rejecting credential-bearing JSON.
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
