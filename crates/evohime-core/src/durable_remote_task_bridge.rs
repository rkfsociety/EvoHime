//! Core-owned durable lifecycle for long-running remote tool operations.
//!
//! The bridge stores only typed metadata and artifact references. It does not
//! execute a remote transport; trusted MCP/provider adapters report outcomes
//! back through the same Core-checked state machine.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current schema version for durable remote task records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum byte length of remote task identifiers and references.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum serialized request payload size.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum byte length of a result artifact reference.
pub const MAX_RESULT_REF_BYTES: usize = 512;
/// Maximum operations declared by one remote toolset.
pub const MAX_TOOLSET_OPERATIONS: usize = 64;
/// Maximum poll attempts allowed for one remote task.
pub const MAX_POLL_ATTEMPTS: u32 = 32;

/// Trusted adapter family responsible for a remote operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteProviderKind {
    /// Model Context Protocol provider.
    Mcp,
    /// Registered integration provider.
    IntegrationProvider,
}

/// Durable lifecycle state reported for a remote task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteTaskStatus {
    /// Request is recorded but has not been dispatched.
    Pending,
    /// Remote operation is in progress or being polled.
    Running,
    /// Remote provider requires additional input.
    InputRequired,
    /// Operation completed and produced an artifact reference.
    Completed,
    /// Operation failed.
    Failed,
    /// Cancellation was requested and awaits transport confirmation.
    CancelRequested,
    /// Remote operation confirmed cancellation.
    Cancelled,
    /// Transport outcome cannot be determined safely.
    Unknown,
}

impl RemoteTaskStatus {
    /// Returns whether the task has reached a non-retryable terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Allow-listed remote operations exposed by one provider toolset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTaskToolset {
    /// Toolset schema version.
    pub schema_version: u32,
    /// Stable toolset identifier.
    pub id: String,
    /// Monotonic toolset revision.
    pub version: u64,
    /// Trusted adapter family.
    pub provider_kind: RemoteProviderKind,
    /// Registered provider reference.
    pub provider_ref: String,
    /// Operations permitted by this toolset.
    pub operation_names: Vec<String>,
    /// Digest binding toolset metadata.
    pub content_hash: String,
}

/// Durable state and provenance for one remote operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTaskRecord {
    /// Record schema version.
    pub schema_version: u32,
    /// Stable remote task identifier.
    pub id: String,
    /// Optimistic-concurrency revision.
    pub version: u64,
    /// Toolset that authorized the operation.
    pub toolset_id: String,
    /// Operation selected from the toolset allow-list.
    pub operation: String,
    /// Current remote task lifecycle state.
    pub status: RemoteTaskStatus,
    /// Sanitized adapter transport status.
    pub transport_status: String,
    /// Digest of the original bounded request payload.
    pub request_hash: String,
    /// Optional artifact reference containing the completed result.
    pub result_artifact_ref: Option<String>,
    /// Run or event reference establishing provenance.
    pub provenance_ref: String,
    /// Number of poll leases already issued.
    pub poll_attempts: u32,
    /// Optional earliest next poll time in Unix milliseconds.
    pub next_poll_at_ms: Option<i64>,
    /// Current poll lease owner, if a poll is active.
    pub lease_owner: Option<String>,
    /// Unix timestamp in milliseconds when the task was created.
    pub created_at_ms: i64,
    /// Unix timestamp in milliseconds when the record last changed.
    pub updated_at_ms: i64,
    /// Digest of this record with this field cleared.
    pub content_hash: String,
}

/// Resource limits applied to remote task creation and polling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTaskPolicy {
    /// Policy schema version.
    pub schema_version: u32,
    /// Maximum number of durable remote tasks.
    pub max_tasks: usize,
    /// Maximum polling attempts per task.
    pub max_poll_attempts: u32,
    /// Maximum request payload size in bytes.
    pub max_payload_bytes: usize,
    /// Maximum result artifact reference length.
    pub max_result_ref_bytes: usize,
}

/// Returns the conservative built-in remote task limits.
pub fn default_policy() -> RemoteTaskPolicy {
    RemoteTaskPolicy {
        schema_version: SCHEMA_VERSION,
        max_tasks: 256,
        max_poll_attempts: MAX_POLL_ATTEMPTS,
        max_payload_bytes: MAX_PAYLOAD_BYTES,
        max_result_ref_bytes: MAX_RESULT_REF_BYTES,
    }
}

