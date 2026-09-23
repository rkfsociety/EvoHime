use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current schema version for kernel capability snapshots.
pub const SCHEMA_VERSION: u32 = 1;
const MAX_ITEMS: usize = 256;
/// Availability result for a capability in a particular kernel snapshot.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// Capability is available under the captured policy and grant.
    Available,
    /// Capability is not currently available.
    Unavailable,
    /// Access was explicitly denied.
    Denied,
    /// Availability could not be determined.
    Unknown,
}
/// Type of asynchronous or durable object exposed through an opaque handle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HandleKind {
    /// Child agent execution.
    Child,
    /// Workflow run.
    WorkflowRun,
    /// Durable background task.
    BackgroundTask,
    /// Verification job.
    Verification,
    /// Generated or stored artifact.
    Artifact,
    /// Context-selection result.
    ContextResult,
    /// Pending approval request.
    ApprovalPending,
}
/// Lifecycle status of a kernel-issued handle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HandleStatus {
    /// Operation has been accepted.
    Accepted,
    /// Operation is currently running.
    Running,
    /// Operation completed successfully.
    Completed,
    /// Operation failed.
    Failed,
    /// Handle no longer refers to the current resource revision.
    Stale,
    /// Handle passed its validity deadline.
    Expired,
    /// Status is not known by the current implementation.
    Unknown,
}
/// One stable callable capability and its captured availability metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityEntry {
    /// Stable capability identifier.
    pub stable_id: String,
    /// Capability version exposed by the kernel.
    pub version: u64,
    /// Callable operation name resolved by the kernel.
    pub callable_name: String,
    /// Side-effect classification attached to the operation.
    pub side_effect_class: String,
    /// Risk classification used by policy.
    pub risk_class: String,
    /// Availability under this snapshot's grants and policy.
    pub availability: Availability,
    /// Digest of the capability descriptor.
    pub descriptor_hash: String,
}
/// Integrity-bound capability view for one run and grant snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    /// Snapshot schema version.
    pub schema_version: u32,
    /// Stable snapshot identifier.
    pub id: String,
    /// Kernel instance that created the snapshot.
    pub kernel_id: String,
    /// Run that owns the snapshot.
    pub run_id: String,
    /// Reference to the grant state used during capability discovery.
    pub grant_snapshot_ref: String,
    /// Capability entries visible to the run.
    pub capability_entries: Vec<CapabilityEntry>,
    /// Digest of the policy used to compute availability.
    pub policy_hash: String,
    /// Unix timestamp in milliseconds when the snapshot was created.
    pub created_at_ms: i64,
    /// Digest of the snapshot with this field cleared.
    pub content_hash: String,
}
/// Idempotent capability invocation bound to a specific snapshot revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Call {
    /// Stable invocation identifier.
    pub call_id: String,
    /// Kernel instance expected to execute the call.
    pub kernel_id: String,
    /// Snapshot identifier against which the call was authorized.
    pub snapshot_ref: String,
    /// Capability selected from the snapshot.
    pub capability_id: String,
    /// Capability version selected from the snapshot.
    pub capability_version: u64,
    /// Reference to bounded serialized call arguments.
    pub args_ref: String,
    /// Key used to prevent duplicate side effects on retries.
    pub idempotency_key: String,
    /// Digest binding the call metadata and arguments reference.
    pub content_hash: String,
}
/// Opaque reference to an operation or artifact owned by a run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Handle {
    /// Stable handle identifier.
    pub id: String,
    /// Kind of resource represented by the handle.
    pub kind: HandleKind,
    /// Run that owns this handle.
    pub owner_run_id: String,
    /// Opaque reference to the underlying resource.
    pub underlying_ref: String,
    /// Current operation status.
    pub status: HandleStatus,
    /// Unix timestamp in milliseconds when the handle was created.
    pub created_at_ms: i64,
    /// Optional absolute expiry timestamp in Unix milliseconds.
    pub expires_at_ms: Option<i64>,
    /// Digest binding the handle metadata.
    pub content_hash: String,
}
/// Invalid facade record or capability unavailable in a captured snapshot.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KernelError {
    /// Snapshot or call metadata violates its contract.
    #[error("invalid kernel facade record: {0}")]
    Invalid(String),
    /// Call references a capability missing or unavailable in the snapshot.
    #[error("capability is not available in snapshot")]
    Unavailable,
}
fn hash<T: Serialize>(v: &T) -> Result<String, KernelError> {
    serde_json::to_vec(v)
        .map(|b| hex::encode(Sha256::digest(b)))
        .map_err(|e| KernelError::Invalid(e.to_string()))
}
/// Validates snapshot bounds and its canonical content digest.
pub fn validate_snapshot(s: &Snapshot) -> Result<(), KernelError> {
    if s.schema_version != SCHEMA_VERSION
        || s.id.trim().is_empty()
        || s.kernel_id.trim().is_empty()
        || s.run_id.trim().is_empty()
        || s.grant_snapshot_ref.trim().is_empty()
        || s.policy_hash.trim().is_empty()
        || s.created_at_ms <= 0
        || s.capability_entries.len() > MAX_ITEMS
    {
        return Err(KernelError::Invalid("snapshot identity".into()));
    }
    let mut c = s.clone();
    c.content_hash.clear();
    if s.content_hash != hash(&c)? {
        return Err(KernelError::Invalid("snapshot hash".into()));
    }
    Ok(())
}
/// Authorizes a call only against its matching kernel and available capability version.
pub fn authorize_call(s: &Snapshot, c: &Call) -> Result<(), KernelError> {
    validate_snapshot(s)?;
    if c.snapshot_ref != s.id || c.kernel_id != s.kernel_id {
        return Err(KernelError::Invalid("snapshot mismatch".into()));
    }
    let e = s
        .capability_entries
        .iter()
        .find(|e| e.stable_id == c.capability_id && e.version == c.capability_version)
        .ok_or(KernelError::Unavailable)?;
    if e.availability != Availability::Available {
        return Err(KernelError::Unavailable);
    }
    Ok(())
}
