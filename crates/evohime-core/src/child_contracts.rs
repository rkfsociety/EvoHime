//! Typed child contracts with correlation IDs, provenance, and grant validation.
//!
//! This module implements the 03-1 plan for typed child contracts:
//! - Typed input/output for child tasks
//! - Correlation IDs for task, child, tool call, and receipt
//! - Contract versioning (major/minor)
//! - Provenance tracking (hashes, versions, timestamps, sequences)
//! - Grant validation (child grants must be subset of parent grants)
//! - Report schema validation for persistence and fan-in

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

// ============================================================================
// Constants and Limits
// ============================================================================

pub const MAX_ID_CHARS: usize = 128;
pub const MAX_ROLE_CHARS: usize = 64;
pub const MAX_PURPOSE_CHARS: usize = 512;
pub const MAX_CONTEXT_ITEMS: usize = 32;
pub const MAX_CONTEXT_ITEM_CHARS: usize = 2_048;
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024;
pub const MAX_REPORT_CHARS: usize = 8_192;
pub const MAX_SOURCES: usize = 32;
pub const MAX_SOURCE_CHARS: usize = 512;
pub const MAX_GRANTS: usize = 16;
pub const MAX_GRANT_CHARS: usize = 256;
pub const MAX_CAPABILITIES: usize = 16;
pub const MAX_CAPABILITY_CHARS: usize = 64;
pub const MAX_SCHEMA_CHARS: usize = 4_096;
pub const MAX_HASH_CHARS: usize = 64;
pub const MAX_MODEL_ID_CHARS: usize = 128;
pub const MAX_TOOL_VERSION_CHARS: usize = 64;
pub const MAX_EVIDENCE: usize = 32;
pub const MAX_CHANGED_PATHS: usize = 64;
pub const MAX_TESTS: usize = 32;
pub const MAX_RISKS: usize = 32;
pub const MAX_REVISIONS: u32 = 3;
pub const DEFAULT_MAX_REVISIONS: u32 = 2;
pub const CONTRACT_VERSION: ContractVersion = ContractVersion { major: 1, minor: 0 };

// ============================================================================
// Contract Versioning
// ============================================================================

/// Semantic versioning for child contracts.
/// Major version changes indicate breaking changes that require migration.
/// Minor version changes are backward-compatible additive changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
}

impl ContractVersion {
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    /// Check if this version is compatible with another (same major version)
    pub fn is_compatible_with(&self, other: &ContractVersion) -> bool {
        self.major == other.major
    }

    /// Check if this version can accept additive changes from a newer minor version
    pub fn can_accept_additive(&self, other: &ContractVersion) -> bool {
        self.major == other.major && self.minor <= other.minor
    }
}

impl fmt::Display for ContractVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

// ============================================================================
// Correlation IDs
// ============================================================================

/// Unique correlation identifier for tracking across task boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct CorrelationId(String);

