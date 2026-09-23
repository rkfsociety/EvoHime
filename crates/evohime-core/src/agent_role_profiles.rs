//! Core-owned, bounded and versioned agent role profiles.
//!
//! A profile describes intent and requested capabilities only.  It is never
//! an authority source: effective grants are calculated from the parent grant,
//! policy and registry intersections immediately before an effect.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Current serialized version for agent role profiles.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum identifier length for profiles, skills, tools, and grants.
pub const MAX_ID_BYTES: usize = 96;
/// Maximum byte length of objective, strategy, and constraint text.
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
/// Maximum number of constraints, skills, tools, or grants.
pub const MAX_ITEMS: usize = 32;
/// Maximum number of fields in an input or output contract.
pub const MAX_CONTRACT_FIELDS: usize = 16;
/// Maximum serialized size of a canonical profile.
pub const MAX_CANONICAL_BYTES: usize = 64 * 1024;
/// Maximum default execution timeout in milliseconds.
pub const MAX_TIMEOUT_MS: u64 = 3_600_000;

/// Execution actor category requested by an agent role profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// A human performs the assigned work.
    Human,
    /// An AI agent performs the assigned work.
    Ai,
}

/// One named field in a role's input or output contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractField {
    /// Stable field identifier.
    pub name: String,
    /// Human-readable or schema type name.
    pub type_name: String,
    /// Whether callers must supply this field.
    pub required: bool,
}

/// Default resource limits for a runtime instance of a role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetDefaults {
    /// Maximum execution duration in milliseconds.
    pub timeout_ms: u64,
    /// Maximum number of steps permitted.
    pub max_steps: u32,
    /// Maximum output size in bytes.
    pub max_output_bytes: u32,
}

/// Versioned role intent, requirements, contracts, and default budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRoleProfile {
    /// Profile schema version.
    pub schema_version: u32,
    /// Stable profile identifier.
    pub id: String,
    /// Monotonic profile revision.
    pub revision: u64,
    /// Objective assigned to the role.
    pub objective: String,
    /// Constraints the role must observe.
    pub constraints: Vec<String>,
    /// Skill identifiers requested by this role.
    pub skills: Vec<String>,
    /// Tool identifiers requested by this role.
    pub tools: Vec<String>,
    /// Ordered strategy description for performing the objective.
    pub strategy: String,
    /// Expected input fields for the role.
    pub input_contract: Vec<ContractField>,
    /// Expected output fields for the role.
    pub output_contract: Vec<ContractField>,
    /// Resource defaults applied when creating a runtime instance.
    pub budget_defaults: BudgetDefaults,
    /// Whether a human or AI is expected to execute the role.
    pub execution_mode: ExecutionMode,
}

/// Immutable profile revision pinned to a runtime instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    /// Profile identifier captured at runtime creation.
    pub profile_id: String,
    /// Profile revision captured at runtime creation.
    pub revision: u64,
    /// Digest of the exact profile revision.
    pub content_hash: String,
}

/// Running role instance with a pinned profile and effective grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstance {
    /// Stable runtime run identifier.
    pub run_id: String,
    /// Profile revision pinned when the instance started.
    pub snapshot: ProfileSnapshot,
    /// Grants surviving parent, policy, registry, and requested intersections.
    pub effective_grants: Vec<String>,
    /// Current role runtime state.
    pub state: RunState,
}

/// Lifecycle state for an agent role runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Runtime is pinned to its profile and ready to execute.
    Pinned,
    /// Runtime is executing the role objective.
    Running,
    /// Cancellation has been requested.
    Cancelling,
    /// Runtime completed successfully.
    Completed,
    /// Runtime was cancelled.
    Cancelled,
    /// Runtime failed.
    Failed,
    /// Runtime outcome is not known.
    Unknown,
}

/// Invalid profile, revision, idempotency key, or requested capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleProfileError {
    /// Profile data or operation input is invalid.
    Invalid(&'static str),
    /// Profile uses an unsupported schema version.
    UnsupportedVersion(u32),
    /// Identifier conflicts with an existing profile or runtime.
    Duplicate,
    /// Requested profile or runtime does not exist.
    NotFound,
    /// Expected profile revision is no longer current.
    Stale,
    /// Idempotency key was reused with different operation data.
    IdempotencyConflict,
    /// Requested grants are not present in all required grant sources.
    CapabilityDenied,
}

impl std::fmt::Display for RoleProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Invalid(v) => *v,
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::Duplicate => "duplicate",
            Self::NotFound => "not_found",
            Self::Stale => "stale",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::CapabilityDenied => "capability_denied",
        };
        f.write_str(value)
    }
}
impl std::error::Error for RoleProfileError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}
fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT_BYTES
}
fn valid_items(values: &[String]) -> bool {
    values.len() <= MAX_ITEMS && values.iter().all(|v| valid_id(v))
}

