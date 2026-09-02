//! Core-owned lifecycle and capability contract for runtime workbench instances.
//!
//! A workbench is a bounded logical component.  OS handles, executable
//! identities and credential material are deliberately outside this contract;
//! a snapshot can only contain safe logical state and credential references.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_TOOLS: usize = 128;
pub const MAX_RESOURCES: usize = 64;
pub const MAX_LEASES: usize = 32;
pub const MAX_IN_FLIGHT: usize = 32;
pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
pub const MIN_LEASE_TTL_MS: u64 = 1_000;
pub const MAX_LEASE_TTL_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Created,
    Starting,
    Ready,
    Stopping,
    Stopped,
    Resetting,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkbenchScope {
    RunScoped,
    GoalScoped,
    ProjectScoped,
    UserSessionScoped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Concurrency {
    Exclusive,
    Serialized,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLeaseState {
    Active,
    Expired,
    Recovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationOutcome {
    Cancelled,
    AlreadyTerminal,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallOutcome {
    Success,
    Unavailable,
    Denied,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchResourceLease {
    pub schema_version: u32,
    pub lease_id: String,
    pub instance_id: String,
    pub owner_id: String,
    pub state: ResourceLeaseState,
    pub expires_at_ms: u64,
    pub heartbeat_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchCallResult {
    pub schema_version: u32,
    pub instance_id: String,
    pub tool_id: String,
    pub outcome: CallOutcome,
    pub value: serde_json::Value,
    pub error_code: Option<String>,
    pub cancellation: CancellationOutcome,
}

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
pub struct ToolDescriptor {
    pub id: String,
    pub capability: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedResource {
    pub id: String,
    pub class: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub kind: String,
    pub scope: WorkbenchScope,
    pub concurrency: Concurrency,
    pub max_in_flight: u32,
    pub lease_ttl_ms: u64,
    pub tools: Vec<ToolDescriptor>,
    pub resources: Vec<SharedResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchSnapshot {
    pub schema_version: u32,
    pub instance_id: String,
    pub descriptor_version: String,
    pub revision: u64,
    pub lifecycle: Lifecycle,
    pub logical_state: serde_json::Value,
    pub credential_refs: Vec<String>,
    pub resource_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchInstance {
    pub schema_version: u32,
    pub instance_id: String,
    pub owner_id: String,
    pub descriptor: WorkbenchDescriptor,
    pub lifecycle: Lifecycle,
    pub revision: u64,
    pub in_flight: u32,
    pub last_heartbeat_ms: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkbenchError {
    #[error("unsupported workbench schema version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid or oversized workbench identifier")]
    InvalidId,
    #[error("workbench descriptor exceeds a bounded collection limit")]
    Bounds,
    #[error("invalid lease TTL")]
    InvalidLease,
    #[error("snapshot contains forbidden sensitive or process state")]
    ForbiddenSnapshotField,
    #[error("snapshot exceeds the bounded size limit")]
    SnapshotTooLarge,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("stale workbench revision")]
    StaleRevision,
    #[error("workbench concurrency limit reached")]
    Busy,
    #[error("capability is not granted")]
    CapabilityDenied,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

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

    pub fn visible_tools<'a>(&'a self, grants: &[String]) -> Vec<&'a ToolDescriptor> {
        self.descriptor
            .tools
            .iter()
            .filter(|tool| grants.iter().any(|grant| grant == &tool.capability))
            .collect()
    }

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

    pub fn finish_call(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    pub fn heartbeat(&mut self, now_ms: u64) {
        self.last_heartbeat_ms = now_ms;
    }

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