impl CorrelationId {
    pub fn new(id: impl Into<String>) -> Result<Self, ContractError> {
        let id = id.into();
        validate_text("correlation_id", &id, MAX_ID_CHARS, true)?;
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn generate() -> Self {
        // Generate a deterministic correlation ID based on timestamp and random
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let random_part = rand::random::<u64>();
        let combined = format!("{timestamp:020}{random_part:016x}");
        // Take first MAX_ID_CHARS characters
        let id = &combined[..combined.len().min(MAX_ID_CHARS)];
        Self(id.to_string())
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Correlation context tracking IDs across task, child, tool call, and receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationContext {
    /// The parent task correlation ID
    pub task_id: CorrelationId,
    /// The child task correlation ID
    pub child_id: CorrelationId,
    /// The tool call correlation ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<CorrelationId>,
    /// The receipt correlation ID from stage 01.3
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<CorrelationId>,
    /// Parent sequence number for ordering
    pub parent_sequence: u64,
}

impl CorrelationContext {
    pub fn new(task_id: CorrelationId, child_id: CorrelationId, parent_sequence: u64) -> Self {
        Self {
            task_id,
            child_id,
            tool_call_id: None,
            receipt_id: None,
            parent_sequence,
        }
    }

    pub fn with_tool_call(mut self, tool_call_id: CorrelationId) -> Self {
        self.tool_call_id = Some(tool_call_id);
        self
    }

    pub fn with_receipt(mut self, receipt_id: CorrelationId) -> Self {
        self.receipt_id = Some(receipt_id);
        self
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        // All IDs must be non-empty
        if self.task_id.0.is_empty() {
            return Err(ContractError::EmptyField("task_id"));
        }
        if self.child_id.0.is_empty() {
            return Err(ContractError::EmptyField("child_id"));
        }
        Ok(())
    }
}

// ============================================================================
// Grant and Capability Types
// ============================================================================

/// A grant defines what resources or permissions a child task has access to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Grant {
    /// The type of grant (e.g., "workspace.read", "path:/src", "token:budget")
    pub grant_type: String,
    /// Optional scope or path for the grant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl Grant {
    pub fn new(grant_type: impl Into<String>) -> Result<Self, ContractError> {
        let grant_type = grant_type.into();
        validate_text("grant_type", &grant_type, MAX_GRANT_CHARS, true)?;
        Ok(Self {
            grant_type,
            scope: None,
        })
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Result<Self, ContractError> {
        let scope = scope.into();
        validate_text("grant_scope", &scope, MAX_GRANT_CHARS, true)?;
        self.scope = Some(scope);
        Ok(self)
    }

    /// Check if this grant is a subset of another grant
    pub fn is_subset_of(&self, parent: &Grant) -> bool {
        if self.grant_type != parent.grant_type {
            return false;
        }
        match (&self.scope, &parent.scope) {
            (None, _) => true,                   // No scope means it's a subset
            (Some(_child_scope), None) => false, // Child has scope but parent doesn't
            (Some(child_scope), Some(parent_scope)) => {
                let child = child_scope.trim_end_matches('/');
                let parent = parent_scope.trim_end_matches('/');
                child == parent
                    || (child.starts_with(parent)
                        && child.as_bytes().get(parent.len()) == Some(&b'/'))
            }
        }
    }
}

/// Budget constraints for a child task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildBudget {
    /// Maximum token budget
    pub max_tokens: Option<u64>,
    /// Maximum time budget in seconds
    pub max_time_seconds: Option<u64>,
    /// Maximum number of tool calls
    pub max_tool_calls: Option<u64>,
}

impl ChildBudget {
    pub fn new() -> Self {
        Self {
            max_tokens: None,
            max_time_seconds: None,
            max_tool_calls: None,
        }
    }

    pub fn with_tokens(mut self, tokens: u64) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn with_time(mut self, seconds: u64) -> Self {
        self.max_time_seconds = Some(seconds);
        self
    }

    pub fn with_tool_calls(mut self, calls: u64) -> Self {
        self.max_tool_calls = Some(calls);
        self
    }

    /// Check if this budget is within the parent budget
    pub fn is_within_parent(&self, parent: &ChildBudget) -> bool {
        match (self.max_tokens, parent.max_tokens) {
            (Some(child), Some(parent)) => {
                if child > parent {
                    return false;
                }
            }
            (Some(_), None) => return false, // Child has limit but parent doesn't
            (None, _) => {}
        }

        match (self.max_time_seconds, parent.max_time_seconds) {
            (Some(child), Some(parent)) => {
                if child > parent {
                    return false;
                }
            }
            (Some(_), None) => return false,
            (None, _) => {}
        }

        match (self.max_tool_calls, parent.max_tool_calls) {
            (Some(child), Some(parent)) => {
                if child > parent {
                    return false;
                }
            }
            (Some(_), None) => return false,
            (None, _) => {}
        }

        true
    }
}

// ============================================================================
// Input/Output Schema
// ============================================================================

/// Schema definition for typed input/output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    /// JSON Schema for validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<String>,
    /// Content type (e.g., "text/markdown", "application/json")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Maximum size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

impl Schema {
    pub fn new() -> Self {
        Self {
            json_schema: None,
            content_type: None,
            max_bytes: None,
        }
    }

    pub fn with_json_schema(mut self, schema: impl Into<String>) -> Result<Self, ContractError> {
        let schema = schema.into();
        validate_text("json_schema", &schema, MAX_SCHEMA_CHARS, false)?;
        self.json_schema = Some(schema);
        Ok(self)
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = Some(max_bytes);
        self
    }

    pub fn validate_content(&self, content: &str) -> Result<(), ContractError> {
        if let Some(max_bytes) = self.max_bytes {
            if content.len() > max_bytes {
                return Err(ContractError::ContentTooLarge {
                    actual: content.len(),
                    max: max_bytes,
                });
            }
        }
        if let Some(schema) = &self.json_schema {
            let schema_value: serde_json::Value = serde_json::from_str(schema)
                .map_err(|_| ContractError::SchemaValidation("schema is not valid JSON".into()))?;
            let compiled = jsonschema::JSONSchema::compile(&schema_value)
                .map_err(|error| ContractError::SchemaValidation(error.to_string()))?;
            let value: serde_json::Value = serde_json::from_str(content)
                .map_err(|_| ContractError::SchemaValidation("content is not valid JSON".into()))?;
            if let Err(mut errors) = compiled.validate(&value) {
                return Err(ContractError::SchemaValidation(
                    errors
                        .next()
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| "content does not match schema".into()),
                ));
            };
        }
        Ok(())
    }
}

// ============================================================================
// Provenance Tracking
// ============================================================================

/// Provenance information for tracking the origin and processing of data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Hash of the input data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    /// Hash of the evidence/data used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_hash: Option<String>,
    /// Version of the tool used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
    /// Version of the schema used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    /// Model ID used for processing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Timestamp when the data was created
    pub created_at: u64,
    /// Timestamp when processing was completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    /// Parent sequence number
    pub parent_sequence: u64,
}

impl Provenance {
    pub fn new(parent_sequence: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            input_hash: None,
            evidence_hash: None,
            tool_version: None,
            schema_version: None,
            model_id: None,
            created_at: now,
            completed_at: None,
            parent_sequence,
        }
    }

    pub fn with_input_hash(mut self, hash: impl Into<String>) -> Result<Self, ContractError> {
        let hash = hash.into();
        validate_text("input_hash", &hash, MAX_HASH_CHARS, true)?;
        self.input_hash = Some(hash);
        Ok(self)
    }

    pub fn with_evidence_hash(mut self, hash: impl Into<String>) -> Result<Self, ContractError> {
        let hash = hash.into();
        validate_text("evidence_hash", &hash, MAX_HASH_CHARS, true)?;
        self.evidence_hash = Some(hash);
        Ok(self)
    }

    pub fn with_tool_version(mut self, version: impl Into<String>) -> Result<Self, ContractError> {
        let version = version.into();
        validate_text("tool_version", &version, MAX_TOOL_VERSION_CHARS, false)?;
        self.tool_version = Some(version);
        Ok(self)
    }

    pub fn with_schema_version(
        mut self,
        version: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let version = version.into();
        validate_text("schema_version", &version, MAX_TOOL_VERSION_CHARS, false)?;
        self.schema_version = Some(version);
        Ok(self)
    }

    pub fn with_model_id(mut self, model_id: impl Into<String>) -> Result<Self, ContractError> {
        let model_id = model_id.into();
        validate_text("model_id", &model_id, MAX_MODEL_ID_CHARS, true)?;
        self.model_id = Some(model_id);
        Ok(self)
    }

    pub fn mark_completed(mut self) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.completed_at = Some(now);
        self
    }

    /// Compute SHA-256 hash of content
    pub fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