/// Validates profile identity, contracts, field bounds, and default budget.
pub fn validate_profile(profile: &AgentRoleProfile) -> Result<(), RoleProfileError> {
    if profile.schema_version != CONTRACT_VERSION {
        return Err(RoleProfileError::UnsupportedVersion(profile.schema_version));
    }
    if !valid_id(&profile.id)
        || profile.revision == 0
        || !valid_text(&profile.objective)
        || !valid_text(&profile.strategy)
    {
        return Err(RoleProfileError::Invalid("profile"));
    }
    if profile.constraints.len() > MAX_ITEMS
        || profile.constraints.iter().any(|v| !valid_text(v))
        || !valid_items(&profile.skills)
        || !valid_items(&profile.tools)
    {
        return Err(RoleProfileError::Invalid("bounds"));
    }
    for fields in [&profile.input_contract, &profile.output_contract] {
        if fields.len() > MAX_CONTRACT_FIELDS
            || fields
                .iter()
                .any(|f| !valid_id(&f.name) || !valid_id(&f.type_name))
        {
            return Err(RoleProfileError::Invalid("contract"));
        }
    }
    if profile.budget_defaults.timeout_ms == 0
        || profile.budget_defaults.timeout_ms > MAX_TIMEOUT_MS
        || profile.budget_defaults.max_steps == 0
        || profile.budget_defaults.max_output_bytes == 0
    {
        return Err(RoleProfileError::Invalid("budget"));
    }
    Ok(())
}

/// Validates a profile and returns its canonical SHA-256 digest.
pub fn canonical_hash(profile: &AgentRoleProfile) -> Result<String, RoleProfileError> {
    validate_profile(profile)?;
    let bytes =
        serde_json::to_vec(profile).map_err(|_| RoleProfileError::Invalid("serialization"))?;
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(RoleProfileError::Invalid("profile_too_large"));
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Intersects requested grants with parent, policy, and registry grants.
pub fn effective_grants(
    parent: &[String],
    policy: &[String],
    registry: &[String],
    requested: &[String],
) -> Result<Vec<String>, RoleProfileError> {
    if !valid_items(requested)
        || !valid_items(parent)
        || !valid_items(policy)
        || !valid_items(registry)
    {
        return Err(RoleProfileError::Invalid("grants"));
    }
    let result: Vec<String> = requested
        .iter()
        .filter(|grant| {
            parent.iter().any(|value| value == *grant)
                && policy.iter().any(|value| value == *grant)
                && registry.iter().any(|value| value == *grant)
        })
        .cloned()
        .collect();
    if result.len() != requested.len() {
        return Err(RoleProfileError::CapabilityDenied);
    }
    Ok(result)
}

/// In-memory profile and runtime registry with revision and idempotency checks.
#[derive(Debug, Default)]
pub struct AgentRoleProfilesRegistry {
    /// Role profiles indexed by their stable identifiers.
    pub profiles: BTreeMap<String, AgentRoleProfile>,
    /// Runtime instances indexed by run identifier.
    pub runs: BTreeMap<String, RuntimeInstance>,
    /// Operation fingerprints indexed by idempotency key.
    pub idempotency: BTreeMap<String, String>,
}

impl AgentRoleProfilesRegistry {
    /// Returns role profiles in stable identifier order.
    pub fn list(&self) -> Vec<AgentRoleProfile> {
        self.profiles.values().cloned().collect()
    }
    /// Validates and registers a profile using an idempotency key.
    pub fn create(
        &mut self,
        profile: AgentRoleProfile,
        idempotency_key: &str,
    ) -> Result<AgentRoleProfile, RoleProfileError> {
        validate_profile(&profile)?;
        let hash = canonical_hash(&profile)?;
        if self.profiles.contains_key(&profile.id) {
            return Err(RoleProfileError::Duplicate);
        }
        if let Some(previous) = self.idempotency.get(idempotency_key) {
            if previous != &hash {
                return Err(RoleProfileError::IdempotencyConflict);
            }
            return Ok(profile);
        }
        self.idempotency.insert(idempotency_key.into(), hash);
        self.profiles.insert(profile.id.clone(), profile.clone());
        Ok(profile)
    }
    /// Stores a newer profile revision after optimistic-concurrency validation.
    pub fn revise(
        &mut self,
        profile: AgentRoleProfile,
        expected_revision: u64,
        idempotency_key: &str,
    ) -> Result<AgentRoleProfile, RoleProfileError> {
        validate_profile(&profile)?;
        let current = self
            .profiles
            .get(&profile.id)
            .ok_or(RoleProfileError::NotFound)?;
        if current.revision != expected_revision || profile.revision <= expected_revision {
            return Err(RoleProfileError::Stale);
        }
        let hash = canonical_hash(&profile)?;
        if let Some(previous) = self.idempotency.get(idempotency_key) {
            if previous != &hash {
                return Err(RoleProfileError::IdempotencyConflict);
            }
            return Ok(profile);
        }
        self.idempotency.insert(idempotency_key.into(), hash);
        self.profiles.insert(profile.id.clone(), profile.clone());
        Ok(profile)
    }
    /// Pins the requested profile revision and computes effective runtime grants.
    pub fn start(
        &mut self,
        input: StartRuntimeInput<'_>,
    ) -> Result<RuntimeInstance, RoleProfileError> {
        if !valid_id(&input.run_id) {
            return Err(RoleProfileError::Invalid("run_id"));
        }
        if self.runs.contains_key(&input.run_id) {
            return Err(RoleProfileError::Duplicate);
        }
        let profile = self
            .profiles
            .get(input.profile_id)
            .ok_or(RoleProfileError::NotFound)?;
        if profile.revision != input.revision {
            return Err(RoleProfileError::Stale);
        }
        let snapshot = ProfileSnapshot {
            profile_id: profile.id.clone(),
            revision: input.revision,
            content_hash: canonical_hash(profile)?,
        };
        let effective_grants =
            effective_grants(input.parent, input.policy, input.registry, &input.grants)?;
        let instance = RuntimeInstance {
            run_id: input.run_id.clone(),
            snapshot,
            effective_grants,
            state: RunState::Pinned,
        };
        self.runs.insert(input.run_id, instance.clone());
        Ok(instance)
    }
    /// Requests cancellation for a nonterminal runtime instance.
    pub fn cancel(&mut self, run_id: &str) -> Result<RuntimeInstance, RoleProfileError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or(RoleProfileError::NotFound)?;
        if matches!(
            run.state,
            RunState::Completed | RunState::Cancelled | RunState::Failed | RunState::Unknown
        ) {
            return Ok(run.clone());
        }
        run.state = RunState::Cancelling;
        Ok(run.clone())
    }
}

