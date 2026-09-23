use crate::ReceiptError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

/// Failure returned by receipt runtime persistence and signing operations.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Stable contract-level failure code.
    #[error("receipt.{0}")]
    Code(&'static str),
    /// SQLite operation failed.
    #[error("receipt.sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The receipt payload or its invariants were invalid.
    #[error("receipt.contract: {0}")]
    Contract(#[from] ReceiptError),
    /// No receipt signer could be loaded for the requested operation.
    #[error("receipt.signer_unavailable")]
    SignerUnavailable,
}

/// Persisted integrity projection for a completed or recoverable tool action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtectedActionRow {
    /// Version of the protected-row representation.
    pub schema_version: u8,
    /// Stable action identifier.
    pub action_id: String,
    /// Digest of the pre-execution receipt.
    pub pre_receipt_hash: String,
    /// Digest of canonical tool arguments.
    pub tool_args_hash: String,
    /// Terminal or recovery status recorded for the action.
    pub result_status: String,
    /// Digest of the bounded result projection.
    pub result_hash: String,
    /// Stable recovery classification code.
    pub recovery_code: String,
    /// Row creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Signer identity used to protect this row.
    pub key_id: String,
}

/// Policy result governing whether an action may be dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Policy permits execution without an approval step.
    Allow,
    /// Policy refuses execution.
    Deny,
    /// Execution may proceed only after an approval is granted.
    ApprovalRequired,
}

impl PolicyDecision {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::ApprovalRequired => "approval_required",
        }
    }
}

/// Validated action inputs passed into receipt preparation.
#[derive(Debug, Clone)]
pub struct ActionRequest {
    /// Stable identifier for this action attempt.
    pub action_id: Uuid,
    /// Task that owns the action.
    pub task_id: String,
    /// Runtime run associated with the action.
    pub run_id: String,
    /// Registered tool requested by the caller.
    pub tool_name: String,
    /// Policy that evaluated the action.
    pub policy_id: String,
    /// Normalized scope used for policy matching.
    pub normalized_scope: String,
    /// Structured tool input covered by the prepared receipt.
    pub input: Value,
    /// Outcome of policy evaluation before preparation.
    pub policy_decision: PolicyDecision,
    /// Approval record associated with this action, when required.
    pub approval_id: Option<Uuid>,
    /// Parent approval reference for a derived or delegated action.
    pub parent_approval_ref: Option<String>,
    /// Human-readable preview shown before an approval decision.
    pub preview: String,
}

/// Result of preparing an action for dispatch or approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareOutcome {
    /// Action has a prepared receipt and may be dispatched.
    Prepared {
        /// Identifier of the prepared action.
        action_id: Uuid,
        /// Hash of the pre-execution receipt.
        receipt_hash: String,
    },
    /// Action is waiting for the referenced approval to be granted.
    ApprovalRequired {
        /// Identifier of the action awaiting approval.
        action_id: Uuid,
        /// Identifier of the approval request.
        approval_id: Uuid,
        /// Approval expiry time in Unix milliseconds.
        expires_at_ms: i64,
    },
    /// Policy refused the action and a refusal receipt was recorded.
    Refused {
        /// Identifier of the refused action.
        action_id: Uuid,
        /// Hash of the refusal receipt.
        receipt_hash: String,
        /// Stable refusal reason code.
        code: String,
    },
}

/// Current persisted dispatch and receipt state for an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionState {
    /// Stable action identifier.
    pub action_id: String,
    /// High-level action lifecycle state.
    pub state: String,
    /// Tool dispatch lifecycle state.
    pub dispatch_state: String,
    /// Hash of the pre-execution receipt, when prepared.
    pub pre_receipt_hash: Option<String>,
    /// Hash of the terminal receipt, when finalized.
    pub terminal_receipt_hash: Option<String>,
    /// Hash of canonical tool arguments.
    pub tool_args_hash: String,
}

/// Snapshot counters for pending, recoverable, and quarantined receipt rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCounts {
    /// Actions awaiting dispatch or completion.
    pub pending: i64,
    /// Actions requiring recovery after interruption.
    pub pending_recovery: i64,
    /// Actions moved into quarantine for investigation.
    pub quarantined: i64,
    /// Actions waiting for user approval.
    pub approval_pending: i64,
}

/// Named receipt-runtime metric counters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    /// Counter values keyed by their stable metric name.
    pub counters: BTreeMap<String, i64>,
}

/// Progress cursor for re-encrypting persisted receipt data under a new key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRotationJob {
    /// Unique identifier of this storage-key rotation job.
    pub job_id: String,
    /// Storage key identifier used before rotation.
    pub old_key_id: String,
    /// Storage key identifier used after rotation.
    pub new_key_id: String,
    /// Opaque position from which the next batch should resume.
    pub cursor: String,
    /// Monotonic generation used to reject stale workers.
    pub generation: i64,
    /// Current job lifecycle state.
    pub state: String,
}

/// Stage 01.4 `ReceiptCheckpointV1` (durable columns; the signed canonical
/// bytes themselves stay in `receipt_checkpoints.canonical_checkpoint` and
/// are not duplicated here). `signature` is Ed25519 over the SHA-256 digest
/// of those canonical bytes, matching the receipt-append signing scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptCheckpointRow {
    /// Identifier of the signed checkpoint.
    pub checkpoint_id: String,
    /// Receipt signing key that created the checkpoint.
    pub key_id: String,
    /// Highest receipt sequence covered by the checkpoint.
    pub cutoff_sequence: i64,
    /// Hash of the earliest receipt retained after pruning.
    pub first_retained_hash: String,
    /// Hash at the end of the checkpointed prefix.
    pub prefix_last_hash: String,
    /// Hash of the final receipt removed by pruning.
    pub last_deleted_receipt_hash: String,
    /// Hash of the receipt-chain head when the checkpoint was created.
    pub head_receipt_hash: String,
    /// UTC creation timestamp.
    pub created_at: String,
    /// Key identity that signed the canonical checkpoint bytes.
    pub signed_by_key_id: String,
    /// Base64-encoded Ed25519 signature over the checkpoint digest.
    pub signature: String,
    /// Current persistence/verification status of the checkpoint.
    pub status: String,
}

/// Signed request-commit receipt appended to the same Ed25519 chain as tool
/// receipts. The payload contains only identifiers and hashes; prompt bytes
/// remain in the model-provenance block store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedModelRequestReceipt {
    /// Identifier of the request receipt.
    pub receipt_id: String,
    /// Identifier of the logical model request.
    pub request_id: String,
    /// Digest of the canonical request receipt payload.
    pub receipt_hash: String,
    /// Canonical signed payload bytes containing identifiers and hashes only.
    pub canonical_payload: Vec<u8>,
    /// Hash of the preceding receipt in the shared chain, when present.
    pub previous_receipt_hash: Option<String>,
    /// Key identity used to sign this receipt.
    pub key_id: String,
    /// Receipt creation time in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Signing boundary. The signer receives the SHA-256 digest of canonical
/// payload bytes, never raw tool input or a mutable JSON representation.
pub trait ReceiptSigner: Send + Sync {
    /// Returns the stable identifier of the active signing key.
    fn key_id(&self) -> Result<String, RuntimeError>;
    /// Signs a lowercase hexadecimal SHA-256 digest of canonical payload bytes.
    fn sign_payload_hash(&self, payload_hash: &str) -> Result<String, RuntimeError>;
}