// ============================================================================
// Typed Child Task Request
// ============================================================================

/// Typed child task request with full contract support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedChildTaskRequest {
    /// Unique child task ID
    pub child_task_id: String,
    /// Parent task ID
    pub parent_task_id: String,
    /// Role of the child (e.g., "researcher", "implementer")
    pub role: String,
    /// Purpose/goal of the child task
    pub purpose: String,
    /// Reduced context for the child task
    pub reduced_context: Vec<String>,
    /// Maximum output size in bytes
    pub max_output_bytes: usize,
    /// Requested capabilities
    pub requested_capabilities: Vec<String>,
    /// Whether the parent is also a child (nested delegation)
    pub parent_is_child: bool,
    /// Contract version
    pub contract_version: ContractVersion,
    /// Correlation context
    pub correlation: CorrelationContext,
    /// Input schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Schema>,
    /// Expected output schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Schema>,
    /// Grants for the child task
    pub grants: Vec<Grant>,
    /// Budget constraints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<ChildBudget>,
    /// Acceptance criteria
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<String>,
    /// Maximum number of revisions allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_revisions: Option<u32>,
    /// Explicit context/artifact allowlist for this child.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_context_ids: Vec<String>,
    /// Explicit opt-in for moving oversized output to the artifact store.
    #[serde(default)]
    pub allow_output_offload: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_privacy: Option<String>,
}

impl TypedChildTaskRequest {
    pub fn new(
        child_task_id: impl Into<String>,
        parent_task_id: impl Into<String>,
        role: impl Into<String>,
        purpose: impl Into<String>,
        correlation: CorrelationContext,
    ) -> Result<Self, ContractError> {
        let child_task_id = child_task_id.into();
        let parent_task_id = parent_task_id.into();
        let role = role.into();
        let purpose = purpose.into();

        validate_text("child_task_id", &child_task_id, MAX_ID_CHARS, true)?;
        validate_text("parent_task_id", &parent_task_id, MAX_ID_CHARS, true)?;
        validate_text("role", &role, MAX_ROLE_CHARS, true)?;
        validate_text("purpose", &purpose, MAX_PURPOSE_CHARS, true)?;

        Ok(Self {
            child_task_id,
            parent_task_id,
            role,
            purpose,
            reduced_context: Vec::new(),
            max_output_bytes: MAX_OUTPUT_BYTES,
            requested_capabilities: Vec::new(),
            parent_is_child: false,
            contract_version: CONTRACT_VERSION,
            correlation,
            input_schema: None,
            output_schema: None,
            grants: Vec::new(),
            budget: None,
            acceptance_criteria: None,
            max_revisions: Some(DEFAULT_MAX_REVISIONS),
            input_context_ids: Vec::new(),
            allow_output_offload: false,
            output_privacy: None,
        })
    }

