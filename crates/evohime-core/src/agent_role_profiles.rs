//! Core-owned, bounded and versioned agent role profiles.
//!
//! A profile describes intent and requested capabilities only.  It is never
//! an authority source: effective grants are calculated from the parent grant,
//! policy and registry intersections immediately before an effect.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 96;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_ITEMS: usize = 32;
pub const MAX_CONTRACT_FIELDS: usize = 16;
pub const MAX_CANONICAL_BYTES: usize = 64 * 1024;
pub const MAX_TIMEOUT_MS: u64 = 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Human,
    Ai,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractField {
    pub name: String,
    pub type_name: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetDefaults {
    pub timeout_ms: u64,
    pub max_steps: u32,
    pub max_output_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRoleProfile {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub objective: String,
    pub constraints: Vec<String>,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub strategy: String,
    pub input_contract: Vec<ContractField>,
    pub output_contract: Vec<ContractField>,
    pub budget_defaults: BudgetDefaults,
    pub execution_mode: ExecutionMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    pub profile_id: String,
    pub revision: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstance {
    pub run_id: String,
    pub snapshot: ProfileSnapshot,
    pub effective_grants: Vec<String>,
    pub state: RunState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Pinned,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleProfileError {
    Invalid(&'static str),
    UnsupportedVersion(u32),
    Duplicate,
    NotFound,
    Stale,
    IdempotencyConflict,
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

pub fn canonical_hash(profile: &AgentRoleProfile) -> Result<String, RoleProfileError> {
    validate_profile(profile)?;
    let bytes =
        serde_json::to_vec(profile).map_err(|_| RoleProfileError::Invalid("serialization"))?;
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(RoleProfileError::Invalid("profile_too_large"));
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

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
    let allowed: BTreeSet<&str> = parent
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        .intersection(&policy.iter().map(String::as_str).collect())
        .copied()
        .collect::<BTreeSet<_>>()
        .intersection(&registry.iter().map(String::as_str).collect())
        .copied()
        .collect();
    let result: Vec<String> = requested
        .iter()
        .filter(|grant| allowed.contains(grant.as_str()))
        .cloned()
        .collect();
    if result.len() != requested.len() {
        return Err(RoleProfileError::CapabilityDenied);
    }
    Ok(result)
}

#[derive(Debug, Default)]
pub struct AgentRoleProfilesRegistry {
    pub profiles: BTreeMap<String, AgentRoleProfile>,
    pub runs: BTreeMap<String, RuntimeInstance>,
    pub idempotency: BTreeMap<String, String>,
}

impl AgentRoleProfilesRegistry {
    pub fn list(&self) -> Vec<AgentRoleProfile> {
        self.profiles.values().cloned().collect()
    }
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
    pub fn start(
        &mut self,
        run_id: String,
        profile_id: &str,
        revision: u64,
        grants: Vec<String>,
        parent: &[String],
        policy: &[String],
        registry: &[String],
    ) -> Result<RuntimeInstance, RoleProfileError> {
        if !valid_id(&run_id) {
            return Err(RoleProfileError::Invalid("run_id"));
        }
        if self.runs.contains_key(&run_id) {
            return Err(RoleProfileError::Duplicate);
        }
        let profile = self
            .profiles
            .get(profile_id)
            .ok_or(RoleProfileError::NotFound)?;
        if profile.revision != revision {
            return Err(RoleProfileError::Stale);
        }
        let snapshot = ProfileSnapshot {
            profile_id: profile.id.clone(),
            revision,
            content_hash: canonical_hash(profile)?,
        };
        let effective_grants = effective_grants(parent, policy, registry, &grants)?;
        let instance = RuntimeInstance {
            run_id: run_id.clone(),
            snapshot,
            effective_grants,
            state: RunState::Pinned,
        };
        self.runs.insert(run_id, instance.clone());
        Ok(instance)
    }
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
