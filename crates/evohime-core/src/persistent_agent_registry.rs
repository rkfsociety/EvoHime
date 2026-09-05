//! Core-owned persistent organization registry for long-lived agents.
//!
//! This contract contains durable identity and accountability metadata only.
//! A role profile describes how an execution may be configured; a runtime run
//! remains the only owner of live execution state and effective grants.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "persistent-agent-organization-registry/v1";
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_NAME_BYTES: usize = 128;
pub const MAX_TEXT_BYTES: usize = 1024;
pub const MAX_ITEMS: usize = 32;
pub const MAX_HISTORY: usize = 256;
pub const MAX_PROJECTION_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationScopeKind {
    Application,
    Workspace,
    WorkspaceSet,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationScope {
    pub kind: OrganizationScopeKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Draft,
    Active,
    Paused,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponsibilityKind {
    RepositoryArea,
    Subsystem,
    ArtifactType,
    GoalClass,
    WorkflowClass,
    ReviewDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsibilityScope {
    pub kind: ResponsibilityKind,
    pub selector: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalResponsibility {
    Owner,
    Contributor,
    Reviewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentSourceKind {
    Task,
    Run,
    TeamSession,
    Handoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Pending,
    Active,
    Completed,
    Cancelled,
    UnknownAfterRestart,
    Broken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAvailability {
    Ready,
    Busy,
    Waiting,
    Blocked,
    RuntimeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleProfileRef {
    pub id: String,
    pub revision: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRef {
    pub goal_id: String,
    pub goal_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistentAgent {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub role_profile_ref: RoleProfileRef,
    pub organization_scope: OrganizationScope,
    pub reporting_to_agent_id: Option<String>,
    pub responsibility_scope: Vec<ResponsibilityScope>,
    pub default_goal_bindings: Vec<GoalRef>,
    pub execution_policy_refs: Vec<String>,
    pub status: LifecycleStatus,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGoalBinding {
    pub schema_version: u32,
    pub agent_id: String,
    pub goal_id: String,
    pub goal_revision: u64,
    pub responsibility: GoalResponsibility,
    pub scope: Option<OrganizationScope>,
    pub created_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionAgentSnapshot {
    pub schema_version: u32,
    pub agent_id: String,
    pub agent_revision: u64,
    pub role_profile_ref: RoleProfileRef,
    pub reporting_chain: Vec<String>,
    pub goal_ref: Option<GoalRef>,
    pub responsibility_scope: Vec<ResponsibilityScope>,
    pub accountability_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAssignment {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub agent_id: String,
    pub source_kind: AssignmentSourceKind,
    pub source_ref: String,
    pub goal_ref: Option<GoalRef>,
    pub team_session_ref: Option<String>,
    pub task_ref: Option<String>,
    pub status: AssignmentStatus,
    pub created_by: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub execution_snapshot: Option<ExecutionAgentSnapshot>,
    pub provenance_hash: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHistoryEntry {
    pub revision: u64,
    pub event_type: String,
    pub actor: String,
    pub summary_hash: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProjection {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub organization_scope: OrganizationScope,
    pub reporting_to_agent_id: Option<String>,
    pub status: LifecycleStatus,
    pub role_profile_ref: RoleProfileRef,
    pub responsibility_count: usize,
    pub default_goal_count: usize,
    pub assignment_count: usize,
    pub availability: ExecutionAvailability,
    pub binding_warnings: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryProjection {
    pub schema_version: u32,
    pub contract_id: String,
    pub status: String,
    pub operation: String,
    pub agent_id: String,
    pub revision: u64,
    pub projection: Option<AgentProjection>,
    pub agents: Vec<AgentProjection>,
    pub history: Vec<AgentHistoryEntry>,
    pub assignments: Vec<AgentAssignment>,
    pub cost_status: String,
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Invalid(&'static str),
    UnsupportedVersion(u32),
    Duplicate,
    NotFound,
    Stale,
    IdempotencyConflict,
    InvalidTransition,
    Cycle,
    ScopeMismatch,
    Retired,
    BrokenBinding(&'static str),
    ActorDenied,
    PayloadTooLarge,
    Corrupt,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Invalid(value) => *value,
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::Duplicate => "duplicate",
            Self::NotFound => "not_found",
            Self::Stale => "stale_revision",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::InvalidTransition => "invalid_status_transition",
            Self::Cycle => "reporting_cycle",
            Self::ScopeMismatch => "scope_mismatch",
            Self::Retired => "agent_retired",
            Self::BrokenBinding(value) => value,
            Self::ActorDenied => "actor_denied",
            Self::PayloadTooLarge => "payload_too_large",
            Self::Corrupt => "corrupt_registry_record",
        };
        formatter.write_str(value)
    }
}

impl std::error::Error for RegistryError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'-' | b'_' | b'/')
        })
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.contains('\0')
}

pub fn validate_actor(actor: &str) -> Result<(), RegistryError> {
    matches!(actor, "user" | "core")
        .then_some(())
        .ok_or(RegistryError::ActorDenied)
}

pub fn scope_token(scope: &OrganizationScope) -> String {
    format!("{:?}:{}", scope.kind, scope.id).to_ascii_lowercase()
}

pub fn validate_scope(scope: &OrganizationScope) -> Result<(), RegistryError> {
    if !valid_id(&scope.id) {
        return Err(RegistryError::Invalid("scope"));
    }
    Ok(())
}

fn validate_goal_ref(reference: &GoalRef) -> Result<(), RegistryError> {
    if !valid_id(&reference.goal_id) || reference.goal_revision == 0 {
        return Err(RegistryError::Invalid("goal_ref"));
    }
    Ok(())
}

fn validate_profile_ref(reference: &RoleProfileRef) -> Result<(), RegistryError> {
    if !valid_id(&reference.id)
        || reference.revision == 0
        || reference.content_hash.len() != 64
        || !reference
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(RegistryError::Invalid("role_profile_ref"));
    }
    Ok(())
}

pub fn validate_agent(agent: &PersistentAgent) -> Result<(), RegistryError> {
    if agent.schema_version != CONTRACT_VERSION {
        return Err(RegistryError::UnsupportedVersion(agent.schema_version));
    }
    if !valid_id(&agent.id)
        || agent.revision == 0
        || !valid_text(&agent.display_name, MAX_NAME_BYTES)
        || agent.created_at_ms < 0
        || agent.updated_at_ms < agent.created_at_ms
        || agent.responsibility_scope.len() > MAX_ITEMS
        || agent.default_goal_bindings.len() > MAX_ITEMS
        || agent.execution_policy_refs.len() > MAX_ITEMS
        || agent
            .execution_policy_refs
            .iter()
            .any(|value| !valid_id(value))
    {
        return Err(RegistryError::Invalid("agent"));
    }
    validate_profile_ref(&agent.role_profile_ref)?;
    validate_scope(&agent.organization_scope)?;
    if let Some(parent) = &agent.reporting_to_agent_id {
        if !valid_id(parent) || parent == &agent.id {
            return Err(RegistryError::Invalid("reporting_parent"));
        }
    }
    for responsibility in &agent.responsibility_scope {
        if !valid_text(&responsibility.selector, MAX_TEXT_BYTES) {
            return Err(RegistryError::Invalid("responsibility_scope"));
        }
    }
    for goal in &agent.default_goal_bindings {
        validate_goal_ref(goal)?;
    }
    if !agent.content_hash.is_empty()
        && (agent.content_hash.len() != 64
            || !agent
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(RegistryError::Invalid("content_hash"));
    }
    Ok(())
}

pub fn canonical_hash(agent: &PersistentAgent) -> Result<String, RegistryError> {
    let mut copy = agent.clone();
    copy.content_hash.clear();
    copy.execution_policy_refs.sort();
    copy.default_goal_bindings.sort_by(|left, right| {
        left.goal_id
            .cmp(&right.goal_id)
            .then(left.goal_revision.cmp(&right.goal_revision))
    });
    copy.responsibility_scope.sort_by(|left, right| {
        format!("{:?}", left.kind)
            .cmp(&format!("{:?}", right.kind))
            .then(left.selector.cmp(&right.selector))
    });
    let bytes = serde_json::to_vec(&copy).map_err(|_| RegistryError::Invalid("serialization"))?;
    if bytes.len() > MAX_PROJECTION_BYTES {
        return Err(RegistryError::PayloadTooLarge);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn normalize_and_validate_agent(agent: &PersistentAgent) -> Result<(), RegistryError> {
    validate_agent(agent)?;
    let hash = canonical_hash(agent)?;
    if agent.content_hash != hash {
        return Err(RegistryError::Invalid("content_hash_mismatch"));
    }
    Ok(())
}

pub fn transition_status(
    current: LifecycleStatus,
    next: LifecycleStatus,
) -> Result<(), RegistryError> {
    let allowed = matches!(
        (current, next),
        (
            LifecycleStatus::Draft,
            LifecycleStatus::Active | LifecycleStatus::Retired
        ) | (
            LifecycleStatus::Active,
            LifecycleStatus::Paused | LifecycleStatus::Suspended | LifecycleStatus::Retired
        ) | (
            LifecycleStatus::Paused,
            LifecycleStatus::Active | LifecycleStatus::Suspended | LifecycleStatus::Retired
        ) | (
            LifecycleStatus::Suspended,
            LifecycleStatus::Active | LifecycleStatus::Retired
        )
    );
    allowed.then_some(()).ok_or_else(|| {
        if current == LifecycleStatus::Retired {
            RegistryError::Retired
        } else {
            RegistryError::InvalidTransition
        }
    })
}

pub fn reporting_graph_valid(
    agents: &BTreeMap<String, PersistentAgent>,
    subject_id: &str,
    parent_id: Option<&str>,
) -> Result<(), RegistryError> {
    let subject = agents.get(subject_id).ok_or(RegistryError::NotFound)?;
    if let Some(parent_id) = parent_id {
        let parent = agents.get(parent_id).ok_or(RegistryError::NotFound)?;
        if parent.status == LifecycleStatus::Retired {
            return Err(RegistryError::Retired);
        }
        if parent.organization_scope != subject.organization_scope
            && !matches!(
                parent.organization_scope.kind,
                OrganizationScopeKind::Application
            )
        {
            return Err(RegistryError::ScopeMismatch);
        }
    }
    let mut seen = BTreeSet::new();
    let mut current = Some(subject_id.to_owned());
    for _ in 0..=MAX_ITEMS {
        let Some(id) = current else {
            return Ok(());
        };
        if !seen.insert(id.clone()) {
            return Err(RegistryError::Cycle);
        }
        if id == subject_id {
            current = parent_id.map(str::to_owned);
        } else {
            current = agents
                .get(&id)
                .and_then(|agent| agent.reporting_to_agent_id.clone());
        }
    }
    Err(RegistryError::Cycle)
}

pub fn reporting_chain(
    agents: &BTreeMap<String, PersistentAgent>,
    agent_id: &str,
) -> Result<Vec<String>, RegistryError> {
    let mut result = Vec::new();
    let mut current = agents
        .get(agent_id)
        .ok_or(RegistryError::NotFound)?
        .reporting_to_agent_id
        .clone();
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id.clone()) || result.len() >= MAX_ITEMS {
            return Err(RegistryError::Cycle);
        }
        result.push(id.clone());
        current = agents
            .get(&id)
            .and_then(|agent| agent.reporting_to_agent_id.clone());
    }
    Ok(result)
}

pub fn build_execution_snapshot(
    agent: &PersistentAgent,
    reporting_chain: Vec<String>,
    goal_ref: Option<GoalRef>,
) -> Result<ExecutionAgentSnapshot, RegistryError> {
    if agent.status != LifecycleStatus::Active {
        return Err(RegistryError::BrokenBinding("agent_not_active"));
    }
    if let Some(goal) = &goal_ref {
        validate_goal_ref(goal)?;
    }
    let mut snapshot = ExecutionAgentSnapshot {
        schema_version: CONTRACT_VERSION,
        agent_id: agent.id.clone(),
        agent_revision: agent.revision,
        role_profile_ref: agent.role_profile_ref.clone(),
        reporting_chain,
        goal_ref,
        responsibility_scope: agent.responsibility_scope.clone(),
        accountability_hash: String::new(),
    };
    let bytes =
        serde_json::to_vec(&snapshot).map_err(|_| RegistryError::Invalid("serialization"))?;
    snapshot.accountability_hash = hex::encode(Sha256::digest(bytes));
    Ok(snapshot)
}

pub fn project_agent(
    agent: &PersistentAgent,
    assignment_count: usize,
    availability: ExecutionAvailability,
    binding_warnings: Vec<String>,
) -> AgentProjection {
    AgentProjection {
        schema_version: CONTRACT_VERSION,
        id: agent.id.clone(),
        revision: agent.revision,
        display_name: agent.display_name.clone(),
        organization_scope: agent.organization_scope.clone(),
        reporting_to_agent_id: agent.reporting_to_agent_id.clone(),
        status: agent.status,
        role_profile_ref: agent.role_profile_ref.clone(),
        responsibility_count: agent.responsibility_scope.len(),
        default_goal_count: agent.default_goal_bindings.len(),
        assignment_count,
        availability,
        binding_warnings,
        content_hash: agent.content_hash.clone(),
    }
}

pub fn assignment_availability(assignments: &[AgentAssignment]) -> ExecutionAvailability {
    if assignments
        .iter()
        .any(|item| item.status == AssignmentStatus::Broken)
    {
        return ExecutionAvailability::Blocked;
    }
    if assignments
        .iter()
        .any(|item| item.status == AssignmentStatus::UnknownAfterRestart)
    {
        return ExecutionAvailability::RuntimeUnavailable;
    }
    if assignments
        .iter()
        .any(|item| item.status == AssignmentStatus::Active)
    {
        return ExecutionAvailability::Busy;
    }
    if assignments
        .iter()
        .any(|item| item.status == AssignmentStatus::Pending)
    {
        return ExecutionAvailability::Waiting;
    }
    ExecutionAvailability::Ready
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, parent: Option<&str>) -> PersistentAgent {
        let mut value = PersistentAgent {
            schema_version: CONTRACT_VERSION,
            id: id.into(),
            revision: 1,
            display_name: id.into(),
            role_profile_ref: RoleProfileRef {
                id: "profile".into(),
                revision: 1,
                content_hash: "0".repeat(64),
            },
            organization_scope: OrganizationScope {
                kind: OrganizationScopeKind::Workspace,
                id: "w".into(),
            },
            reporting_to_agent_id: parent.map(str::to_owned),
            responsibility_scope: Vec::new(),
            default_goal_bindings: Vec::new(),
            execution_policy_refs: Vec::new(),
            status: LifecycleStatus::Active,
            created_at_ms: 1,
            updated_at_ms: 1,
            content_hash: String::new(),
        };
        value.content_hash = canonical_hash(&value).unwrap();
        value
    }

    #[test]
    fn status_lifecycle_is_terminal_and_resume_is_explicit() {
        assert!(transition_status(LifecycleStatus::Draft, LifecycleStatus::Active).is_ok());
        assert!(transition_status(LifecycleStatus::Active, LifecycleStatus::Paused).is_ok());
        assert!(transition_status(LifecycleStatus::Paused, LifecycleStatus::Active).is_ok());
        assert_eq!(
            transition_status(LifecycleStatus::Retired, LifecycleStatus::Active),
            Err(RegistryError::Retired)
        );
    }

    #[test]
    fn reporting_graph_rejects_cycles_and_cross_scope() {
        let mut agents = BTreeMap::new();
        agents.insert("a".into(), agent("a", Some("b")));
        agents.insert("b".into(), agent("b", None));
        assert!(reporting_graph_valid(&agents, "a", Some("b")).is_ok());
        assert_eq!(
            reporting_graph_valid(&agents, "b", Some("a")),
            Err(RegistryError::Cycle)
        );
        agents.get_mut("b").unwrap().organization_scope.id = "other".into();
        assert_eq!(
            reporting_graph_valid(&agents, "a", Some("b")),
            Err(RegistryError::ScopeMismatch)
        );
    }

    #[test]
    fn source_status_derivation_never_leaves_terminal_runs_busy() {
        assert_eq!(
            map_source_status(AssignmentSourceKind::Run, "completed"),
            AssignmentStatus::Completed
        );
        assert_eq!(
            map_source_status(AssignmentSourceKind::Run, "failed"),
            AssignmentStatus::Broken
        );
        assert_eq!(
            map_source_status(AssignmentSourceKind::Run, "running"),
            AssignmentStatus::Active
        );
        assert_eq!(
            map_source_status(AssignmentSourceKind::Handoff, "unknown"),
            AssignmentStatus::UnknownAfterRestart
        );
    }

    #[test]
    fn hash_is_stable_after_metadata_reordering() {
        let mut left = agent("a", None);
        left.execution_policy_refs = vec!["z".into(), "a".into()];
        let mut right = left.clone();
        right.execution_policy_refs.reverse();
        assert_eq!(canonical_hash(&left), canonical_hash(&right));
    }

    #[test]
    fn unauthorized_actor_and_missing_binding_are_typed() {
        assert_eq!(validate_actor("model"), Err(RegistryError::ActorDenied));
        let value = agent("a", None);
        assert_eq!(
            build_execution_snapshot(
                &PersistentAgent {
                    status: LifecycleStatus::Paused,
                    ..value
                },
                Vec::new(),
                None
            ),
            Err(RegistryError::BrokenBinding("agent_not_active"))
        );
    }

    #[tokio::test]
    async fn durable_command_roundtrip_is_idempotent_and_rejects_unresolved_snapshot() {
        let path = std::env::temp_dir().join(format!(
            "evohime-persistent-agent-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = crate::EventJournal::open(&path).unwrap();
        let mut value = agent("persistent-a", None);
        value.status = LifecycleStatus::Draft;
        value.content_hash.clear();
        let payload = serde_json::to_vec(&value).unwrap();
        let command = RegistryCommand {
            operation: "create".into(),
            agent_id: value.id.clone(),
            owner_scope: scope_token(&value.organization_scope),
            actor: "user".into(),
            payload: payload.clone(),
            expected_revision: 0,
            idempotency_key: "create-persistent-a".into(),
        };
        let first = journal
            .persistent_agent_registry_command(command.clone())
            .await
            .unwrap();
        let second = journal
            .persistent_agent_registry_command(command)
            .await
            .unwrap();
        assert_eq!(first, second);
        let error = journal
            .persistent_agent_registry_command(RegistryCommand {
                operation: "resolve".into(),
                agent_id: "persistent-a".into(),
                owner_scope: "workspace:w".into(),
                actor: "user".into(),
                payload: Vec::new(),
                expected_revision: 1,
                idempotency_key: "resolve-persistent-a".into(),
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("missing_role_profile_revision"));
        drop(journal);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }
}

/// Command data passed from the authenticated IPC bridge.  The bridge may
/// only supply `user`; `core` is reserved for startup/recovery code.
#[derive(Debug, Clone)]
pub struct RegistryCommand {
    pub operation: String,
    pub agent_id: String,
    pub owner_scope: String,
    pub actor: String,
    pub payload: Vec<u8>,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

fn map_storage_error(error: rusqlite::Error) -> crate::StorageError {
    crate::StorageError::InvalidInput(format!("persistent agent registry storage: {error}"))
}

fn deserialize_agent(bytes: &[u8]) -> Result<PersistentAgent, RegistryError> {
    let agent: PersistentAgent =
        serde_json::from_slice(bytes).map_err(|_| RegistryError::Corrupt)?;
    normalize_and_validate_agent(&agent).map(|_| agent)
}

fn prepare_agent(mut agent: PersistentAgent) -> Result<PersistentAgent, RegistryError> {
    if agent.content_hash.is_empty() {
        agent.content_hash = canonical_hash(&agent)?;
    }
    normalize_and_validate_agent(&agent)?;
    Ok(agent)
}

fn role_profile_binding(
    connection: &rusqlite::Connection,
    reference: &RoleProfileRef,
) -> Result<Option<&'static str>, crate::StorageError> {
    let Some(bytes) = evohime_local_storage::agent_role_profiles_store::load_json(
        connection,
        &reference.id,
        reference.revision,
    )
    .map_err(map_storage_error)?
    else {
        return Ok(Some("missing_role_profile_revision"));
    };
    let profile: crate::agent_role_profiles::AgentRoleProfile = serde_json::from_slice(&bytes)
        .map_err(|_| crate::StorageError::InvalidInput("corrupt_role_profile".into()))?;
    let actual = crate::agent_role_profiles::canonical_hash(&profile)
        .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
    if actual != reference.content_hash {
        return Ok(Some("role_profile_hash_mismatch"));
    }
    Ok(None)
}

fn goal_exists(
    connection: &rusqlite::Connection,
    reference: &GoalRef,
) -> Result<bool, crate::StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_revisions WHERE goal_id=?1 AND version=?2)",
            rusqlite::params![reference.goal_id, reference.goal_revision as i64],
            |row| row.get(0),
        )
        .map_err(map_storage_error)
}

fn load_agents_map(
    connection: &rusqlite::Connection,
) -> Result<std::collections::BTreeMap<String, PersistentAgent>, crate::StorageError> {
    let mut result = std::collections::BTreeMap::new();
    for bytes in
        evohime_local_storage::persistent_agent_registry_store::load_agents(connection, 256)
            .map_err(map_storage_error)?
    {
        let agent = deserialize_agent(&bytes)
            .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
        result.insert(agent.id.clone(), agent);
    }
    Ok(result)
}

fn load_assignments(
    connection: &rusqlite::Connection,
    agent_id: &str,
) -> Result<Vec<AgentAssignment>, crate::StorageError> {
    evohime_local_storage::persistent_agent_registry_store::load_assignments_for_agent(
        connection,
        agent_id,
        MAX_HISTORY,
    )
    .map_err(map_storage_error)?
    .into_iter()
    .map(|bytes| serde_json::from_slice(&bytes).map_err(crate::StorageError::from))
    .collect()
}

fn map_source_status(kind: AssignmentSourceKind, status: &str) -> AssignmentStatus {
    match kind {
        AssignmentSourceKind::Task | AssignmentSourceKind::Run => match status {
            "completed" | "done" => AssignmentStatus::Completed,
            "running" | "in_progress" => AssignmentStatus::Active,
            "pending" | "queued" | "ready" | "backlog" | "waiting" => AssignmentStatus::Pending,
            "failed" | "cancelled" | "timed_out" => AssignmentStatus::Broken,
            _ => AssignmentStatus::UnknownAfterRestart,
        },
        AssignmentSourceKind::TeamSession => match status {
            "completed" => AssignmentStatus::Completed,
            "running" => AssignmentStatus::Active,
            "pinned" | "paused" => AssignmentStatus::Pending,
            "cancelled" | "blocked" => AssignmentStatus::Broken,
            _ => AssignmentStatus::UnknownAfterRestart,
        },
        AssignmentSourceKind::Handoff => match status {
            "completed" => AssignmentStatus::Completed,
            "active" => AssignmentStatus::Active,
            "proposed" | "accepted" => AssignmentStatus::Pending,
            "rejected" | "expired" | "failed" | "returned" => AssignmentStatus::Broken,
            _ => AssignmentStatus::UnknownAfterRestart,
        },
    }
}

fn source_assignment_status(
    connection: &rusqlite::Connection,
    kind: AssignmentSourceKind,
    source_ref: &str,
) -> Result<Option<AssignmentStatus>, crate::StorageError> {
    let (table, key_column, status_column) = match kind {
        AssignmentSourceKind::Task => ("work_items", "id", "status"),
        AssignmentSourceKind::Run => ("runs", "id", "status"),
        AssignmentSourceKind::TeamSession => ("team_sop_sessions", "id", "status"),
        AssignmentSourceKind::Handoff => ("typed_agent_handoffs", "handoff_id", "state"),
    };
    let sql = format!("SELECT {status_column} FROM {table} WHERE {key_column}=?1");
    connection
        .query_row(&sql, [source_ref], |row| row.get::<_, String>(0))
        .optional()
        .map(|value| value.map(|status| map_source_status(kind, &status)))
        .map_err(map_storage_error)
}

fn effective_assignments(
    connection: &rusqlite::Connection,
    agent_id: &str,
) -> Result<Vec<AgentAssignment>, crate::StorageError> {
    let mut assignments = load_assignments(connection, agent_id)?;
    for assignment in &mut assignments {
        if matches!(
            assignment.status,
            AssignmentStatus::Active | AssignmentStatus::Pending
        ) {
            assignment.status = source_assignment_status(
                connection,
                assignment.source_kind,
                &assignment.source_ref,
            )?
            .unwrap_or(AssignmentStatus::UnknownAfterRestart);
        }
    }
    Ok(assignments)
}

fn projection(
    connection: &rusqlite::Connection,
    agent: &PersistentAgent,
) -> Result<AgentProjection, crate::StorageError> {
    let assignments = effective_assignments(connection, &agent.id)?;
    let mut warnings = Vec::new();
    if let Some(warning) = role_profile_binding(connection, &agent.role_profile_ref)? {
        warnings.push(warning.to_owned());
    }
    for goal in &agent.default_goal_bindings {
        if !goal_exists(connection, goal)? {
            warnings.push(format!(
                "missing_goal_revision:{}:{}",
                goal.goal_id, goal.goal_revision
            ));
        }
    }
    Ok(project_agent(
        agent,
        assignments.len(),
        assignment_availability(&assignments),
        warnings,
    ))
}

fn source_exists(
    connection: &rusqlite::Connection,
    kind: AssignmentSourceKind,
    source_ref: &str,
) -> Result<bool, crate::StorageError> {
    Ok(source_assignment_status(connection, kind, source_ref)?.is_some())
}

fn authorize_scope(agent: &PersistentAgent, owner_scope: &str) -> Result<(), crate::StorageError> {
    let application_scope = "application:application".to_string();
    if owner_scope != scope_token(&agent.organization_scope) && owner_scope != application_scope {
        return Err(crate::StorageError::InvalidInput(
            RegistryError::ScopeMismatch.to_string(),
        ));
    }
    Ok(())
}

fn assignment_hash(assignment: &AgentAssignment) -> Result<String, RegistryError> {
    let mut copy = assignment.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| RegistryError::Invalid("serialization"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn prepare_assignment(mut assignment: AgentAssignment) -> Result<AgentAssignment, RegistryError> {
    if assignment.schema_version != CONTRACT_VERSION
        || !valid_id(&assignment.id)
        || assignment.revision == 0
        || !valid_id(&assignment.agent_id)
        || !valid_id(&assignment.source_ref)
        || assignment.created_by != "user"
        || assignment.created_at_ms < 0
        || assignment.updated_at_ms < assignment.created_at_ms
    {
        return Err(RegistryError::Invalid("assignment"));
    }
    if assignment.content_hash.is_empty() {
        assignment.content_hash = assignment_hash(&assignment)?;
    }
    if assignment.content_hash != assignment_hash(&assignment)? {
        return Err(RegistryError::Invalid("assignment_content_hash_mismatch"));
    }
    Ok(assignment)
}

fn command_hash(command: &RegistryCommand) -> Result<String, crate::StorageError> {
    let mut hasher = Sha256::new();
    let expected_revision = command.expected_revision.to_string();
    for value in [
        command.operation.as_bytes(),
        command.agent_id.as_bytes(),
        command.owner_scope.as_bytes(),
        command.actor.as_bytes(),
        expected_revision.as_bytes(),
        command.payload.as_slice(),
    ] {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    Ok(hex::encode(hasher.finalize()))
}

struct ResponseInput<'a> {
    operation: &'a str,
    agent_id: &'a str,
    revision: u64,
    status: &'a str,
    projection: Option<AgentProjection>,
    agents: Vec<AgentProjection>,
    history: Vec<AgentHistoryEntry>,
    assignments: Vec<AgentAssignment>,
}

#[derive(Serialize)]
struct RegistryResponse {
    schema_version: u32,
    contract_id: &'static str,
    status: String,
    operation: String,
    agent_id: String,
    revision: u64,
    projection: Option<AgentProjection>,
    agents: Vec<AgentProjection>,
    history: Vec<AgentHistoryEntry>,
    assignments: Vec<AgentAssignment>,
    cost_status: &'static str,
    redacted: bool,
}

fn response(input: ResponseInput<'_>) -> serde_json::Value {
    serde_json::to_value(RegistryResponse {
        schema_version: CONTRACT_VERSION,
        contract_id: CONTRACT_ID,
        status: input.status.into(),
        operation: input.operation.into(),
        agent_id: input.agent_id.into(),
        revision: input.revision,
        projection: input.projection,
        agents: input.agents,
        history: input.history,
        assignments: input.assignments,
        cost_status: "unavailable",
        redacted: true,
    })
    .expect("typed registry response is serializable")
}

fn write_agent(
    connection: &rusqlite::Connection,
    agent: &PersistentAgent,
    actor: &str,
    event_type: &str,
    now_ms: i64,
) -> Result<(), crate::StorageError> {
    let json = serde_json::to_vec(agent)?;
    let stored = evohime_local_storage::persistent_agent_registry_store::save_agent_revision(
        connection,
        &agent.id,
        agent.revision,
        &format!("{:?}", agent.status).to_ascii_lowercase(),
        &agent.content_hash,
        &json,
        actor,
        now_ms,
    )
    .map_err(map_storage_error)?;
    if !stored {
        return Err(crate::StorageError::InvalidInput(
            RegistryError::Stale.to_string(),
        ));
    }
    evohime_local_storage::persistent_agent_registry_store::save_reporting_history(
        connection,
        &agent.id,
        agent.revision,
        agent.reporting_to_agent_id.as_deref(),
        event_type,
        actor,
        now_ms,
    )
    .map_err(map_storage_error)
}

fn execute(
    connection: &rusqlite::Connection,
    command: &RegistryCommand,
) -> Result<serde_json::Value, crate::StorageError> {
    if command.payload.len() > MAX_PROJECTION_BYTES {
        return Err(crate::StorageError::InvalidInput(
            RegistryError::PayloadTooLarge.to_string(),
        ));
    }
    let value: serde_json::Value = if command.payload.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice(&command.payload)?
    };
    let now_ms = crate::task_memory::now_millis() as i64;
    match command.operation.as_str() {
        "list" => {
            let mut agents = Vec::new();
            for agent in load_agents_map(connection)?.into_values() {
                agents.push(projection(connection, &agent)?);
            }
            Ok(response(ResponseInput {
                operation: "list",
                agent_id: "",
                revision: 0,
                status: "ok",
                projection: None,
                agents,
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "get" | "availability" | "activity" | "resolve" | "history" => {
            let bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &command.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let agent = deserialize_agent(&bytes)
                .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
            authorize_scope(&agent, &command.owner_scope)?;
            if command.operation == "resolve" {
                if let Some(warning) = role_profile_binding(connection, &agent.role_profile_ref)? {
                    return Err(crate::StorageError::InvalidInput(
                        RegistryError::BrokenBinding(warning).to_string(),
                    ));
                }
                let goal_ref = value
                    .get("goal_ref")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|_| crate::StorageError::InvalidInput("invalid_goal_ref".into()))?;
                if let Some(goal) = &goal_ref {
                    if !goal_exists(connection, goal)? {
                        return Err(crate::StorageError::InvalidInput(
                            RegistryError::BrokenBinding("missing_goal_revision").to_string(),
                        ));
                    }
                }
                let agents = load_agents_map(connection)?;
                let snapshot = build_execution_snapshot(
                    &agent,
                    reporting_chain(&agents, &agent.id)
                        .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?,
                    goal_ref,
                )
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                return Ok(response(ResponseInput {
                    operation: "resolve",
                    agent_id: &agent.id,
                    revision: agent.revision,
                    status: "resolved",
                    projection: Some(projection(connection, &agent)?),
                    agents: Vec::new(),
                    history: Vec::new(),
                    assignments: vec![AgentAssignment {
                        schema_version: CONTRACT_VERSION,
                        id: format!("snapshot:{}", agent.id),
                        revision: agent.revision,
                        agent_id: agent.id.clone(),
                        source_kind: AssignmentSourceKind::Run,
                        source_ref: String::new(),
                        goal_ref: snapshot.goal_ref.clone(),
                        team_session_ref: None,
                        task_ref: None,
                        status: AssignmentStatus::Active,
                        created_by: "core".into(),
                        created_at_ms: now_ms,
                        updated_at_ms: now_ms,
                        execution_snapshot: Some(snapshot),
                        provenance_hash: None,
                        content_hash: String::new(),
                    }],
                }));
            }
            if command.operation == "history" {
                let history =
                    evohime_local_storage::persistent_agent_registry_store::load_reporting_history(
                        connection,
                        &agent.id,
                        value
                            .get("limit")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(MAX_HISTORY as u64) as usize,
                    )
                    .map_err(map_storage_error)?
                    .into_iter()
                    .map(|(revision, _parent, event_type, actor, created_at_ms)| {
                        AgentHistoryEntry {
                            revision,
                            event_type: event_type.clone(),
                            actor,
                            summary_hash: hex::encode(Sha256::digest(event_type.as_bytes())),
                            created_at_ms,
                        }
                    })
                    .collect();
                return Ok(response(ResponseInput {
                    operation: "history",
                    agent_id: &agent.id,
                    revision: agent.revision,
                    status: "ok",
                    projection: None,
                    agents: Vec::new(),
                    history,
                    assignments: Vec::new(),
                }));
            }
            let assignments = effective_assignments(connection, &agent.id)?;
            let mut item = projection(connection, &agent)?;
            if command.operation == "availability" {
                item.availability = assignment_availability(&assignments);
            }
            Ok(response(ResponseInput {
                operation: &command.operation,
                agent_id: &agent.id,
                revision: agent.revision,
                status: "ok",
                projection: Some(item),
                agents: Vec::new(),
                history: Vec::new(),
                assignments: if command.operation == "activity" {
                    assignments
                } else {
                    Vec::new()
                },
            }))
        }
        "create" => {
            let mut agent: PersistentAgent = serde_json::from_value(value)
                .map_err(|_| crate::StorageError::InvalidInput("invalid_agent".into()))?;
            if !command.agent_id.is_empty() && agent.id != command.agent_id {
                return Err(crate::StorageError::InvalidInput(
                    "agent_id_mismatch".into(),
                ));
            }
            agent = prepare_agent(agent)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            if scope_token(&agent.organization_scope) != command.owner_scope {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::ScopeMismatch.to_string(),
                ));
            }
            if evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection, &agent.id,
            )
            .map_err(map_storage_error)?
            .is_some()
            {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::Duplicate.to_string(),
                ));
            }
            write_agent(connection, &agent, &command.actor, "created", now_ms)?;
            Ok(response(ResponseInput {
                operation: "create",
                agent_id: &agent.id,
                revision: agent.revision,
                status: "created",
                projection: Some(projection(connection, &agent)?),
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "revise" => {
            let old_bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &command.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let old = deserialize_agent(&old_bytes)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            if old.revision != command.expected_revision {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::Stale.to_string(),
                ));
            }
            let mut next: PersistentAgent = serde_json::from_value(value)
                .map_err(|_| crate::StorageError::InvalidInput("invalid_agent".into()))?;
            if next.id != old.id || next.revision != old.revision + 1 {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::Stale.to_string(),
                ));
            }
            next.created_at_ms = old.created_at_ms;
            next.updated_at_ms = now_ms;
            next = prepare_agent(next)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            if scope_token(&next.organization_scope) != command.owner_scope {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::ScopeMismatch.to_string(),
                ));
            }
            let agents = load_agents_map(connection)?;
            reporting_graph_valid(&agents, &next.id, next.reporting_to_agent_id.as_deref())
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            write_agent(connection, &next, &command.actor, "revised", now_ms)?;
            Ok(response(ResponseInput {
                operation: "revise",
                agent_id: &next.id,
                revision: next.revision,
                status: "updated",
                projection: Some(projection(connection, &next)?),
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "activate" | "pause" | "suspend" | "resume" | "retire" => {
            let bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &command.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let mut agent = deserialize_agent(&bytes)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            if agent.revision != command.expected_revision {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::Stale.to_string(),
                ));
            }
            authorize_scope(&agent, &command.owner_scope)?;
            let next_status = match command.operation.as_str() {
                "activate" | "resume" => LifecycleStatus::Active,
                "pause" => LifecycleStatus::Paused,
                "suspend" => LifecycleStatus::Suspended,
                _ => LifecycleStatus::Retired,
            };
            transition_status(agent.status, next_status)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            agent.status = next_status;
            agent.revision += 1;
            agent.updated_at_ms = now_ms;
            agent.content_hash.clear();
            agent.content_hash = canonical_hash(&agent)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            write_agent(
                connection,
                &agent,
                &command.actor,
                &command.operation,
                now_ms,
            )?;
            Ok(response(ResponseInput {
                operation: &command.operation,
                agent_id: &agent.id,
                revision: agent.revision,
                status: "updated",
                projection: Some(projection(connection, &agent)?),
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "reporting_set" => {
            let bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &command.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let mut agent = deserialize_agent(&bytes)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            if agent.revision != command.expected_revision {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::Stale.to_string(),
                ));
            }
            authorize_scope(&agent, &command.owner_scope)?;
            let parent = value
                .get("parent_agent_id")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty());
            let agents = load_agents_map(connection)?;
            reporting_graph_valid(&agents, &agent.id, parent)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            agent.reporting_to_agent_id = parent.map(str::to_owned);
            agent.revision += 1;
            agent.updated_at_ms = now_ms;
            agent.content_hash.clear();
            agent.content_hash = canonical_hash(&agent)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            write_agent(connection, &agent, &command.actor, "reporting_set", now_ms)?;
            Ok(response(ResponseInput {
                operation: "reporting_set",
                agent_id: &agent.id,
                revision: agent.revision,
                status: "updated",
                projection: Some(projection(connection, &agent)?),
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "goal_bind" => {
            let mut binding: AgentGoalBinding = serde_json::from_value(value)
                .map_err(|_| crate::StorageError::InvalidInput("invalid_goal_binding".into()))?;
            if binding.agent_id != command.agent_id {
                return Err(crate::StorageError::InvalidInput(
                    "agent_id_mismatch".into(),
                ));
            }
            let agent_bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &binding.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let agent = deserialize_agent(&agent_bytes)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            authorize_scope(&agent, &command.owner_scope)?;
            validate_actor(&command.actor)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            let goal = GoalRef {
                goal_id: binding.goal_id.clone(),
                goal_revision: binding.goal_revision,
            };
            validate_goal_ref(&goal)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            if !goal_exists(connection, &goal)? {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::BrokenBinding("missing_goal_revision").to_string(),
                ));
            }
            binding.created_at_ms = if binding.created_at_ms == 0 {
                now_ms
            } else {
                binding.created_at_ms
            };
            binding.content_hash = hex::encode(Sha256::digest(serde_json::to_vec(&binding)?));
            let json = serde_json::to_vec(&binding)?;
            evohime_local_storage::persistent_agent_registry_store::save_goal_binding(
                connection,
                &binding.agent_id,
                &binding.goal_id,
                binding.goal_revision,
                &format!("{:?}", binding.responsibility).to_ascii_lowercase(),
                binding
                    .scope
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()
                    .map_err(crate::StorageError::from)?
                    .as_deref(),
                &json,
                now_ms,
            )
            .map_err(map_storage_error)?;
            Ok(response(ResponseInput {
                operation: "goal_bind",
                agent_id: &binding.agent_id,
                revision: command.expected_revision,
                status: "bound",
                projection: None,
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "goal_unbind" => {
            let goal_id = value
                .get("goal_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| crate::StorageError::InvalidInput("goal_id_required".into()))?;
            let goal_revision = value
                .get("goal_revision")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| {
                    crate::StorageError::InvalidInput("goal_revision_required".into())
                })?;
            let responsibility = value
                .get("responsibility")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    crate::StorageError::InvalidInput("responsibility_required".into())
                })?;
            evohime_local_storage::persistent_agent_registry_store::remove_goal_binding(
                connection,
                &command.agent_id,
                goal_id,
                goal_revision,
                responsibility,
            )
            .map_err(map_storage_error)?;
            Ok(response(ResponseInput {
                operation: "goal_unbind",
                agent_id: &command.agent_id,
                revision: command.expected_revision,
                status: "unbound",
                projection: None,
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
        }
        "assignment_create" => {
            let mut assignment: AgentAssignment = serde_json::from_value(value)
                .map_err(|_| crate::StorageError::InvalidInput("invalid_assignment".into()))?;
            if assignment.agent_id != command.agent_id {
                return Err(crate::StorageError::InvalidInput(
                    "agent_id_mismatch".into(),
                ));
            }
            assignment = prepare_assignment(assignment)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            let agent_bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &assignment.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let agent = deserialize_agent(&agent_bytes)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            authorize_scope(&agent, &command.owner_scope)?;
            if !source_exists(connection, assignment.source_kind, &assignment.source_ref)? {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::BrokenBinding("source_not_found").to_string(),
                ));
            }
            if let Some(goal) = &assignment.goal_ref {
                if !goal_exists(connection, goal)? {
                    return Err(crate::StorageError::InvalidInput(
                        RegistryError::BrokenBinding("missing_goal_revision").to_string(),
                    ));
                }
            }
            if let Some(warning) = role_profile_binding(connection, &agent.role_profile_ref)? {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::BrokenBinding(warning).to_string(),
                ));
            }
            let agents = load_agents_map(connection)?;
            let snapshot = build_execution_snapshot(
                &agent,
                reporting_chain(&agents, &agent.id)
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?,
                assignment.goal_ref.clone(),
            )
            .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            assignment.execution_snapshot = Some(snapshot);
            assignment.status = source_assignment_status(
                connection,
                assignment.source_kind,
                &assignment.source_ref,
            )?
            .unwrap_or(AssignmentStatus::UnknownAfterRestart);
            assignment.updated_at_ms = now_ms;
            assignment.content_hash = assignment_hash(&assignment)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            let json = serde_json::to_vec(&assignment)?;
            evohime_local_storage::persistent_agent_registry_store::save_assignment(
                connection,
                &assignment.id,
                assignment.revision,
                &assignment.agent_id,
                "active",
                &format!("{:?}", assignment.source_kind).to_ascii_lowercase(),
                &assignment.source_ref,
                &json,
                now_ms,
            )
            .map_err(map_storage_error)?;
            let assignment_agent_id = assignment.agent_id.clone();
            Ok(response(ResponseInput {
                operation: "assignment_create",
                agent_id: &assignment_agent_id,
                revision: assignment.revision,
                status: "assigned",
                projection: Some(projection(connection, &agent)?),
                agents: Vec::new(),
                history: Vec::new(),
                assignments: vec![assignment],
            }))
        }
        "assignment_cancel" => {
            let id = value
                .get("assignment_id")
                .and_then(serde_json::Value::as_str)
                .or_else(|| (!command.agent_id.is_empty()).then_some(command.agent_id.as_str()))
                .ok_or_else(|| {
                    crate::StorageError::InvalidInput("assignment_id_required".into())
                })?;
            let bytes = evohime_local_storage::persistent_agent_registry_store::load_assignment(
                connection, id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let mut assignment: AgentAssignment = serde_json::from_slice(&bytes)?;
            let agent_bytes = evohime_local_storage::persistent_agent_registry_store::load_agent(
                connection,
                &assignment.agent_id,
            )
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                crate::StorageError::InvalidInput(RegistryError::NotFound.to_string())
            })?;
            let agent = deserialize_agent(&agent_bytes)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            authorize_scope(&agent, &command.owner_scope)?;
            if assignment.revision != command.expected_revision {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::Stale.to_string(),
                ));
            }
            assignment.revision += 1;
            assignment.status = AssignmentStatus::Cancelled;
            assignment.updated_at_ms = now_ms;
            assignment.content_hash.clear();
            assignment.content_hash = assignment_hash(&assignment)
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
            let json = serde_json::to_vec(&assignment)?;
            evohime_local_storage::persistent_agent_registry_store::save_assignment(
                connection,
                &assignment.id,
                assignment.revision,
                &assignment.agent_id,
                "cancelled",
                &format!("{:?}", assignment.source_kind).to_ascii_lowercase(),
                &assignment.source_ref,
                &json,
                now_ms,
            )
            .map_err(map_storage_error)?;
            let assignment_agent_id = assignment.agent_id.clone();
            Ok(response(ResponseInput {
                operation: "assignment_cancel",
                agent_id: &assignment_agent_id,
                revision: assignment.revision,
                status: "cancelled",
                projection: None,
                agents: Vec::new(),
                history: Vec::new(),
                assignments: vec![assignment],
            }))
        }
        "recover" => {
            let mut recovered = 0usize;
            let all = evohime_local_storage::persistent_agent_registry_store::load_assignments(
                connection, 512,
            )
            .map_err(map_storage_error)?;
            for bytes in all {
                let mut assignment: AgentAssignment = serde_json::from_slice(&bytes)?;
                if matches!(
                    assignment.status,
                    AssignmentStatus::Active | AssignmentStatus::Pending
                ) {
                    let next_status = source_assignment_status(
                        connection,
                        assignment.source_kind,
                        &assignment.source_ref,
                    )?
                    .unwrap_or(AssignmentStatus::UnknownAfterRestart);
                    if next_status == assignment.status {
                        continue;
                    }
                    assignment.status = next_status;
                    assignment.revision += 1;
                    assignment.updated_at_ms = now_ms;
                    assignment.content_hash.clear();
                    assignment.content_hash = assignment_hash(&assignment)
                        .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                    let json = serde_json::to_vec(&assignment)?;
                    evohime_local_storage::persistent_agent_registry_store::save_assignment(
                        connection,
                        &assignment.id,
                        assignment.revision,
                        &assignment.agent_id,
                        "unknown_after_restart",
                        &format!("{:?}", assignment.source_kind).to_ascii_lowercase(),
                        &assignment.source_ref,
                        &json,
                        now_ms,
                    )
                    .map_err(map_storage_error)?;
                    recovered += 1;
                }
            }
            Ok(response(ResponseInput {
                operation: "recover",
                agent_id: &command.agent_id,
                revision: 0,
                status: "reconciled",
                projection: None,
                agents: Vec::new(),
                history: Vec::new(),
                assignments: Vec::new(),
            }))
            .map(|mut value| {
                value["recovered_assignments"] = serde_json::json!(recovered);
                value
            })
        }
        _ => Err(crate::StorageError::InvalidInput(
            "unsupported_registry_operation".into(),
        )),
    }
}

impl crate::EventJournal {
    pub async fn persistent_agent_registry_command(
        &self,
        command: RegistryCommand,
    ) -> Result<Vec<u8>, crate::StorageError> {
        validate_actor(&command.actor)
            .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
        if command.idempotency_key.is_empty() || !valid_id(&command.idempotency_key) {
            return Err(crate::StorageError::InvalidInput(
                "idempotency_key_required".into(),
            ));
        }
        let hash = command_hash(&command)?;
        let database = self.database.lock().await;
        if let Some((previous_hash, outcome)) =
            evohime_local_storage::persistent_agent_registry_store::load_command_outcome(
                database.connection(),
                &command.idempotency_key,
            )
            .map_err(map_storage_error)?
        {
            if previous_hash != hash {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::IdempotencyConflict.to_string(),
                ));
            }
            let cached: serde_json::Value = serde_json::from_slice(&outcome)?;
            if cached.get("ok") == Some(&serde_json::Value::Bool(false)) {
                return Err(crate::StorageError::InvalidInput(
                    cached
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("cached_registry_error")
                        .into(),
                ));
            }
            return Ok(cached
                .get("result")
                .and_then(|value| serde_json::to_vec(value).ok())
                .unwrap_or(outcome));
        }
        let result = execute(database.connection(), &command);
        let (ok, body) = match &result {
            Ok(value) => (true, serde_json::to_vec(value)?),
            Err(error) => (false, error.to_string().into_bytes()),
        };
        let outcome = if ok {
            serde_json::json!({"ok": true, "result": serde_json::from_slice::<serde_json::Value>(&body)?})
        } else {
            serde_json::json!({"ok": false, "error": String::from_utf8_lossy(&body)})
        };
        let outcome_bytes = serde_json::to_vec(&outcome)?;
        if let Some((previous_hash, previous)) =
            evohime_local_storage::persistent_agent_registry_store::record_command_outcome(
                database.connection(),
                &command.idempotency_key,
                &hash,
                &outcome_bytes,
                crate::task_memory::now_millis() as i64,
            )
            .map_err(map_storage_error)?
        {
            if previous_hash != hash {
                return Err(crate::StorageError::InvalidInput(
                    RegistryError::IdempotencyConflict.to_string(),
                ));
            }
            let cached: serde_json::Value = serde_json::from_slice(&previous)?;
            if cached.get("ok") == Some(&serde_json::Value::Bool(true)) {
                return Ok(serde_json::to_vec(&cached["result"])?);
            }
            return Err(crate::StorageError::InvalidInput(
                cached
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("cached_registry_error")
                    .into(),
            ));
        }
        result
            .map(|_| body)
            .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))
    }

    pub async fn recover_persistent_agent_registry(&self) -> Result<usize, crate::StorageError> {
        let command = RegistryCommand {
            operation: "recover".into(),
            agent_id: String::new(),
            owner_scope: "application:application".into(),
            actor: "core".into(),
            payload: Vec::new(),
            expected_revision: 0,
            idempotency_key: format!("startup-{}", crate::task_memory::now_millis()),
        };
        let bytes = self.persistent_agent_registry_command(command).await?;
        Ok(serde_json::from_slice::<serde_json::Value>(&bytes)?
            .get("recovered_assignments")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize)
    }
}