    pub fn with_context(mut self, context: Vec<String>) -> Result<Self, ContractError> {
        if context.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyItems {
                field: "reduced_context",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        for item in &context {
            validate_text("context_item", item, MAX_CONTEXT_ITEM_CHARS, true)?;
        }
        let context_bytes: usize = context.iter().map(|s| s.len()).sum();
        if context_bytes > MAX_CONTEXT_BYTES {
            return Err(ContractError::ContextTooLarge {
                actual: context_bytes,
                max: MAX_CONTEXT_BYTES,
            });
        }
        self.reduced_context = context;
        Ok(self)
    }

    pub fn with_max_output_bytes(mut self, bytes: usize) -> Result<Self, ContractError> {
        if bytes == 0 || bytes > MAX_OUTPUT_BYTES {
            return Err(ContractError::OutputTooLarge {
                actual: bytes,
                max: MAX_OUTPUT_BYTES,
            });
        }
        self.max_output_bytes = bytes;
        Ok(self)
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Result<Self, ContractError> {
        if capabilities.len() > MAX_CAPABILITIES {
            return Err(ContractError::TooManyItems {
                field: "requested_capabilities",
                max: MAX_CAPABILITIES,
            });
        }
        for cap in &capabilities {
            validate_text("capability", cap, MAX_CAPABILITY_CHARS, true)?;
            if !capability_name_is_bounded(cap) {
                return Err(ContractError::ForbiddenCapability(cap.clone()));
            }
        }
        self.requested_capabilities = capabilities;
        Ok(self)
    }

    pub fn with_grants(mut self, grants: Vec<Grant>) -> Result<Self, ContractError> {
        if grants.len() > MAX_GRANTS {
            return Err(ContractError::TooManyItems {
                field: "grants",
                max: MAX_GRANTS,
            });
        }
        self.grants = grants;
        Ok(self)
    }

    pub fn with_input_schema(mut self, schema: Schema) -> Self {
        self.input_schema = Some(schema);
        self
    }

    pub fn with_output_schema(mut self, schema: Schema) -> Self {
        self.output_schema = Some(schema);
        self
    }

    pub fn with_budget(mut self, budget: ChildBudget) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn with_acceptance_criteria(
        mut self,
        criteria: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let criteria = criteria.into();
        validate_text("acceptance_criteria", &criteria, MAX_PURPOSE_CHARS, false)?;
        self.acceptance_criteria = Some(criteria);
        Ok(self)
    }

    pub fn with_max_revisions(mut self, max: u32) -> Self {
        self.max_revisions = Some(max.min(MAX_REVISIONS));
        self
    }

    pub fn with_input_context_ids(mut self, ids: Vec<String>) -> Result<Self, ContractError> {
        if ids.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyItems {
                field: "input_context_ids",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        for id in &ids {
            validate_text("input_context_id", id, MAX_CONTEXT_ITEM_CHARS, true)?;
        }
        self.input_context_ids = ids;
        Ok(self)
    }

    pub fn with_output_offload(mut self, enabled: bool, privacy: Option<String>) -> Self {
        self.allow_output_offload = enabled;
        self.output_privacy = privacy;
        self
    }

    /// Validate the entire request
    pub fn validate(&self) -> Result<(), ContractError> {
        // Validate basic fields
        validate_text("child_task_id", &self.child_task_id, MAX_ID_CHARS, true)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_CHARS, true)?;
        validate_text("role", &self.role, MAX_ROLE_CHARS, true)?;
        validate_text("purpose", &self.purpose, MAX_PURPOSE_CHARS, true)?;

        // Validate correlation
        self.correlation.validate()?;

        // Validate nested child
        if self.parent_is_child {
            return Err(ContractError::NestedChildForbidden);
        }

        // Validate context
        if self.reduced_context.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyItems {
                field: "reduced_context",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        let context_bytes: usize = self.reduced_context.iter().map(|s| s.len()).sum();
        if context_bytes > MAX_CONTEXT_BYTES {
            return Err(ContractError::ContextTooLarge {
                actual: context_bytes,
                max: MAX_CONTEXT_BYTES,
            });
        }
        for item in &self.reduced_context {
            validate_text("context_item", item, MAX_CONTEXT_ITEM_CHARS, true)?;
        }

        // Validate output bytes
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_OUTPUT_BYTES {
            return Err(ContractError::OutputTooLarge {
                actual: self.max_output_bytes,
                max: MAX_OUTPUT_BYTES,
            });
        }

        // Validate capabilities
        if self.requested_capabilities.len() > MAX_CAPABILITIES {
            return Err(ContractError::TooManyItems {
                field: "requested_capabilities",
                max: MAX_CAPABILITIES,
            });
        }
        for cap in &self.requested_capabilities {
            validate_text("capability", cap, MAX_CAPABILITY_CHARS, true)?;
            if !capability_name_is_bounded(cap) || !role_allows_capability(&self.role, cap) {
                return Err(ContractError::ForbiddenCapability(cap.clone()));
            }
        }

        // Validate grants
        if self.grants.len() > MAX_GRANTS {
            return Err(ContractError::TooManyItems {
                field: "grants",
                max: MAX_GRANTS,
            });
        }

        // Validate schemas if present
        if let Some(ref schema) = self.input_schema {
            if let Some(ref json_schema) = schema.json_schema {
                validate_text("input_json_schema", json_schema, MAX_SCHEMA_CHARS, false)?;
            }
        }
        if let Some(ref schema) = self.output_schema {
            if let Some(ref json_schema) = schema.json_schema {
                validate_text("output_json_schema", json_schema, MAX_SCHEMA_CHARS, false)?;
            }
        }

        // Validate acceptance criteria if present
        if let Some(ref criteria) = self.acceptance_criteria {
            validate_text("acceptance_criteria", criteria, MAX_PURPOSE_CHARS, false)?;
        }
        if self.max_revisions.is_some_and(|max| max > MAX_REVISIONS) {
            return Err(ContractError::TooManyRevisions);
        }
        if self.input_context_ids.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyItems {
                field: "input_context_ids",
                max: MAX_CONTEXT_ITEMS,
            });
        }

        Ok(())
    }

    /// Check if this request's grants are a subset of parent grants
    pub fn grants_are_subset_of(&self, parent_grants: &[Grant]) -> bool {
        // If child has no grants, it's always a subset
        if self.grants.is_empty() {
            return true;
        }

        // For each child grant, check if there's a matching parent grant
        for child_grant in &self.grants {
            let has_matching_parent = parent_grants
                .iter()
                .any(|parent_grant| child_grant.is_subset_of(parent_grant));
            if !has_matching_parent {
                return false;
            }
        }

        true
    }

    /// Check if this request's budget is within parent budget
    pub fn budget_is_within_parent(&self, parent_budget: &Option<ChildBudget>) -> bool {
        match (&self.budget, parent_budget) {
            (Some(child_budget), Some(parent_budget)) => {
                child_budget.is_within_parent(parent_budget)
            }
            (Some(_), None) => false, // Child has budget but parent doesn't
            (None, _) => true,        // Child has no budget constraints
        }
    }

    /// Serialize to deterministic JSON
    pub fn to_deterministic_json(&self) -> Result<String, ContractError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| ContractError::SerializationError)
    }
}

// ============================================================================
// Typed Child Report
// ============================================================================

/// Status of a child report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedReportStatus {
    Complete,
    Partial,
    Rejected,
    Failed,
}

