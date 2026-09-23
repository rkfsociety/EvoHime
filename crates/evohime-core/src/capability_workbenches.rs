//! Core-owned lifecycle and capability contract for runtime workbench instances.
//!
//! A workbench is a bounded logical component.  OS handles, executable
//! identities and credential material are deliberately outside this contract;
//! a snapshot can only contain safe logical state and credential references.

use serde::{Deserialize, Serialize};

/// Serialized schema version for workbench descriptors and snapshots.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum byte length accepted for workbench identifiers.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum tools in one workbench descriptor.
pub const MAX_TOOLS: usize = 128;
/// Maximum shared resources in one descriptor or snapshot.
pub const MAX_RESOURCES: usize = 64;
/// Maximum credential or resource leases retained in a snapshot.
pub const MAX_LEASES: usize = 32;
/// Maximum admitted concurrent calls per instance.
pub const MAX_IN_FLIGHT: usize = 32;
/// Maximum serialized snapshot size in bytes.
pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
/// Minimum accepted resource lease duration.
pub const MIN_LEASE_TTL_MS: u64 = 1_000;
/// Maximum accepted resource lease duration.
pub const MAX_LEASE_TTL_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of one logical workbench instance.
pub enum Lifecycle {
    /// Instance is constructed but has not started.
    Created,
    /// Instance startup is in progress.
    Starting,
    /// Instance is available for capability-checked calls.
    Ready,
    /// Instance is shutting down.
    Stopping,
    /// Instance is stopped and accepts no calls.
    Stopped,
    /// Instance state is being reset.
    Resetting,
    /// Instance missed its lease heartbeat and requires recovery.
    Degraded,
    /// Instance failed and requires owner intervention.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifetime and owner scope for a workbench instance.
pub enum WorkbenchScope {
    /// Instance lifetime is limited to one run.
    RunScoped,
    /// Instance lifetime is limited to one goal.
    GoalScoped,
    /// Instance is owned by a project.
    ProjectScoped,
    /// Instance lifetime is limited to a user session.
    UserSessionScoped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Call scheduling semantics for a workbench.
pub enum Concurrency {
    /// Admit at most one call at a time.
    Exclusive,
    /// Allow concurrent requests but execute them in a serialized order.
    Serialized,
    /// Allow calls to execute concurrently up to the configured limit.
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of a resource lease.
pub enum ResourceLeaseState {
    /// Lease is valid and held by its owner.
    Active,
    /// Lease passed its expiration time.
    Expired,
    /// Lease was recovered after owner or runtime restart.
    Recovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Outcome of a cancellation request.
pub enum CancellationOutcome {
    /// An active operation accepted cancellation.
    Cancelled,
    /// The operation was already complete or stopped.
    AlreadyTerminal,
    /// No matching operation or cancellation outcome was found.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Result category for a workbench tool call.
pub enum CallOutcome {
    /// Call completed successfully.
    Success,
    /// Workbench or tool is unavailable.
    Unavailable,
    /// Call was rejected by capability or policy checks.
    Denied,
    /// No matching operation or cancellation outcome was found.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Owner, expiry, and heartbeat data for a leased resource.
pub struct WorkbenchResourceLease {
    /// Serialized schema version supported by the contract.
    pub schema_version: u32,
    /// Stable identifier for this lease.
    pub lease_id: String,
    /// Workbench instance owning this lease or call.
    pub instance_id: String,
    /// Owner authorized to manage this instance.
    pub owner_id: String,
    /// Current lifecycle or result state.
    pub state: ResourceLeaseState,
    /// Lease expiration time as Unix epoch milliseconds.
    pub expires_at_ms: u64,
    /// Most recent lease heartbeat time as Unix epoch milliseconds.
    pub heartbeat_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Structured result of a tool call handled by a workbench.
pub struct WorkbenchCallResult {
    /// Serialized schema version supported by the contract.
    pub schema_version: u32,
    /// Workbench instance owning this lease or call.
    pub instance_id: String,
    /// Stable tool identifier within the workbench.
    pub tool_id: String,
    /// Result category for the invocation.
    pub outcome: CallOutcome,
    /// Structured tool output, excluding secret material.
    pub value: serde_json::Value,
    /// Optional stable error category.
    pub error_code: Option<String>,
    /// Cancellation result associated with the call.
    pub cancellation: CancellationOutcome,
}

/// Classifies cancellation from active and terminal operation state.
pub fn cancellation_outcome(active: bool, terminal: bool) -> CancellationOutcome {
    if active {
        CancellationOutcome::Cancelled
    } else if terminal {
        CancellationOutcome::AlreadyTerminal
    } else {
        CancellationOutcome::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Tool identity and required capability exposed by a workbench.
pub struct ToolDescriptor {
    /// Stable descriptor, tool, or resource identifier.
    pub id: String,
    /// Capability required to invoke the tool.
    pub capability: String,
    /// Human-readable tool title.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Logical resource advertised by a workbench.
pub struct SharedResource {
    /// Stable descriptor, tool, or resource identifier.
    pub id: String,
    /// Logical resource category.
    pub class: String,
    /// Whether the resource is currently available.
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Versioned tools, resources, and concurrency policy for an instance.
pub struct WorkbenchDescriptor {
    /// Serialized schema version supported by the contract.
    pub schema_version: u32,
    /// Stable descriptor, tool, or resource identifier.
    pub id: String,
    /// Descriptor revision string used to bind snapshots.
    pub version: String,
    /// Workbench implementation category.
    pub kind: String,
    /// Owner and lifecycle scope for the instance.
    pub scope: WorkbenchScope,
    /// Policy for overlapping calls.
    pub concurrency: Concurrency,
    /// Maximum concurrent calls admitted.
    pub max_in_flight: u32,
    /// Duration after which a missed heartbeat triggers recovery.
    pub lease_ttl_ms: u64,
    /// Tools advertised by the workbench.
    pub tools: Vec<ToolDescriptor>,
    /// Logical resources advertised by the workbench.
    pub resources: Vec<SharedResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Portable logical state for recovery without process handles or secret values.
pub struct WorkbenchSnapshot {
    /// Serialized schema version supported by the contract.
    pub schema_version: u32,
    /// Workbench instance owning this lease or call.
    pub instance_id: String,
    /// Version of the descriptor captured in this snapshot.
    pub descriptor_version: String,
    /// Monotonic instance revision.
    pub revision: u64,
    /// Lifecycle state captured for recovery.
    pub lifecycle: Lifecycle,
    /// Safe JSON state retained across recovery.
    pub logical_state: serde_json::Value,
    /// Credential identifiers only; secret values are never serialized.
    pub credential_refs: Vec<String>,
    /// Logical resource identifiers held by the instance.
    pub resource_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Core-owned lifecycle and admission state for a workbench.
pub struct WorkbenchInstance {
    /// Serialized schema version supported by the contract.
    pub schema_version: u32,
    /// Workbench instance owning this lease or call.
    pub instance_id: String,
    /// Owner authorized to manage this instance.
    pub owner_id: String,
    /// Validated descriptor that defines this instance's tools and limits.
    pub descriptor: WorkbenchDescriptor,
    /// Lifecycle state captured for recovery.
    pub lifecycle: Lifecycle,
    /// Monotonic instance revision.
    pub revision: u64,
    /// Number of calls currently admitted.
    pub in_flight: u32,
    /// Time of the most recent heartbeat as Unix epoch milliseconds.
    pub last_heartbeat_ms: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
/// Validation, lifecycle, size, concurrency, or capability failure.
pub enum WorkbenchError {
    /// The serialized schema version is unsupported.
    #[error("unsupported workbench schema version {0}")]
    UnsupportedVersion(u32),
    /// An identifier is malformed or exceeds its limit.
    #[error("invalid or oversized workbench identifier")]
    InvalidId,
    /// A descriptor collection or concurrency limit was exceeded.
    #[error("workbench descriptor exceeds a bounded collection limit")]
    Bounds,
    /// Lease duration is outside its supported range.
    #[error("invalid lease TTL")]
    InvalidLease,
    /// Snapshot contains sensitive or process-local state.
    #[error("snapshot contains forbidden sensitive or process state")]
    ForbiddenSnapshotField,
    /// Serialized snapshot exceeds the supported size.
    #[error("snapshot exceeds the bounded size limit")]
    SnapshotTooLarge,
    /// The requested lifecycle transition is not allowed.
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    /// The caller supplied an outdated instance revision.
    #[error("stale workbench revision")]
    StaleRevision,
    /// Concurrency or lifecycle state prevents admission.
    #[error("workbench concurrency limit reached")]
    Busy,
    /// The caller lacks the capability required by the tool.
    #[error("capability is not granted")]
    CapabilityDenied,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

/// Checks descriptor identity, collection bounds, uniqueness, and lease TTL.
pub fn validate_descriptor(descriptor: &WorkbenchDescriptor) -> Result<(), WorkbenchError> {
    if descriptor.schema_version != SCHEMA_VERSION {
        return Err(WorkbenchError::UnsupportedVersion(
            descriptor.schema_version,
        ));
    }
    if !valid_id(&descriptor.id) || !valid_id(&descriptor.version) || !valid_id(&descriptor.kind) {
        return Err(WorkbenchError::InvalidId);
    }
    if descriptor.tools.len() > MAX_TOOLS
        || descriptor.resources.len() > MAX_RESOURCES
        || descriptor.max_in_flight == 0
        || descriptor.max_in_flight as usize > MAX_IN_FLIGHT
    {
        return Err(WorkbenchError::Bounds);
    }
    if !(MIN_LEASE_TTL_MS..=MAX_LEASE_TTL_MS).contains(&descriptor.lease_ttl_ms) {
        return Err(WorkbenchError::InvalidLease);
    }
    if descriptor
        .tools
        .iter()
        .any(|tool| !valid_id(&tool.id) || !valid_id(&tool.capability) || !valid_id(&tool.title))
        || descriptor
            .resources
            .iter()
            .any(|resource| !valid_id(&resource.id) || !valid_id(&resource.class))
    {
        return Err(WorkbenchError::InvalidId);
    }
    if descriptor.tools.iter().enumerate().any(|(index, tool)| {
        descriptor.tools[..index]
            .iter()
            .any(|prior| prior.id == tool.id)
    }) || descriptor
        .resources
        .iter()
        .enumerate()
        .any(|(index, resource)| {
            descriptor.resources[..index]
                .iter()
                .any(|prior| prior.id == resource.id)
        })
    {
        return Err(WorkbenchError::InvalidId);
    }
    Ok(())
}

fn forbidden_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "secret",
        "password",
        "token",
        "raw_prompt",
        "raw_output",
        "os_handle",
        "credential",
    ]
    .iter()
    .any(|part| key.contains(part))
}

fn contains_forbidden(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object
            .iter()
            .any(|(key, value)| forbidden_key(key) || contains_forbidden(value)),
        serde_json::Value::Array(items) => items.iter().any(contains_forbidden),
        _ => false,
    }
}

/// Rejects process-local or sensitive fields and enforces size limits.
pub fn validate_snapshot(snapshot: &WorkbenchSnapshot) -> Result<(), WorkbenchError> {
    if snapshot.schema_version != SCHEMA_VERSION
        || !valid_id(&snapshot.instance_id)
        || !valid_id(&snapshot.descriptor_version)
        || snapshot.credential_refs.len() > MAX_LEASES
        || snapshot.resource_ids.len() > MAX_RESOURCES
        || snapshot
            .credential_refs
            .iter()
            .any(|value| !valid_id(value))
        || snapshot.resource_ids.iter().any(|value| !valid_id(value))
        || contains_forbidden(&snapshot.logical_state)
    {
        return Err(WorkbenchError::ForbiddenSnapshotField);
    }
    let bytes = serde_json::to_vec(snapshot).map_err(|_| WorkbenchError::ForbiddenSnapshotField)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(WorkbenchError::SnapshotTooLarge);
    }
    Ok(())
}

impl WorkbenchInstance {
    /// Creates a validated workbench instance in the created state.
    pub fn new(
        instance_id: String,
        owner_id: String,
        descriptor: WorkbenchDescriptor,
        now_ms: u64,
    ) -> Result<Self, WorkbenchError> {
        validate_descriptor(&descriptor)?;
        if !valid_id(&instance_id) || !valid_id(&owner_id) {
            return Err(WorkbenchError::InvalidId);
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            instance_id,
            owner_id,
            descriptor,
            lifecycle: Lifecycle::Created,
            revision: 1,
            in_flight: 0,
            last_heartbeat_ms: now_ms,
        })
    }

    /// Applies a lifecycle transition using an expected revision.
    pub fn transition(
        &mut self,
        target: Lifecycle,
        expected_revision: u64,
    ) -> Result<(), WorkbenchError> {
        if self.revision != expected_revision || !valid_transition(self.lifecycle, target) {
            return if self.revision != expected_revision {
                Err(WorkbenchError::StaleRevision)
            } else {
                Err(WorkbenchError::InvalidTransition)
            };
        }
        self.lifecycle = target;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// Returns only tools whose capabilities appear in the supplied grants.
    pub fn visible_tools<'a>(&'a self, grants: &[String]) -> Vec<&'a ToolDescriptor> {
        self.descriptor
            .tools
            .iter()
            .filter(|tool| grants.iter().any(|grant| grant == &tool.capability))
            .collect()
    }

    /// Admits a call after capability, lifecycle, and concurrency checks.
    pub fn admit_call(
        &mut self,
        capability: &str,
        grants: &[String],
    ) -> Result<(), WorkbenchError> {
        if !grants.iter().any(|grant| grant == capability)
            || !self
                .descriptor
                .tools
                .iter()
                .any(|tool| tool.capability == capability)
        {
            return Err(WorkbenchError::CapabilityDenied);
        }
        if self.lifecycle != Lifecycle::Ready || self.in_flight >= self.descriptor.max_in_flight {
            return Err(WorkbenchError::Busy);
        }
        if self.descriptor.concurrency == Concurrency::Exclusive && self.in_flight > 0 {
            return Err(WorkbenchError::Busy);
        }
        self.in_flight += 1;
        Ok(())
    }

    /// Releases one admitted in-flight call slot.
    pub fn finish_call(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// Refreshes the instance lease timestamp.
    pub fn heartbeat(&mut self, now_ms: u64) {
        self.last_heartbeat_ms = now_ms;
    }

    /// Marks an expired instance degraded and clears abandoned call counts.
    pub fn recover_if_expired(&mut self, now_ms: u64) -> bool {
        if now_ms.saturating_sub(self.last_heartbeat_ms) > self.descriptor.lease_ttl_ms {
            self.in_flight = 0;
            self.lifecycle = Lifecycle::Degraded;
            self.revision = self.revision.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Creates and validates a portable snapshot of logical state.
    pub fn snapshot(
        &self,
        logical_state: serde_json::Value,
        credential_refs: Vec<String>,
    ) -> Result<WorkbenchSnapshot, WorkbenchError> {
        let snapshot = WorkbenchSnapshot {
            schema_version: SCHEMA_VERSION,
            instance_id: self.instance_id.clone(),
            descriptor_version: self.descriptor.version.clone(),
            revision: self.revision,
            lifecycle: self.lifecycle,
            logical_state,
            credential_refs,
            resource_ids: self
                .descriptor
                .resources
                .iter()
                .map(|resource| resource.id.clone())
                .collect(),
        };
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }
}

fn valid_transition(from: Lifecycle, to: Lifecycle) -> bool {
    matches!(
        (from, to),
        (Lifecycle::Created, Lifecycle::Starting)
            | (Lifecycle::Starting, Lifecycle::Ready)
            | (Lifecycle::Starting, Lifecycle::Failed)
            | (Lifecycle::Ready, Lifecycle::Stopping)
            | (Lifecycle::Ready, Lifecycle::Resetting)
            | (Lifecycle::Ready, Lifecycle::Degraded)
            | (Lifecycle::Stopping, Lifecycle::Stopped)
            | (Lifecycle::Resetting, Lifecycle::Ready)
            | (Lifecycle::Degraded, Lifecycle::Starting)
            | (Lifecycle::Degraded, Lifecycle::Failed)
            | (Lifecycle::Failed, Lifecycle::Starting)
            | (Lifecycle::Stopped, Lifecycle::Starting)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> WorkbenchDescriptor {
        WorkbenchDescriptor {
            schema_version: SCHEMA_VERSION,
            id: "repo".into(),
            version: "1".into(),
            kind: "repository".into(),
            scope: WorkbenchScope::ProjectScoped,
            concurrency: Concurrency::Serialized,
            max_in_flight: 2,
            lease_ttl_ms: 10_000,
            tools: vec![ToolDescriptor {
                id: "status".into(),
                capability: "repo.read".into(),
                title: "Status".into(),
            }],
            resources: vec![SharedResource {
                id: "workspace".into(),
                class: "filesystem".into(),
                available: true,
            }],
        }
    }

    #[test]
    fn lifecycle_revision_capability_and_lease_are_core_owned() {
        let mut instance =
            WorkbenchInstance::new("i".into(), "o".into(), descriptor(), 100).unwrap();
        assert_eq!(instance.visible_tools(&["repo.read".into()]).len(), 1);
        instance.transition(Lifecycle::Starting, 1).unwrap();
        instance.transition(Lifecycle::Ready, 2).unwrap();
        instance
            .admit_call("repo.read", &["repo.read".into()])
            .unwrap();
        assert!(instance
            .admit_call("repo.read", &["repo.read".into()])
            .is_ok());
        instance.finish_call();
        instance.finish_call();
        assert!(instance.recover_if_expired(10_101));
        assert_eq!(instance.lifecycle, Lifecycle::Degraded);
    }

    #[test]
    fn snapshots_reject_secrets_and_bound_size() {
        let instance = WorkbenchInstance::new("i".into(), "o".into(), descriptor(), 100).unwrap();
        assert_eq!(
            instance
                .snapshot(serde_json::json!({"api_token":"no"}), vec![])
                .unwrap_err(),
            WorkbenchError::ForbiddenSnapshotField
        );
        assert!(instance
            .snapshot(
                serde_json::json!({"note":"ok"}),
                vec!["credential-ref".into()]
            )
            .is_ok());
    }
}
