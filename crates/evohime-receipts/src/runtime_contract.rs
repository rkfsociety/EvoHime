use crate::ReceiptError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("receipt.{0}")]
    Code(&'static str),
    #[error("receipt.sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("receipt.contract: {0}")]
    Contract(#[from] ReceiptError),
    #[error("receipt.signer_unavailable")]
    SignerUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtectedActionRow {
    pub schema_version: u8,
    pub action_id: String,
    pub pre_receipt_hash: String,
    pub tool_args_hash: String,
    pub result_status: String,
    pub result_hash: String,
    pub recovery_code: String,
    pub created_at_ms: i64,
    pub key_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny,
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

#[derive(Debug, Clone)]
pub struct ActionRequest {
    pub action_id: Uuid,
    pub task_id: String,
    pub run_id: String,
    pub tool_name: String,
    pub policy_id: String,
    pub normalized_scope: String,
    pub input: Value,
    pub policy_decision: PolicyDecision,
    pub approval_id: Option<Uuid>,
    pub parent_approval_ref: Option<String>,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareOutcome {
    Prepared {
        action_id: Uuid,
        receipt_hash: String,
    },
    ApprovalRequired {
        action_id: Uuid,
        approval_id: Uuid,
        expires_at_ms: i64,
    },
    Refused {
        action_id: Uuid,
        receipt_hash: String,
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionState {
    pub action_id: String,
    pub state: String,
    pub dispatch_state: String,
    pub pre_receipt_hash: Option<String>,
    pub terminal_receipt_hash: Option<String>,
    pub tool_args_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCounts {
    pub pending: i64,
    pub pending_recovery: i64,
    pub quarantined: i64,
    pub approval_pending: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub counters: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRotationJob {
    pub job_id: String,
    pub old_key_id: String,
    pub new_key_id: String,
    pub cursor: String,
    pub generation: i64,
    pub state: String,
}

/// Stage 01.4 `ReceiptCheckpointV1` (durable columns; the signed canonical
/// bytes themselves stay in `receipt_checkpoints.canonical_checkpoint` and
/// are not duplicated here). `signature` is Ed25519 over the SHA-256 digest
/// of those canonical bytes, matching the receipt-append signing scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptCheckpointRow {
    pub checkpoint_id: String,
    pub key_id: String,
    pub cutoff_sequence: i64,
    pub first_retained_hash: String,
    pub prefix_last_hash: String,
    pub last_deleted_receipt_hash: String,
    pub head_receipt_hash: String,
    pub created_at: String,
    pub signed_by_key_id: String,
    pub signature: String,
    pub status: String,
}

/// Signed request-commit receipt appended to the same Ed25519 chain as tool
/// receipts. The payload contains only identifiers and hashes; prompt bytes
/// remain in the model-provenance block store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedModelRequestReceipt {
    pub receipt_id: String,
    pub request_id: String,
    pub receipt_hash: String,
    pub canonical_payload: Vec<u8>,
    pub previous_receipt_hash: Option<String>,
    pub key_id: String,
    pub created_at_ms: i64,
}

/// Signing boundary. The signer receives the SHA-256 digest of canonical
/// payload bytes, never raw tool input or a mutable JSON representation.
pub trait ReceiptSigner: Send + Sync {
    fn key_id(&self) -> Result<String, RuntimeError>;
    fn sign_payload_hash(&self, payload_hash: &str) -> Result<String, RuntimeError>;
}
