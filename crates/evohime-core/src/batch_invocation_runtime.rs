//! Durable, provider-neutral batch invocation state machine.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized schema version supported by the batch runtime.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum items accepted in one batch.
pub const MAX_ITEMS: usize = 256;
/// Maximum parallel item executions supported by the runtime.
pub const MAX_CONCURRENCY: u32 = 16;
/// Maximum serialized input payload size in bytes.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum length of an artifact reference.
pub const MAX_REF_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Aggregate lifecycle state of a batch invocation.
pub enum BatchStatus {
    /// The batch or item is waiting to be scheduled.
    Pending,
    /// The batch or item currently has active execution.
    Running,
    /// All required work completed and results were recorded.
    Completed,
    /// At least one item failed or has an unknown outcome.
    Partial,
    /// Batch execution failed before successful completion.
    Failed,
    /// Execution was cancelled.
    Cancelled,
    /// The item may have executed, but its outcome could not be recovered.
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle and outcome state of one batch item.
pub enum ItemStatus {
    /// The batch or item is waiting to be scheduled.
    Pending,
    /// The batch or item currently has active execution.
    Running,
    /// All required work completed and results were recorded.
    Completed,
    /// Batch execution failed before successful completion.
    Failed,
    /// Execution is waiting for required user approval.
    ApprovalRequired,
    /// The item may have executed, but its outcome could not be recovered.
    Unknown,
    /// Execution was cancelled.
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Whether item failures stop or allow the remaining batch work.
pub enum FailurePolicy {
    /// Continue scheduling other items when one item fails.
    Continue,
    /// Stop scheduling pending items after the first failure.
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Versioned durable state for a provider-neutral batch run.
pub struct BatchInvocation {
    /// Serialized schema version supported by this runtime.
    pub schema_version: u32,
    /// Stable identifier for the batch or item.
    pub id: String,
    /// Monotonic batch revision used for stale-write detection.
    pub version: u64,
    /// Reference to the immutable batch definition.
    pub definition_ref: String,
    /// Exact definition revision bound to the batch.
    pub definition_version: u64,
    /// Ordered work items submitted to the invocation.
    pub items: Vec<BatchItem>,
    /// Maximum number of items allowed to run simultaneously.
    pub max_concurrency: u32,
    /// Policy for scheduling remaining items after a failure.
    pub failure_policy: FailurePolicy,
    /// Outcome to persist for the selected item.
    pub status: BatchStatus,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Last state change time as Unix epoch milliseconds.
    pub updated_at_ms: i64,
    /// Integrity hash of canonical batch content.
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Input, execution attempt, and result state for one batch item.
pub struct BatchItem {
    /// Stable identifier for this item within the batch.
    pub item_id: String,
    /// Zero-based position preserving input order.
    pub ordinal: u32,
    /// Serialized input assigned to this item.
    pub input_payload: String,
    /// SHA-256 hash of the item input payload.
    pub input_hash: String,
    /// Outcome to persist for the selected item.
    pub status: ItemStatus,
    /// Current or most recent execution run identifier.
    pub run_id: Option<String>,
    /// Number of recorded execution attempts.
    pub attempts: u32,
    /// Artifact reference required for a completed item.
    pub result_ref: Option<String>,
    /// Optional failure category reported for the item.
    pub error_class: Option<String>,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Last state change time as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Hard limits applied when creating and updating a batch.
pub struct BatchPolicy {
    /// Serialized schema version supported by this runtime.
    pub schema_version: u32,
    /// Maximum number of items accepted in one batch.
    pub max_items: usize,
    /// Maximum number of items allowed to run simultaneously.
    pub max_concurrency: u32,
    /// Maximum execution attempts per item.
    pub max_attempts: u32,
}
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
/// Validation, limit, concurrency, or result contract failure.
pub enum BatchError {
    /// The serialized schema version is unsupported.
    #[error("unsupported batch schema {0}")]
    UnsupportedVersion(u32),
    /// The batch state or item reference is invalid.
    #[error("invalid batch contract")]
    Invalid,
    /// A policy or payload bound was exceeded.
    #[error("batch bound exceeded")]
    Limit,
    /// The caller supplied an outdated batch revision.
    #[error("batch version is stale")]
    Stale,
    /// An unknown outcome cannot be retried automatically.
    #[error("unknown outcome cannot be retried")]
    UnknownRetry,
    /// A completed item must reference a result artifact.
    #[error("result must be an artifact reference")]
    InvalidResult,
}
/// Returns the standard bounded batch execution policy.
pub fn default_policy() -> BatchPolicy {
    BatchPolicy {
        schema_version: 1,
        max_items: MAX_ITEMS,
        max_concurrency: MAX_CONCURRENCY,
        max_attempts: 3,
    }
}
fn valid(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:/".contains(&b))
}
/// Returns the SHA-256 hash of a payload within the supported size limit.
pub fn input_hash(input: &str) -> Result<String, BatchError> {
    if input.len() > MAX_PAYLOAD_BYTES {
        return Err(BatchError::Limit);
    }
    Ok(hex::encode(Sha256::digest(input.as_bytes())))
}
/// Computes the batch integrity hash with the hash field cleared.
pub fn canonical_hash(batch: &BatchInvocation) -> String {
    let mut copy = batch.clone();
    copy.content_hash.clear();
    hex::encode(Sha256::digest(
        serde_json::to_vec(&copy).unwrap_or_default(),
    ))
}
/// Validates batch structure, item hashes, policy limits, and result references.
pub fn validate(batch: &BatchInvocation, policy: &BatchPolicy) -> Result<(), BatchError> {
    if policy.schema_version != SCHEMA_VERSION {
        return Err(BatchError::UnsupportedVersion(policy.schema_version));
    }
    if batch.schema_version != SCHEMA_VERSION {
        return Err(BatchError::UnsupportedVersion(batch.schema_version));
    }
    if !valid(&batch.id)
        || !valid(&batch.definition_ref)
        || batch.definition_version == 0
        || batch.version == 0
        || batch.items.is_empty()
        || batch.items.len() > policy.max_items
        || batch.max_concurrency == 0
        || batch.max_concurrency > policy.max_concurrency
        || batch.content_hash != canonical_hash(batch)
    {
        return Err(BatchError::Invalid);
    }
    for (index, item) in batch.items.iter().enumerate() {
        if !valid(&item.item_id)
            || item.ordinal != index as u32
            || item.input_payload.len() > MAX_PAYLOAD_BYTES
            || item.input_hash != input_hash(&item.input_payload)?
            || item.attempts > policy.max_attempts
            || item
                .result_ref
                .as_deref()
                .is_some_and(|v| !valid(v) || v.len() > MAX_REF_BYTES)
        {
            return Err(BatchError::Invalid);
        }
        if item.status == ItemStatus::Completed && item.result_ref.is_none() {
            return Err(BatchError::InvalidResult);
        }
    }
    Ok(())
}
/// Validated construction inputs for creating a durable batch.
pub struct NewBatchInput {
    /// Stable identifier for the batch or item.
    pub id: String,
    /// Reference to the immutable batch definition.
    pub definition_ref: String,
    /// Exact definition revision bound to the batch.
    pub definition_version: u64,
    /// Input payloads to create as ordered independent items.
    pub inputs: Vec<String>,
    /// Maximum number of items allowed to run simultaneously.
    pub max_concurrency: u32,
    /// Policy for scheduling remaining items after a failure.
    pub failure_policy: FailurePolicy,
    /// Current operation time as Unix epoch milliseconds.
    pub now_ms: i64,
    /// Hard limits validated before the operation is committed.
    pub policy: BatchPolicy,
}

/// Wire-вход создания batch без промежуточного `serde_json::Value`.
#[derive(Debug, Deserialize)]
/// Wire request for creating a batch invocation.
pub struct CreateBatchRequest {
    /// Input payloads to create as ordered independent items.
    pub inputs: Vec<String>,
    /// Reference to the immutable batch definition.
    pub definition_ref: String,
    /// Exact definition revision bound to the batch.
    pub definition_version: u64,
    #[serde(default = "default_max_concurrency")]
    /// Maximum number of items allowed to run simultaneously.
    pub max_concurrency: u32,
    #[serde(default = "default_failure_policy")]
    /// Policy for scheduling remaining items after a failure.
    pub failure_policy: FailurePolicy,
}

fn default_max_concurrency() -> u32 {
    1
}

fn default_failure_policy() -> FailurePolicy {
    FailurePolicy::Continue
}

/// Wire-вход завершения одного элемента batch.
#[derive(Debug, Deserialize)]
/// Wire request for recording one item outcome.
pub struct RecordResultRequest {
    /// Stable identifier for this item within the batch.
    pub item_id: String,
    /// Outcome to persist for the selected item.
    pub status: ItemStatus,
    /// Artifact reference required for a completed item.
    pub result_ref: Option<String>,
    /// Optional failure category reported for the item.
    pub error_class: Option<String>,
}

/// Creates independent ordered items and seals them with a canonical hash.
pub fn new_batch(input: NewBatchInput) -> Result<BatchInvocation, BatchError> {
    if input.inputs.is_empty()
        || input.inputs.len() > input.policy.max_items
        || !valid(&input.id)
        || !valid(&input.definition_ref)
        || input.definition_version == 0
        || input.max_concurrency == 0
        || input.max_concurrency > input.policy.max_concurrency
    {
        return Err(BatchError::Limit);
    }
    let items = input
        .inputs
        .into_iter()
        .enumerate()
        .map(|(ordinal, input_payload)| {
            Ok(BatchItem {
                item_id: format!("{}:{ordinal}", input.id),
                ordinal: ordinal as u32,
                input_hash: input_hash(&input_payload)?,
                input_payload,
                status: ItemStatus::Pending,
                run_id: Some(format!("{}:{ordinal}:run:0", input.id)),
                attempts: 0,
                result_ref: None,
                error_class: None,
                created_at_ms: input.now_ms,
                updated_at_ms: input.now_ms,
            })
        })
        .collect::<Result<Vec<_>, BatchError>>()?;
    let mut batch = BatchInvocation {
        schema_version: 1,
        id: input.id,
        version: 1,
        definition_ref: input.definition_ref,
        definition_version: input.definition_version,
        items,
        max_concurrency: input.max_concurrency,
        failure_policy: input.failure_policy,
        status: BatchStatus::Pending,
        created_at_ms: input.now_ms,
        updated_at_ms: input.now_ms,
        content_hash: String::new(),
    };
    batch.content_hash = canonical_hash(&batch);
    validate(&batch, &input.policy)?;
    Ok(batch)
}
/// Marks interrupted running items unknown and returns the number left pending.
pub fn resume_pending(
    batch: &mut BatchInvocation,
    expected_version: u64,
    now_ms: i64,
    policy: &BatchPolicy,
) -> Result<usize, BatchError> {
    if batch.version != expected_version {
        return Err(BatchError::Stale);
    }
    validate(batch, policy)?;
    let mut resumed = 0;
    for item in &mut batch.items {
        if item.status == ItemStatus::Running {
            item.status = ItemStatus::Unknown;
            item.error_class = Some("unknown_after_restart".into());
        }
        if item.status == ItemStatus::Pending {
            resumed += 1;
        }
    }
    batch.version += 1;
    batch.updated_at_ms = now_ms;
    batch.content_hash = canonical_hash(batch);
    Ok(resumed)
}
/// Starts pending items up to the configured concurrency limit.
pub fn start_batch(
    batch: &mut BatchInvocation,
    expected_version: u64,
    now_ms: i64,
    policy: &BatchPolicy,
) -> Result<usize, BatchError> {
    if batch.version != expected_version {
        return Err(BatchError::Stale);
    }
    validate(batch, policy)?;
    let active = batch
        .items
        .iter()
        .filter(|item| item.status == ItemStatus::Running)
        .count() as u32;
    let capacity = batch.max_concurrency.saturating_sub(active) as usize;
    let mut started = 0;
    for item in &mut batch.items {
        if started >= capacity || item.status != ItemStatus::Pending {
            continue;
        }
        item.status = ItemStatus::Running;
        item.run_id = Some(format!("{}:run:{}", item.item_id, item.attempts));
        item.updated_at_ms = now_ms;
        started += 1;
    }
    if started > 0 {
        batch.status = BatchStatus::Running;
        batch.version += 1;
        batch.updated_at_ms = now_ms;
        batch.content_hash = canonical_hash(batch);
    }
    Ok(started)
}
/// Mutable batch state and versioned inputs for recording an item result.
pub struct RecordResultInput<'a> {
    /// Mutable batch whose revision and item state will be updated.
    pub batch: &'a mut BatchInvocation,
    /// Stable identifier for this item within the batch.
    pub item_id: &'a str,
    /// Batch revision the caller read before this update.
    pub expected_version: u64,
    /// Outcome to persist for the selected item.
    pub status: ItemStatus,
    /// Artifact reference required for a completed item.
    pub result_ref: Option<String>,
    /// Optional failure category reported for the item.
    pub error_class: Option<String>,
    /// Current operation time as Unix epoch milliseconds.
    pub now_ms: i64,
    /// Hard limits validated before the operation is committed.
    pub policy: &'a BatchPolicy,
}

/// Records one item outcome with optimistic revision and policy checks.
pub fn record_result(input: RecordResultInput<'_>) -> Result<(), BatchError> {
    if input.batch.version != input.expected_version {
        return Err(BatchError::Stale);
    }
    let item = input
        .batch
        .items
        .iter_mut()
        .find(|item| item.item_id == input.item_id)
        .ok_or(BatchError::Invalid)?;
    if item.status == ItemStatus::Unknown && input.status == ItemStatus::Running {
        return Err(BatchError::UnknownRetry);
    }
    if input.status == ItemStatus::Completed && input.result_ref.is_none() {
        return Err(BatchError::InvalidResult);
    }
    item.status = input.status;
    item.result_ref = input.result_ref;
    item.error_class = input.error_class;
    item.attempts = item.attempts.saturating_add(1);
    item.updated_at_ms = input.now_ms;
    input.batch.version += 1;
    input.batch.status = if input
        .batch
        .items
        .iter()
        .all(|i| matches!(i.status, ItemStatus::Completed))
    {
        BatchStatus::Completed
    } else if input
        .batch
        .items
        .iter()
        .any(|i| matches!(i.status, ItemStatus::Failed | ItemStatus::Unknown))
    {
        BatchStatus::Partial
    } else {
        BatchStatus::Running
    };
    input.batch.updated_at_ms = input.now_ms;
    input.batch.content_hash = canonical_hash(input.batch);
    validate(input.batch, input.policy)
}
/// Builds a redacted JSON read model for UI or transport.
pub fn projection(batch: &BatchInvocation) -> serde_json::Value {
    serde_json::json!({"schema_version":batch.schema_version,"batch_id":batch.id,"version":batch.version,"definition_ref":batch.definition_ref,"definition_version":batch.definition_version,"status":batch.status,"max_concurrency":batch.max_concurrency,"item_count":batch.items.len(),"completed":batch.items.iter().filter(|i|i.status==ItemStatus::Completed).count(),"failed":batch.items.iter().filter(|i|matches!(i.status,ItemStatus::Failed|ItemStatus::Unknown)).count(),"pending":batch.items.iter().filter(|i|i.status==ItemStatus::Pending).count(),"items":batch.items.iter().map(|i|serde_json::json!({"item_id":i.item_id,"ordinal":i.ordinal,"status":i.status,"attempts":i.attempts,"run_id":i.run_id,"result_ref":i.result_ref,"error_class":i.error_class})).collect::<Vec<_>>(),"content_hash":batch.content_hash,"redacted":true})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creates_isolated_items_and_hashes_inputs() {
        let b = new_batch(NewBatchInput {
            id: "b".into(),
            definition_ref: "workflow".into(),
            definition_version: 1,
            inputs: vec!["a".into(), "b".into()],
            max_concurrency: 2,
            failure_policy: FailurePolicy::Continue,
            now_ms: 1,
            policy: default_policy(),
        })
        .unwrap();
        assert_eq!(b.items[0].item_id, "b:0");
        assert_ne!(b.items[0].input_hash, b.items[1].input_hash);
    }
    #[test]
    fn restart_marks_inflight_unknown_without_retry() {
        let mut b = new_batch(NewBatchInput {
            id: "b".into(),
            definition_ref: "workflow".into(),
            definition_version: 1,
            inputs: vec!["a".into()],
            max_concurrency: 1,
            failure_policy: FailurePolicy::Continue,
            now_ms: 1,
            policy: default_policy(),
        })
        .unwrap();
        b.items[0].status = ItemStatus::Running;
        b.content_hash = canonical_hash(&b);
        assert_eq!(resume_pending(&mut b, 1, 2, &default_policy()).unwrap(), 0);
        assert_eq!(b.items[0].status, ItemStatus::Unknown);
        assert_eq!(
            record_result(RecordResultInput {
                batch: &mut b,
                item_id: "b:0",
                expected_version: 2,
                status: ItemStatus::Running,
                result_ref: None,
                error_class: None,
                now_ms: 3,
                policy: &default_policy(),
            }),
            Err(BatchError::UnknownRetry)
        );
    }
}