/// Remote task validation, version, lifecycle, lease, or resource-limit failure.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RemoteTaskError {
    /// Stored data uses an unsupported schema version.
    #[error("unsupported remote task schema version {0}")]
    UnsupportedVersion(u32),
    /// Identifier, reference, or integrity metadata is invalid.
    #[error("invalid remote task identifier or reference")]
    InvalidIdentifier,
    /// Request payload exceeds the configured size bound.
    #[error("remote task payload limit exceeded")]
    PayloadLimit,
    /// Poll attempt limit was reached.
    #[error("remote task poll budget exhausted")]
    PollLimit,
    /// Requested operation is absent from the toolset allow-list.
    #[error("remote task operation is not allowed by toolset")]
    OperationDenied,
    /// Requested lifecycle transition is invalid.
    #[error("remote task transition is invalid")]
    InvalidTransition,
    /// Caller expected an obsolete record version.
    #[error("remote task version is stale")]
    StaleVersion,
    /// Poll lease belongs to another owner.
    #[error("remote task is not leased by this owner")]
    LeaseDenied,
    /// Completed result is missing its required artifact reference.
    #[error("remote task result must use an artifact reference")]
    InvalidResult,
    /// Canonical record serialization failed.
    #[error("remote task serialization failed")]
    Serialization,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:/".contains(&b))
}

/// Checks policy version and ensures configured bounds stay within hard limits.
pub fn validate_policy(policy: &RemoteTaskPolicy) -> Result<(), RemoteTaskError> {
    if policy.schema_version != SCHEMA_VERSION
        || policy.max_tasks == 0
        || policy.max_tasks > 256
        || policy.max_poll_attempts == 0
        || policy.max_poll_attempts > MAX_POLL_ATTEMPTS
        || policy.max_payload_bytes == 0
        || policy.max_payload_bytes > MAX_PAYLOAD_BYTES
        || policy.max_result_ref_bytes == 0
        || policy.max_result_ref_bytes > MAX_RESULT_REF_BYTES
    {
        return Err(RemoteTaskError::PayloadLimit);
    }
    Ok(())
}

/// Validates toolset identity, operation allow-list, and provider reference.
pub fn validate_toolset(
    toolset: &RemoteTaskToolset,
    policy: &RemoteTaskPolicy,
) -> Result<(), RemoteTaskError> {
    validate_policy(policy)?;
    if toolset.schema_version != SCHEMA_VERSION {
        return Err(RemoteTaskError::UnsupportedVersion(toolset.schema_version));
    }
    if !valid_id(&toolset.id)
        || !valid_id(&toolset.provider_ref)
        || toolset.operation_names.is_empty()
        || toolset.operation_names.len() > MAX_TOOLSET_OPERATIONS
        || toolset.content_hash.is_empty()
        || toolset.operation_names.iter().any(|name| !valid_id(name))
    {
        return Err(RemoteTaskError::InvalidIdentifier);
    }
    Ok(())
}

/// Validates task identity, toolset authorization, result reference, and poll bounds.
pub fn validate_record(
    record: &RemoteTaskRecord,
    toolset: &RemoteTaskToolset,
    policy: &RemoteTaskPolicy,
) -> Result<(), RemoteTaskError> {
    validate_toolset(toolset, policy)?;
    if record.schema_version != SCHEMA_VERSION
        || !valid_id(&record.id)
        || !valid_id(&record.toolset_id)
        || !valid_id(&record.operation)
        || record.toolset_id != toolset.id
        || !toolset
            .operation_names
            .iter()
            .any(|name| name == &record.operation)
        || record.transport_status.is_empty()
        || record.request_hash.is_empty()
        || !valid_id(&record.provenance_ref)
        || record.content_hash.is_empty()
        || record.poll_attempts > policy.max_poll_attempts
        || record
            .result_artifact_ref
            .as_deref()
            .is_some_and(|value| value.len() > policy.max_result_ref_bytes || !valid_id(value))
    {
        return Err(RemoteTaskError::InvalidIdentifier);
    }
    if record.status == RemoteTaskStatus::Completed && record.result_artifact_ref.is_none() {
        return Err(RemoteTaskError::InvalidResult);
    }
    Ok(())
}

/// Validates payload size and computes the request's SHA-256 digest.
pub fn request_hash(payload: &[u8], policy: &RemoteTaskPolicy) -> Result<String, RemoteTaskError> {
    validate_policy(policy)?;
    if payload.len() > policy.max_payload_bytes {
        return Err(RemoteTaskError::PayloadLimit);
    }
    Ok(hex::encode(Sha256::digest(payload)))
}

/// Creates and validates an undispatched task for an allow-listed operation.
pub fn build_record(
    id: String,
    toolset: &RemoteTaskToolset,
    operation: String,
    payload: &[u8],
    provenance_ref: String,
    now_ms: i64,
    policy: &RemoteTaskPolicy,
) -> Result<RemoteTaskRecord, RemoteTaskError> {
    validate_toolset(toolset, policy)?;
    if !toolset
        .operation_names
        .iter()
        .any(|name| name == &operation)
    {
        return Err(RemoteTaskError::OperationDenied);
    }
    let record = RemoteTaskRecord {
        schema_version: SCHEMA_VERSION,
        id,
        version: 1,
        toolset_id: toolset.id.clone(),
        operation,
        status: RemoteTaskStatus::Pending,
        transport_status: "not_dispatched".into(),
        request_hash: request_hash(payload, policy)?,
        result_artifact_ref: None,
        provenance_ref,
        poll_attempts: 0,
        next_poll_at_ms: None,
        lease_owner: None,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        content_hash: String::new(),
    };
    let mut record = record;
    refresh_content_hash(&mut record)?;
    validate_record(&record, toolset, policy)?;
    Ok(record)
}

