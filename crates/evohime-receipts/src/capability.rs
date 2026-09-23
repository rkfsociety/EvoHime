//! Core-owned capability snapshots and the stable policy decision contract.
//!
//! A snapshot is an immutable, bounded description of the authority available
//! to one run.  It is deliberately data-only: callers may display it, but no
//! renderer, prompt, or model signal can enlarge it.

use crate::{canonicalize_json, sha256_hex, ReceiptError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const SNAPSHOT_DOMAIN: &[u8] = b"evohime-capability-snapshot-v1\0";
const MAX_ITEMS: usize = 128;
const MAX_TEXT: usize = 512;
const MAX_PAYLOAD: usize = 64 * 1024;

/// Stable categories returned by policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    /// Policy granted the requested operation.
    Allowed,
    /// An explicit user approval is required before execution.
    ApprovalRequired,
    /// Policy refused the operation.
    Denied,
    /// Policy evaluation could not run because a required dependency was unavailable.
    Unavailable,
    /// The decision or approval has expired.
    Expired,
    /// The operation was cancelled before execution.
    Cancelled,
    /// Policy evaluation failed with an internal policy error.
    PolicyError,
    /// Outcome is unknown to this version of the contract.
    UnknownOutcome,
}

/// Stable policy result carried into action receipts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    /// Final outcome of policy evaluation.
    pub outcome: PolicyOutcome,
    /// Bounded stable reason code safe for persistence and display.
    pub reason_code: String,
    /// Whether retrying may produce a different outcome.
    pub retryable: bool,
}

impl PolicyDecision {
    /// Creates a validated decision and derives retryability from its outcome.
    pub fn new(
        outcome: PolicyOutcome,
        reason_code: impl Into<String>,
    ) -> Result<Self, ReceiptError> {
        let reason_code = reason_code.into();
        if reason_code.is_empty() || reason_code.len() > MAX_TEXT || !valid_text(&reason_code) {
            return Err(ReceiptError::SchemaViolation);
        }
        let retryable = matches!(outcome, PolicyOutcome::Unavailable);
        Ok(Self {
            outcome,
            reason_code,
            retryable,
        })
    }
}

/// Immutable, hashed description of authority available to a single run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySnapshotV1 {
    /// Unique identifier of this snapshot.
    pub snapshot_id: String,
    /// Run identifier that owns the capabilities.
    pub run_id: String,
    /// Session identifier associated with the run.
    pub session_id: String,
    /// Task identifier associated with the run.
    pub task_id: String,
    /// Parent snapshot hash when this snapshot narrows inherited authority.
    pub parent_snapshot_hash: Option<String>,
    /// Identifier of the policy that produced the snapshot.
    pub policy_id: String,
    /// Version of the policy definition.
    pub policy_version: u32,
    /// Hash of the exact policy definition.
    pub policy_hash: String,
    /// Hash of the tool or capability manifest used for evaluation.
    pub manifest_hash: String,
    /// Workspace roots the run may access.
    pub workspace_anchors: Vec<String>,
    /// Normalized operation scopes authorized for the run.
    pub operation_scopes: Vec<String>,
    /// Named permissions available to the run.
    pub permissions: Vec<String>,
    /// Tool identities available to the run.
    pub tool_identities: Vec<String>,
    /// Network routes allowed to the run.
    pub network_routes: Vec<String>,
    /// Adapter-specific scopes granted to the run.
    pub adapter_scopes: Vec<String>,
    /// Opaque secret references paired with their declared purposes.
    pub secret_refs: Vec<SecretRefPurpose>,
    /// Upper bounds applied to execution and resource consumption.
    pub limits: CapabilityLimits,
    #[serde(skip)]
    /// Hash of the canonical snapshot payload, excluded from its own digest.
    pub snapshot_hash: String,
}

/// Opaque secret reference and the purpose for which it may be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRefPurpose {
    /// Non-secret reference identifying a secret in the secure store.
    pub secret_ref: String,
    /// Bounded explanation of the authorized use.
    pub purpose: String,
}