/// Typed child report with full provenance and correlation support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedChildReport {
    /// Child task ID this report is for
    pub child_task_id: String,
    /// Parent task ID
    pub parent_task_id: String,
    /// Status of the report
    pub status: TypedReportStatus,
    /// Summary of the results
    pub summary: String,
    /// Detailed findings
    pub findings: Vec<String>,
    /// Sources referenced
    pub sources: Vec<String>,
    /// Confidence percentage (0-100)
    pub confidence_percent: u8,
    /// Correlation context
    pub correlation: CorrelationContext,
    /// Provenance information
    pub provenance: Provenance,
    /// Output data (must match output_schema if specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_data: Option<String>,
    /// Errors encountered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    /// Revision number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    /// Whether this report has been accepted by parent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidenceItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub risks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_artifact: Option<ArtifactOutputRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub locator: String,
    pub summary: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactOutputRef {
    pub locator: String,
    pub content_hash: String,
    pub summary: String,
}

impl TypedChildReport {
    pub fn new(
        child_task_id: impl Into<String>,
        parent_task_id: impl Into<String>,
        correlation: CorrelationContext,
        provenance: Provenance,
    ) -> Result<Self, ContractError> {
        let child_task_id = child_task_id.into();
        let parent_task_id = parent_task_id.into();

        validate_text("child_task_id", &child_task_id, MAX_ID_CHARS, true)?;
        validate_text("parent_task_id", &parent_task_id, MAX_ID_CHARS, true)?;

        Ok(Self {
            child_task_id,
            parent_task_id,
            status: TypedReportStatus::Complete,
            summary: String::new(),
            findings: Vec::new(),
            sources: Vec::new(),
            confidence_percent: 0,
            correlation,
            provenance,
            output_data: None,
            errors: None,
            revision: None,
            accepted: None,
            evidence: Vec::new(),
            changed_paths: Vec::new(),
            tests: Vec::new(),
            risks: Vec::new(),
            next_action: None,
            output_artifact: None,
        })
    }

