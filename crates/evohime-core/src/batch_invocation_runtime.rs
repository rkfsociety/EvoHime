//! Durable, provider-neutral batch invocation state machine.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 256;
pub const MAX_CONCURRENCY: u32 = 16;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_REF_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Pending,
    Running,
    Completed,
    Partial,
    Failed,
    Cancelled,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Pending,
    Running,
    Completed,
    Failed,
    ApprovalRequired,
    Unknown,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    Continue,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchInvocation {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub definition_ref: String,
    pub definition_version: u64,
    pub items: Vec<BatchItem>,
    pub max_concurrency: u32,
    pub failure_policy: FailurePolicy,
    pub status: BatchStatus,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchItem {
    pub item_id: String,
    pub ordinal: u32,
    pub input_payload: String,
    pub input_hash: String,
    pub status: ItemStatus,
    pub run_id: Option<String>,
    pub attempts: u32,
    pub result_ref: Option<String>,
    pub error_class: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchPolicy {
    pub schema_version: u32,
    pub max_items: usize,
    pub max_concurrency: u32,
    pub max_attempts: u32,
}
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BatchError {
    #[error("unsupported batch schema {0}")]
    UnsupportedVersion(u32),
    #[error("invalid batch contract")]
    Invalid,
    #[error("batch bound exceeded")]
    Limit,
    #[error("batch version is stale")]
    Stale,
    #[error("unknown outcome cannot be retried")]
    UnknownRetry,
    #[error("result must be an artifact reference")]
    InvalidResult,
}
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
pub fn input_hash(input: &str) -> Result<String, BatchError> {
    if input.len() > MAX_PAYLOAD_BYTES {
        return Err(BatchError::Limit);
    }
    Ok(hex::encode(Sha256::digest(input.as_bytes())))
}
pub fn canonical_hash(batch: &BatchInvocation) -> String {
    let mut copy = batch.clone();
    copy.content_hash.clear();
    hex::encode(Sha256::digest(
        serde_json::to_vec(&copy).unwrap_or_default(),
    ))
}
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
pub struct NewBatchInput {
    pub id: String,
    pub definition_ref: String,
    pub definition_version: u64,
    pub inputs: Vec<String>,
    pub max_concurrency: u32,
    pub failure_policy: FailurePolicy,
    pub now_ms: i64,
    pub policy: BatchPolicy,
}

/// Wire-вход создания batch без промежуточного `serde_json::Value`.
#[derive(Debug, Deserialize)]
pub struct CreateBatchRequest {
    pub inputs: Vec<String>,
    pub definition_ref: String,
    pub definition_version: u64,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u32,
    #[serde(default = "default_failure_policy")]
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
pub struct RecordResultRequest {
    pub item_id: String,
    pub status: ItemStatus,
    pub result_ref: Option<String>,
    pub error_class: Option<String>,
}

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
pub struct RecordResultInput<'a> {
    pub batch: &'a mut BatchInvocation,
    pub item_id: &'a str,
    pub expected_version: u64,
    pub status: ItemStatus,
    pub result_ref: Option<String>,
    pub error_class: Option<String>,
    pub now_ms: i64,
    pub policy: &'a BatchPolicy,
}

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