/// Numeric upper bounds copied into a capability snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLimits {
    /// Maximum execution duration in milliseconds.
    pub timeout_ms: u64,
    /// Maximum input bytes accepted by the action.
    pub input_bytes: u64,
    /// Maximum output bytes accepted from the action.
    pub output_bytes: u64,
    /// Maximum number of concurrent operations.
    pub concurrency: u32,
    /// Maximum tool calls in the run.
    pub tool_calls: u32,
    /// Maximum model-token budget.
    pub token_budget: u64,
    /// Maximum estimated cost in micro-units.
    pub cost_micros: u64,
}

impl CapabilitySnapshotV1 {
    /// Validates the snapshot and fills its canonical content hash.
    pub fn finalize(mut self) -> Result<Self, ReceiptError> {
        self.validate()?;
        self.snapshot_hash = self.compute_hash()?;
        Ok(self)
    }

    /// Checks identifiers, hashes, bounds, uniqueness, and nonzero required limits.
    pub fn validate(&self) -> Result<(), ReceiptError> {
        for id in [
            &self.snapshot_id,
            &self.run_id,
            &self.session_id,
            &self.task_id,
            &self.policy_id,
        ] {
            if id.is_empty() || id.len() > MAX_TEXT || !valid_text(id) {
                return Err(ReceiptError::SchemaViolation);
            }
        }
        for hash in [&self.policy_hash, &self.manifest_hash] {
            if !is_hash(hash) {
                return Err(ReceiptError::SchemaViolation);
            }
        }
        if let Some(hash) = &self.parent_snapshot_hash {
            if !is_hash(hash) {
                return Err(ReceiptError::SchemaViolation);
            }
        }
        if self.policy_version == 0
            || self.operation_scopes.is_empty()
            || self.tool_identities.is_empty()
        {
            return Err(ReceiptError::SchemaViolation);
        }
        for list in [
            &self.workspace_anchors,
            &self.operation_scopes,
            &self.permissions,
            &self.tool_identities,
            &self.network_routes,
            &self.adapter_scopes,
        ] {
            if list.len() > MAX_ITEMS
                || list
                    .iter()
                    .any(|v| v.is_empty() || v.len() > MAX_TEXT || !valid_text(v))
                || list.iter().collect::<HashSet<_>>().len() != list.len()
            {
                return Err(ReceiptError::SchemaViolation);
            }
        }
        if self.secret_refs.len() > MAX_ITEMS
            || self.secret_refs.iter().any(|r| {
                r.secret_ref.is_empty()
                    || r.purpose.is_empty()
                    || r.secret_ref.len() > MAX_TEXT
                    || r.purpose.len() > MAX_TEXT
                    || !valid_text(&r.secret_ref)
                    || !valid_text(&r.purpose)
            })
            || self
                .secret_refs
                .iter()
                .map(|r| (&r.secret_ref, &r.purpose))
                .collect::<HashSet<_>>()
                .len()
                != self.secret_refs.len()
        {
            return Err(ReceiptError::SchemaViolation);
        }
        if self.limits.timeout_ms == 0 || self.limits.concurrency == 0 {
            return Err(ReceiptError::SchemaViolation);
        }
        Ok(())
    }

    /// Computes the domain-separated hash of canonical snapshot bytes.
    pub fn compute_hash(&self) -> Result<String, ReceiptError> {
        let canonical = self.canonical_bytes()?;
        let mut input = SNAPSHOT_DOMAIN.to_vec();
        input.extend(&canonical);
        Ok(sha256_hex(&input))
    }