/// Вход запуска runtime с явно разделёнными источниками grants.
pub struct StartRuntimeInput<'a> {
    /// New runtime identifier.
    pub run_id: String,
    /// Profile to pin for the runtime.
    pub profile_id: &'a str,
    /// Required current profile revision.
    pub revision: u64,
    /// Capabilities requested by the runtime.
    pub grants: Vec<String>,
    /// Capabilities granted by the parent runtime.
    pub parent: &'a [String],
    /// Capabilities allowed by active policy.
    pub policy: &'a [String],
    /// Capabilities registered for this profile/runtime class.
    pub registry: &'a [String],
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> AgentRoleProfile {
        AgentRoleProfile {
            schema_version: 1,
            id: "reviewer".into(),
            revision: 1,
            objective: "Review bounded changes".into(),
            constraints: vec!["summary_only".into()],
            skills: vec!["review".into()],
            tools: vec!["workspace.read".into()],
            strategy: "inspect_then_report".into(),
            input_contract: vec![ContractField {
                name: "task_ref".into(),
                type_name: "string".into(),
                required: true,
            }],
            output_contract: vec![ContractField {
                name: "status".into(),
                type_name: "string".into(),
                required: true,
            }],
            budget_defaults: BudgetDefaults {
                timeout_ms: 1000,
                max_steps: 2,
                max_output_bytes: 1024,
            },
            execution_mode: ExecutionMode::Ai,
        }
    }
    #[test]
    fn profile_hash_is_deterministic_and_versioned() {
        let p = profile();
        assert_eq!(canonical_hash(&p), canonical_hash(&p));
        assert!(validate_profile(&p).is_ok());
    }
    #[test]
    fn grants_are_intersection_and_never_expansion() {
        assert_eq!(
            effective_grants(
                &["read".into(), "write".into()],
                &["read".into()],
                &["read".into()],
                &["read".into()]
            )
            .unwrap(),
            vec!["read"]
        );
        assert_eq!(
            effective_grants(
                &["read".into()],
                &["read".into()],
                &["read".into()],
                &["write".into()]
            ),
            Err(RoleProfileError::CapabilityDenied)
        );
    }
    #[test]
    fn stale_revision_is_rejected() {
        let mut r = AgentRoleProfilesRegistry::default();
        r.create(profile(), "create-1").unwrap();
        let mut revised = profile();
        revised.revision = 2;
        assert!(r.revise(revised, 0, "rev-1").is_err());
    }
}