/// Recomputes a task record's integrity digest after a state change.
pub fn refresh_content_hash(record: &mut RemoteTaskRecord) -> Result<(), RemoteTaskError> {
    record.content_hash.clear();
    let bytes = serde_json::to_vec(record).map_err(|_| RemoteTaskError::Serialization)?;
    record.content_hash = hex::encode(Sha256::digest(bytes));
    Ok(())
}

/// Requests cancellation using an expected record revision.
pub fn cancel(
    record: &mut RemoteTaskRecord,
    expected_version: u64,
    now_ms: i64,
) -> Result<(), RemoteTaskError> {
    if record.version != expected_version || record.status.is_terminal() {
        return Err(if record.version != expected_version {
            RemoteTaskError::StaleVersion
        } else {
            RemoteTaskError::InvalidTransition
        });
    }
    record.version += 1;
    record.status = if matches!(record.status, RemoteTaskStatus::Pending) {
        RemoteTaskStatus::Cancelled
    } else {
        RemoteTaskStatus::CancelRequested
    };
    record.transport_status = "cancel_requested".into();
    record.updated_at_ms = now_ms;
    Ok(())
}

/// Acquires the next bounded poll attempt for one owner.
pub fn lease_for_poll(
    record: &mut RemoteTaskRecord,
    owner: &str,
    now_ms: i64,
    policy: &RemoteTaskPolicy,
) -> Result<(), RemoteTaskError> {
    validate_policy(policy)?;
    if !valid_id(owner)
        || record.status.is_terminal()
        || record.status == RemoteTaskStatus::CancelRequested
        || record.poll_attempts >= policy.max_poll_attempts
    {
        return Err(if record.poll_attempts >= policy.max_poll_attempts {
            RemoteTaskError::PollLimit
        } else {
            RemoteTaskError::InvalidTransition
        });
    }
    if record
        .lease_owner
        .as_deref()
        .is_some_and(|current| current != owner)
    {
        return Err(RemoteTaskError::LeaseDenied);
    }
    record.lease_owner = Some(owner.into());
    record.poll_attempts += 1;
    record.status = RemoteTaskStatus::Running;
    record.transport_status = "polling".into();
    record.updated_at_ms = now_ms;
    Ok(())
}

/// Projects durable task status without request payload or credential data.
pub fn status_projection(record: &RemoteTaskRecord) -> serde_json::Value {
    serde_json::json!({
        "schema_version": record.schema_version,
        "remote_task_id": record.id,
        "version": record.version,
        "toolset_id": record.toolset_id,
        "operation": record.operation,
        "status": record.status,
        "transport_status": record.transport_status,
        "poll_attempts": record.poll_attempts,
        "next_poll_at_ms": record.next_poll_at_ms,
        "result_artifact_ref": record.result_artifact_ref,
        "provenance_ref": record.provenance_ref,
        "content_hash": record.content_hash,
        "redacted": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toolset() -> RemoteTaskToolset {
        RemoteTaskToolset {
            schema_version: 1,
            id: "mcp-docs".into(),
            version: 1,
            provider_kind: RemoteProviderKind::Mcp,
            provider_ref: "mcp.docs".into(),
            operation_names: vec!["search".into()],
            content_hash: "toolset-hash".into(),
        }
    }

    #[test]
    fn lifecycle_is_bounded_and_cancel_is_not_blind_retry() {
        let policy = default_policy();
        let mut record = build_record(
            "remote-1".into(),
            &toolset(),
            "search".into(),
            b"{}",
            "run-1".into(),
            1,
            &policy,
        )
        .unwrap();
        lease_for_poll(&mut record, "core", 2, &policy).unwrap();
        assert_eq!(record.status, RemoteTaskStatus::Running);
        let previous_hash = record.content_hash.clone();
        let version = record.version;
        cancel(&mut record, version, 3).unwrap();
        refresh_content_hash(&mut record).unwrap();
        assert_eq!(record.status, RemoteTaskStatus::CancelRequested);
        assert_eq!(record.transport_status, "cancel_requested");
        assert_ne!(record.content_hash, previous_hash);
    }

    #[test]
    fn completed_requires_artifact_reference() {
        let policy = default_policy();
        let mut record = build_record(
            "remote-1".into(),
            &toolset(),
            "search".into(),
            b"{}",
            "run-1".into(),
            1,
            &policy,
        )
        .unwrap();
        record.status = RemoteTaskStatus::Completed;
        assert_eq!(
            validate_record(&record, &toolset(), &policy),
            Err(RemoteTaskError::InvalidResult)
        );
    }
}