    /// Serializes the snapshot canonically without its self-referential hash field.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ReceiptError> {
        let mut value = serde_json::to_value(self).map_err(|_| ReceiptError::InvalidJson)?;
        value
            .as_object_mut()
            .ok_or(ReceiptError::SchemaViolation)?
            .remove("snapshot_hash");
        let canonical =
            canonicalize_json(&serde_json::to_vec(&value).map_err(|_| ReceiptError::InvalidJson)?)?;
        if canonical.len() > MAX_PAYLOAD {
            return Err(ReceiptError::PayloadTooLarge);
        }
        Ok(canonical)
    }

    /// Verifies this snapshot only narrows the parent's scopes and resource limits.
    pub fn is_subset_of(&self, parent: &Self) -> Result<(), ReceiptError> {
        self.validate()?;
        parent.validate()?;
        if self.parent_snapshot_hash.as_deref() != Some(parent.snapshot_hash.as_str()) {
            return Err(ReceiptError::SchemaViolation);
        }
        if !self
            .workspace_anchors
            .iter()
            .all(|v| parent.workspace_anchors.contains(v))
            || !self
                .operation_scopes
                .iter()
                .all(|v| parent.operation_scopes.contains(v))
            || !self
                .permissions
                .iter()
                .all(|v| parent.permissions.contains(v))
            || !self
                .tool_identities
                .iter()
                .all(|v| parent.tool_identities.contains(v))
            || !self
                .network_routes
                .iter()
                .all(|v| parent.network_routes.contains(v))
            || !self
                .adapter_scopes
                .iter()
                .all(|v| parent.adapter_scopes.contains(v))
        {
            return Err(ReceiptError::SchemaViolation);
        }
        if self.limits.timeout_ms > parent.limits.timeout_ms
            || self.limits.input_bytes > parent.limits.input_bytes
            || self.limits.output_bytes > parent.limits.output_bytes
            || self.limits.concurrency > parent.limits.concurrency
            || self.limits.tool_calls > parent.limits.tool_calls
            || self.limits.token_budget > parent.limits.token_budget
            || self.limits.cost_micros > parent.limits.cost_micros
        {
            return Err(ReceiptError::SchemaViolation);
        }
        if !self
            .secret_refs
            .iter()
            .all(|r| parent.secret_refs.contains(r))
        {
            return Err(ReceiptError::SchemaViolation);
        }
        Ok(())
    }

    /// Returns a display-safe summary that omits opaque secret references.
    pub fn redacted_summary(&self) -> Value {
        serde_json::json!({
            "snapshot_id": self.snapshot_id, "snapshot_hash": self.snapshot_hash,
            "policy_id": self.policy_id, "policy_version": self.policy_version,
            "permissions": self.permissions, "tools": self.tool_identities,
            "workspace_scope_count": self.workspace_anchors.len(),
            "network_scope_count": self.network_routes.len(), "limits": self.limits,
        })
    }
}

fn valid_text(value: &str) -> bool {
    !value.contains('\0') && value.chars().all(|c| !c.is_control())
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> CapabilitySnapshotV1 {
        CapabilitySnapshotV1 {
            snapshot_id: "snapshot".into(),
            run_id: "run".into(),
            session_id: "session".into(),
            task_id: "task".into(),
            parent_snapshot_hash: None,
            policy_id: "policy".into(),
            policy_version: 1,
            policy_hash: "a".repeat(64),
            manifest_hash: "b".repeat(64),
            workspace_anchors: vec!["workspace".into()],
            operation_scopes: vec!["read".into()],
            permissions: vec!["filesystem_read".into()],
            tool_identities: vec!["files.read".into()],
            network_routes: vec![],
            adapter_scopes: vec![],
            secret_refs: vec![],
            limits: CapabilityLimits {
                timeout_ms: 1000,
                input_bytes: 100,
                output_bytes: 100,
                concurrency: 1,
                tool_calls: 1,
                token_budget: 10,
                cost_micros: 10,
            },
            snapshot_hash: String::new(),
        }
    }

    #[test]
    fn hash_is_stable_and_excludes_cached_hash() {
        let one = snapshot().finalize().unwrap();
        let mut two = one.clone();
        two.snapshot_hash = "f".repeat(64);
        assert_eq!(one.snapshot_hash, two.compute_hash().unwrap());
    }

    #[test]
    fn child_cannot_escalate_budget() {
        let parent = snapshot().finalize().unwrap();
        let mut child = parent.clone();
        child.snapshot_id = "child".into();
        child.parent_snapshot_hash = Some(parent.snapshot_hash.clone());
        child.limits.timeout_ms += 1;
        child.snapshot_hash = child.compute_hash().unwrap();
        assert!(child.is_subset_of(&parent).is_err());
    }

    #[test]
    fn decisions_have_bounded_retry_semantics() {
        assert!(
            PolicyDecision::new(PolicyOutcome::Unavailable, "adapter_missing")
                .unwrap()
                .retryable
        );
        assert!(
            !PolicyDecision::new(PolicyOutcome::Denied, "hard_policy")
                .unwrap()
                .retryable
        );
    }
}