    pub fn with_status(mut self, status: TypedReportStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Result<Self, ContractError> {
        let summary = summary.into();
        validate_text("summary", &summary, MAX_REPORT_CHARS, true)?;
        reject_secret_like(&summary)?;
        self.summary = summary;
        Ok(self)
    }

    pub fn with_findings(mut self, findings: Vec<String>) -> Result<Self, ContractError> {
        if findings.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyItems {
                field: "findings",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        for finding in &findings {
            validate_text("finding", finding, MAX_REPORT_CHARS, true)?;
            reject_secret_like(finding)?;
        }
        self.findings = findings;
        Ok(self)
    }

    pub fn with_sources(mut self, sources: Vec<String>) -> Result<Self, ContractError> {
        if sources.len() > MAX_SOURCES {
            return Err(ContractError::TooManyItems {
                field: "sources",
                max: MAX_SOURCES,
            });
        }
        let mut seen = BTreeSet::new();
        for source in &sources {
            validate_text("source", source, MAX_SOURCE_CHARS, true)?;
            reject_secret_like(source)?;
            if !seen.insert(source) {
                return Err(ContractError::DuplicateSource);
            }
        }
        self.sources = sources;
        Ok(self)
    }

    pub fn with_confidence(mut self, percent: u8) -> Self {
        self.confidence_percent = percent.min(100);
        self
    }

    pub fn with_output_data(mut self, data: impl Into<String>) -> Result<Self, ContractError> {
        let data = data.into();
        // Validate against output schema if present
        // (This would be checked against the request's output_schema)
        self.output_data = Some(data);
        Ok(self)
    }

    pub fn with_errors(mut self, errors: Vec<String>) -> Self {
        self.errors = Some(errors);
        self
    }

    pub fn with_revision(mut self, revision: u32) -> Self {
        self.revision = Some(revision);
        self
    }

    pub fn with_accepted(mut self, accepted: bool) -> Self {
        self.accepted = Some(accepted);
        self
    }

    pub fn with_evidence(mut self, evidence: Vec<EvidenceItem>) -> Result<Self, ContractError> {
        if evidence.len() > MAX_EVIDENCE {
            return Err(ContractError::TooManyItems {
                field: "evidence",
                max: MAX_EVIDENCE,
            });
        }
        self.evidence = evidence;
        Ok(self)
    }

    pub fn with_changed_paths(mut self, paths: Vec<String>) -> Result<Self, ContractError> {
        if paths.len() > MAX_CHANGED_PATHS {
            return Err(ContractError::TooManyItems {
                field: "changed_paths",
                max: MAX_CHANGED_PATHS,
            });
        }
        self.changed_paths = paths;
        Ok(self)
    }

    pub fn with_tests(mut self, tests: Vec<TestResult>) -> Result<Self, ContractError> {
        if tests.len() > MAX_TESTS {
            return Err(ContractError::TooManyItems {
                field: "tests",
                max: MAX_TESTS,
            });
        }
        self.tests = tests;
        Ok(self)
    }

    pub fn with_risks(mut self, risks: Vec<String>) -> Result<Self, ContractError> {
        if risks.len() > MAX_RISKS {
            return Err(ContractError::TooManyItems {
                field: "risks",
                max: MAX_RISKS,
            });
        }
        self.risks = risks;
        Ok(self)
    }

    pub fn with_next_action(mut self, action: impl Into<String>) -> Result<Self, ContractError> {
        let action = action.into();
        validate_text("next_action", &action, MAX_PURPOSE_CHARS, false)?;
        self.next_action = Some(action);
        Ok(self)
    }

    /// Validate the report
    pub fn validate(&self) -> Result<(), ContractError> {
        // Validate basic fields
        validate_text("child_task_id", &self.child_task_id, MAX_ID_CHARS, true)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_CHARS, true)?;

        // Validate correlation
        self.correlation.validate()?;

        // Validate summary
        validate_text("summary", &self.summary, MAX_REPORT_CHARS, true)?;
        reject_secret_like(&self.summary)?;

        // Validate findings
        if self.findings.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyItems {
                field: "findings",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        for finding in &self.findings {
            validate_text("finding", finding, MAX_REPORT_CHARS, true)?;
            reject_secret_like(finding)?;
        }

        // Validate sources
        if self.sources.len() > MAX_SOURCES {
            return Err(ContractError::TooManyItems {
                field: "sources",
                max: MAX_SOURCES,
            });
        }
        let mut seen = BTreeSet::new();
        for source in &self.sources {
            validate_text("source", source, MAX_SOURCE_CHARS, true)?;
            reject_secret_like(source)?;
            if !seen.insert(source) {
                return Err(ContractError::DuplicateSource);
            }
        }

        // Validate output data if present
        if let Some(ref data) = self.output_data {
            if data.len() > MAX_OUTPUT_BYTES {
                return Err(ContractError::OutputTooLarge {
                    actual: data.len(),
                    max: MAX_OUTPUT_BYTES,
                });
            }
            reject_secret_like(data)?;
        }

        // Validate errors if present
        if let Some(ref errors) = self.errors {
            for error in errors {
                reject_secret_like(error)?;
            }
        }
        if self.evidence.len() > MAX_EVIDENCE
            || self.changed_paths.len() > MAX_CHANGED_PATHS
            || self.tests.len() > MAX_TESTS
            || self.risks.len() > MAX_RISKS
        {
            return Err(ContractError::TooManyItems {
                field: "report_details",
                max: MAX_EVIDENCE,
            });
        }
        for evidence in &self.evidence {
            validate_text(
                "evidence.locator",
                &evidence.locator,
                MAX_SOURCE_CHARS,
                true,
            )?;
            validate_text(
                "evidence.content_hash",
                &evidence.content_hash,
                MAX_HASH_CHARS,
                true,
            )?;
            reject_secret_like(&evidence.summary)?;
        }
        for risk in &self.risks {
            reject_secret_like(risk)?;
        }

        Ok(())
    }

    /// Validate that this report matches the corresponding request
    pub fn validate_against_request(
        &self,
        request: &TypedChildTaskRequest,
    ) -> Result<(), ContractError> {
        // Check task IDs match
        if self.child_task_id != request.child_task_id {
            return Err(ContractError::TaskMismatch);
        }
        if self.parent_task_id != request.parent_task_id {
            return Err(ContractError::ParentTaskMismatch);
        }

        // Check correlation IDs match
        if self.correlation.child_id != request.correlation.child_id {
            return Err(ContractError::CorrelationMismatch);
        }
        if self.correlation.task_id != request.correlation.task_id {
            return Err(ContractError::CorrelationMismatch);
        }

        // Validate output data against schema if present
        if let Some(ref output_data) = self.output_data {
            if let Some(ref schema) = request.output_schema {
                schema.validate_content(output_data)?;
            }
        }

        if self.provenance.parent_sequence != self.correlation.parent_sequence
            || self.provenance.created_at
                > self
                    .provenance
                    .completed_at
                    .unwrap_or(self.provenance.created_at)
            || (matches!(
                self.status,
                TypedReportStatus::Complete
                    | TypedReportStatus::Rejected
                    | TypedReportStatus::Failed
            ) && self.provenance.completed_at.is_none())
        {
            return Err(ContractError::StaleProvenance);
        }
        if let Some(schema_version) = &self.provenance.schema_version {
            let major = schema_version
                .split('.')
                .next()
                .and_then(|value| value.parse::<u32>().ok());
            if major != Some(request.contract_version.major) {
                return Err(ContractError::StaleProvenance);
            }
        }

        Ok(())
    }

    /// Serialize to deterministic JSON
    pub fn to_deterministic_json(&self) -> Result<String, ContractError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| ContractError::SerializationError)
    }
}

// ============================================================================
// Acceptance and Validation
// ============================================================================

/// Accept a typed report against its request.
pub fn accept_typed_report(
    request: &TypedChildTaskRequest,
    report: &TypedChildReport,
) -> Result<TypedChildReport, ContractError> {
    request.validate()?;
    report.validate()?;
    report.validate_against_request(request)?;
    Ok(report.clone())
}

/// Boundary check used by create/accept paths. Optional minor fields are
/// additive; a different major is never silently migrated.
pub fn validate_contract_version(
    actual: ContractVersion,
    supported: ContractVersion,
) -> Result<(), ContractError> {
    if supported.can_accept_additive(&actual) {
        Ok(())
    } else {
        Err(ContractError::VersionMismatch)
    }
}

pub fn validate_context_allowlist(
    request: &TypedChildTaskRequest,
    accessible_ids: &BTreeSet<String>,
) -> Result<(), ContractError> {
    for id in &request.input_context_ids {
        if !accessible_ids.contains(id) {
            return Err(ContractError::ContextIdNotAccessible { id: id.clone() });
        }
    }
    Ok(())
}

/// Validate that child grants are a subset of parent grants.
pub fn validate_grant_subset(
    child_grants: &[Grant],
    parent_grants: &[Grant],
) -> Result<(), ContractError> {
    for child_grant in child_grants {
        let has_matching_parent = parent_grants
            .iter()
            .any(|parent_grant| child_grant.is_subset_of(parent_grant));
        if !has_matching_parent {
            return Err(ContractError::GrantEscalation {
                grant: child_grant.grant_type.clone(),
            });
        }
    }
    Ok(())
}

/// Validate that child budget is within parent budget.
pub fn validate_budget_subset(
    child_budget: &Option<ChildBudget>,
    parent_budget: &Option<ChildBudget>,
) -> Result<(), ContractError> {
    match (child_budget, parent_budget) {
        (Some(child), Some(parent)) => {
            if !child.is_within_parent(parent) {
                return Err(ContractError::BudgetExceedsParent);
            }
        }
        (Some(_), None) => {
            return Err(ContractError::BudgetExceedsParent);
        }
        (None, _) => {}
    }
    Ok(())
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    // Basic validation errors
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    TooManyItems { field: &'static str, max: usize },
    ContentTooLarge { actual: usize, max: usize },
    OutputTooLarge { actual: usize, max: usize },
    ContextTooLarge { actual: usize, max: usize },
    SchemaValidation(String),
    VersionMismatch,
    StaleProvenance,
    ContextIdNotAccessible { id: String },
    TooManyRevisions,
    GrantDrift,
    ArtifactOffload(String),

    // Capability errors
    ForbiddenCapability(String),
    NestedChildForbidden,

    // Grant errors
    GrantEscalation { grant: String },
    BudgetExceedsParent,

    // Correlation errors
    CorrelationMismatch,

    // Task matching errors
    TaskMismatch,
    ParentTaskMismatch,

    // Source errors
    DuplicateSource,

    // Secret content
    SecretLikeContent,

    // Serialization
    SerializationError,
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyItems { field, max } => write!(f, "{field} exceeds {max} items"),
            Self::ContentTooLarge { actual, max } => {
                write!(f, "content is {actual} bytes, maximum is {max}")
            }
            Self::OutputTooLarge { actual, max } => {
                write!(f, "output is {actual} bytes, maximum is {max}")
            }
            Self::ContextTooLarge { actual, max } => {
                write!(f, "context is {actual} bytes, maximum is {max}")
            }
            Self::SchemaValidation(message) => write!(f, "schema validation failed: {message}"),
            Self::VersionMismatch => write!(f, "incompatible child contract version"),
            Self::StaleProvenance => write!(f, "child report provenance is stale or incomplete"),
            Self::ContextIdNotAccessible { id } => {
                write!(f, "child context is not accessible: {id}")
            }
            Self::TooManyRevisions => write!(f, "child revision limit exceeds the maximum"),
            Self::GrantDrift => write!(f, "parent grant changed and no longer covers child grant"),
            Self::ArtifactOffload(message) => write!(f, "child output offload failed: {message}"),
            Self::ForbiddenCapability(capability) => {
                write!(f, "forbidden child capability: {capability}")
            }
            Self::NestedChildForbidden => write!(f, "nested child delegation is forbidden"),
            Self::GrantEscalation { grant } => {
                write!(f, "grant escalation detected: {grant}")
            }
            Self::BudgetExceedsParent => write!(f, "child budget exceeds parent budget"),
            Self::CorrelationMismatch => write!(f, "correlation IDs do not match"),
            Self::TaskMismatch => write!(f, "report child_task_id does not match request"),
            Self::ParentTaskMismatch => write!(f, "report parent_task_id does not match request"),
            Self::DuplicateSource => write!(f, "report contains duplicate sources"),
            Self::SecretLikeContent => write!(f, "secret-like content is not allowed"),
            Self::SerializationError => write!(f, "serialization error"),
        }
    }
}

impl std::error::Error for ContractError {}

// ============================================================================
// Helper Functions
// ============================================================================

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), ContractError> {
    if required && value.trim().is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ContractError::FieldTooLong { field, max });
    }
    Ok(())
}

fn capability_name_is_bounded(capability: &str) -> bool {
    matches!(
        capability,
        "workspace.read"
            | "workspace.search"
            | "workspace.write"
            | "git.diff"
            | "git.status"
            | "test.execute"
    )
}

fn role_allows_capability(role: &str, capability: &str) -> bool {
    match role {
        "implementer" => !matches!(capability, "test.execute" | "git.commit" | "git.push"),
        "tester" => !matches!(capability, "workspace.write" | "git.commit" | "git.push"),
        "researcher" | "reviewer" | "coordinator" => !matches!(
            capability,
            "workspace.write" | "test.execute" | "git.commit" | "git.push"
        ),
        _ => {
            capability_name_is_bounded(capability)
                && !matches!(capability, "workspace.write" | "test.execute")
        }
    }
}

fn reject_secret_like(value: &str) -> Result<(), ContractError> {
    let lower = value.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer ",
        "password",
        "secret",
        "token",
        "sk-",
        "ghp_",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(ContractError::SecretLikeContent);
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_correlation() -> CorrelationContext {
        CorrelationContext::new(
            CorrelationId::new("task-1").unwrap(),
            CorrelationId::new("child-1").unwrap(),
            1,
        )
    }

    fn create_test_request() -> TypedChildTaskRequest {
        TypedChildTaskRequest::new(
            "child-1",
            "task-1",
            "researcher",
            "analyze codebase",
            create_test_correlation(),
        )
        .unwrap()
        .with_context(vec!["inspect src".to_string()])
        .unwrap()
        .with_max_output_bytes(4096)
        .unwrap()
        .with_capabilities(vec!["workspace.read".to_string()])
        .unwrap()
    }

    fn create_test_report() -> TypedChildReport {
        TypedChildReport::new(
            "child-1",
            "task-1",
            create_test_correlation(),
            Provenance::new(1).mark_completed(),
        )
        .unwrap()
        .with_status(TypedReportStatus::Complete)
        .with_summary("analysis complete")
        .unwrap()
        .with_findings(vec!["found module".to_string()])
        .unwrap()
        .with_sources(vec!["src/lib.rs:10".to_string()])
        .unwrap()
        .with_confidence(90)
    }

    #[test]
    fn test_correlation_id_creation() {
        let id = CorrelationId::new("test-123").unwrap();
        assert_eq!(id.as_str(), "test-123");
        assert!(CorrelationId::new("").is_err());
    }

    #[test]
    fn test_contract_version_compatibility() {
        let v1_0 = ContractVersion::new(1, 0);
        let v1_1 = ContractVersion::new(1, 1);
        let v2_0 = ContractVersion::new(2, 0);

        assert!(v1_0.is_compatible_with(&v1_1));
        assert!(!v1_0.is_compatible_with(&v2_0));
        assert!(v1_0.can_accept_additive(&v1_1));
        assert!(!v1_1.can_accept_additive(&v1_0));
    }

    #[test]
    fn test_grant_subset_validation() {
        let parent_grant = Grant::new("workspace.read").unwrap();
        let child_grant = Grant::new("workspace.read").unwrap();
        let other_grant = Grant::new("workspace.write").unwrap();

        assert!(child_grant.is_subset_of(&parent_grant));
        assert!(!other_grant.is_subset_of(&parent_grant));
    }

    #[test]
    fn test_budget_subset_validation() {
        let parent_budget = ChildBudget::new()
            .with_tokens(1000)
            .with_time(60)
            .with_tool_calls(10);

        let child_budget = ChildBudget::new()
            .with_tokens(500)
            .with_time(30)
            .with_tool_calls(5);

        assert!(child_budget.is_within_parent(&parent_budget));

        let oversized_budget = ChildBudget::new()
            .with_tokens(1500)
            .with_time(30)
            .with_tool_calls(5);

        assert!(!oversized_budget.is_within_parent(&parent_budget));
    }

    #[test]
    fn test_typed_request_validation() {
        let request = create_test_request();
        assert!(request.validate().is_ok());

        // Test invalid nested child
        let mut nested = request.clone();
        nested.parent_is_child = true;
        assert!(matches!(
            nested.validate(),
            Err(ContractError::NestedChildForbidden)
        ));
    }

    #[test]
    fn test_typed_report_validation() {
        let report = create_test_report();
        assert!(report.validate().is_ok());

        // Test secret-like content
        let mut unsafe_report = report.clone();
        unsafe_report.summary = "token must not leak".to_string();
        assert!(matches!(
            unsafe_report.validate(),
            Err(ContractError::SecretLikeContent)
        ));
    }

    #[test]
    fn test_report_against_request_validation() {
        let request = create_test_request();
        let report = create_test_report();
        assert!(report.validate_against_request(&request).is_ok());

        // Test mismatched task ID
        let mut mismatched = report.clone();
        mismatched.child_task_id = "other-child".to_string();
        assert!(matches!(
            mismatched.validate_against_request(&request),
            Err(ContractError::TaskMismatch)
        ));
    }

    #[test]
    fn test_accept_typed_report() {
        let request = create_test_request();
        let report = create_test_report();
        let accepted = accept_typed_report(&request, &report).unwrap();
        assert_eq!(accepted.child_task_id, "child-1");
    }

    #[test]
    fn test_grant_validation() {
        let parent_grants = vec![
            Grant::new("workspace.read").unwrap(),
            Grant::new("git.status").unwrap(),
        ];

        let child_grants = vec![Grant::new("workspace.read").unwrap()];

        assert!(validate_grant_subset(&child_grants, &parent_grants).is_ok());

        let escalated_grants = vec![Grant::new("workspace.write").unwrap()];

        assert!(matches!(
            validate_grant_subset(&escalated_grants, &parent_grants),
            Err(ContractError::GrantEscalation { .. })
        ));
    }

    #[test]
    fn test_provenance_hashing() {
        let hash = Provenance::compute_hash("test content");
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[test]
    fn test_schema_validation() {
        let schema = Schema::new()
            .with_max_bytes(100)
            .with_content_type("text/plain");

        assert!(schema.validate_content("short").is_ok());
        assert!(schema.validate_content(&"x".repeat(101)).is_err());
    }

    #[test]
    fn test_correlation_context() {
        let context = CorrelationContext::new(
            CorrelationId::new("task-1").unwrap(),
            CorrelationId::new("child-1").unwrap(),
            1,
        )
        .with_tool_call(CorrelationId::new("tool-1").unwrap())
        .with_receipt(CorrelationId::new("receipt-1").unwrap());

        assert!(context.validate().is_ok());
        assert_eq!(context.parent_sequence, 1);
    }

    #[test]
    fn test_deterministic_serialization() {
        let request = create_test_request();
        let json1 = request.to_deterministic_json().unwrap();
        let json2 = request.to_deterministic_json().unwrap();
        assert_eq!(json1, json2);

        let report = create_test_report();
        let json1 = report.to_deterministic_json().unwrap();
        let json2 = report.to_deterministic_json().unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn test_contract_version_constants() {
        assert_eq!(CONTRACT_VERSION.major, 1);
        assert_eq!(CONTRACT_VERSION.minor, 0);
    }
}
