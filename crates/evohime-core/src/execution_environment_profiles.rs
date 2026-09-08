//! Core-owned execution environment profile contract.
//!
//! Profiles compose bounded references; owners remain authoritative.  The
//! resolver is deliberately injected so neither SQLite nor the renderer can
//! turn a string reference into a capability grant.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BINDINGS: usize = 32;
pub const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentScope {
    Application,
    Workspace,
    Project,
    ConversationDefault,
}
impl EnvironmentScope {
    pub fn precedence(self) -> u8 {
        match self {
            Self::Application => 0,
            Self::Workspace => 1,
            Self::Project => 2,
            Self::ConversationDefault => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    ModelRouting,
    ExecutionBackend,
    ExternalAgentPreset,
    Workbench,
    McpServer,
    SkillSet,
    InstructionStack,
    ExecutionPolicy,
    ApprovalPolicy,
    ContinuationPolicy,
    BudgetPolicy,
    CredentialBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceMode {
    PinnedRevision,
    FollowCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileState {
    Ready,
    NeedsReview,
    Degraded,
    Broken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeBoundary {
    NewRunOnly,
    NextTurn,
    NewConversationOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentBinding {
    pub kind: BindingKind,
    pub reference: String,
    pub revision: u64,
    pub mode: ReferenceMode,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEnvironmentProfile {
    pub id: String,
    pub revision: u64,
    pub scope: EnvironmentScope,
    pub bindings: Vec<EnvironmentBinding>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingDiagnostic {
    pub kind: BindingKind,
    pub reference: String,
    pub code: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preflight {
    pub state: ProfileState,
    pub boundary: SafeBoundary,
    pub diagnostics: Vec<BindingDiagnostic>,
    pub resolved_revisions: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveEnvironmentSnapshot {
    pub profile_id: String,
    pub profile_revision: u64,
    pub profile_hash: String,
    pub state: ProfileState,
    pub boundary: SafeBoundary,
    pub resolved_revisions: BTreeMap<String, u64>,
    pub snapshot_hash: String,
}

#[derive(Debug, Clone)]
pub struct EnvironmentProfileCommand {
    pub operation: String,
    pub profile_id: String,
    pub owner_scope: String,
    pub payload: Vec<u8>,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

fn command_hash(command: &EnvironmentProfileCommand) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            [
                command.owner_scope.as_bytes(),
                b"\0",
                command.operation.as_bytes(),
                b"\0",
                command.profile_id.as_bytes(),
                b"\0",
                &command.expected_revision.to_le_bytes(),
                b"\0",
                &command.payload,
            ]
            .concat()
        )
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerAvailability {
    Available { revision: u64, compatible: bool },
    Missing,
    UnavailableOwner,
    PolicyDenied,
    ScopeDenied,
}

pub trait EnvironmentBindingResolver {
    fn resolve(&self, binding: &EnvironmentBinding) -> OwnerAvailability;
}

/// Read-only adapter over established owner metadata.  It is intentionally
/// conservative: an owner without a stable revision lookup is unavailable,
/// rather than guessed from a user-controlled reference.
pub struct SqliteEnvironmentResolver<'a> {
    connection: &'a Connection,
}
impl<'a> SqliteEnvironmentResolver<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
}
impl EnvironmentBindingResolver for SqliteEnvironmentResolver<'_> {
    fn resolve(&self, binding: &EnvironmentBinding) -> OwnerAvailability {
        let revision: rusqlite::Result<Option<u64>> = match binding.kind {
            BindingKind::ModelRouting => self.connection.query_row("SELECT version FROM model_purpose_routing WHERE policy_id=?1", [&binding.reference], |row| row.get(0)).optional(),
            BindingKind::ExecutionBackend => self.connection.query_row("SELECT version FROM execution_backends WHERE id=?1 AND enabled=1 AND health='ready'", [&binding.reference], |row| row.get(0)).optional(),
            BindingKind::ExternalAgentPreset => self.connection.query_row("SELECT revision FROM external_agent_presets WHERE id=?1 AND enabled=1", [&binding.reference], |row| row.get(0)).optional(),
            BindingKind::ExecutionPolicy => self.connection.query_row("SELECT version FROM execution_policy_profiles WHERE profile_id=?1", [&binding.reference], |row| row.get(0)).optional(),
            BindingKind::ApprovalPolicy => self.connection.query_row("SELECT version FROM approval_policy_profiles WHERE id=?1 AND enabled=1", [&binding.reference], |row| row.get(0)).optional(),
            _ => return OwnerAvailability::UnavailableOwner,
        };
        match revision {
            Ok(Some(revision)) => OwnerAvailability::Available {
                revision,
                compatible: true,
            },
            Ok(None) => OwnerAvailability::Missing,
            Err(_) => OwnerAvailability::UnavailableOwner,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvironmentProfileError {
    #[error("invalid profile id")]
    InvalidId,
    #[error("invalid binding")]
    InvalidBinding,
    #[error("duplicate binding")]
    DuplicateBinding,
    #[error("profile is not activatable: {0:?}")]
    NotActivatable(ProfileState),
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
}
fn hash<T: Serialize>(value: &T) -> Result<String, EnvironmentProfileError> {
    let bytes = serde_json::to_vec(value).map_err(|_| EnvironmentProfileError::InvalidBinding)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn canonical_profile_hash(
    profile: &ExecutionEnvironmentProfile,
) -> Result<String, EnvironmentProfileError> {
    hash(&(
        profile.id.clone(),
        profile.revision,
        profile.scope,
        profile.bindings.clone(),
    ))
}

fn validate_profile(profile: &ExecutionEnvironmentProfile) -> Result<(), EnvironmentProfileError> {
    let rebuilt = ExecutionEnvironmentProfile::new(
        profile.id.clone(),
        profile.revision,
        profile.scope,
        profile.bindings.clone(),
    )?;
    if rebuilt.content_hash != profile.content_hash {
        return Err(EnvironmentProfileError::InvalidBinding);
    }
    Ok(())
}

impl ExecutionEnvironmentProfile {
    pub fn new(
        id_value: String,
        revision: u64,
        scope: EnvironmentScope,
        bindings: Vec<EnvironmentBinding>,
    ) -> Result<Self, EnvironmentProfileError> {
        if !id(&id_value) || revision == 0 || bindings.is_empty() || bindings.len() > MAX_BINDINGS {
            return Err(EnvironmentProfileError::InvalidId);
        }
        let mut seen = BTreeSet::new();
        for binding in &bindings {
            if !id(&binding.reference)
                || binding.revision == 0
                || !seen.insert((binding.kind, binding.reference.as_str()))
            {
                return Err(if !id(&binding.reference) || binding.revision == 0 {
                    EnvironmentProfileError::InvalidBinding
                } else {
                    EnvironmentProfileError::DuplicateBinding
                });
            }
        }
        let mut profile = Self {
            id: id_value,
            revision,
            scope,
            bindings,
            content_hash: String::new(),
        };
        profile.content_hash = canonical_profile_hash(&profile)?;
        Ok(profile)
    }
}

pub fn preflight(
    profile: &ExecutionEnvironmentProfile,
    resolver: &impl EnvironmentBindingResolver,
) -> Preflight {
    let mut diagnostics = Vec::new();
    let mut resolved_revisions = BTreeMap::new();
    let mut boundary = SafeBoundary::NextTurn;
    for binding in &profile.bindings {
        match resolver.resolve(binding) {
            OwnerAvailability::Available {
                revision,
                compatible: _,
            } if binding.mode == ReferenceMode::PinnedRevision && revision != binding.revision => {
                diagnostics.push(BindingDiagnostic {
                    kind: binding.kind,
                    reference: binding.reference.clone(),
                    code: "pinned_revision_drift".into(),
                    required: binding.required,
                })
            }
            OwnerAvailability::Available {
                revision: _,
                compatible: false,
            } => diagnostics.push(BindingDiagnostic {
                kind: binding.kind,
                reference: binding.reference.clone(),
                code: "incompatible".into(),
                required: binding.required,
            }),
            OwnerAvailability::Available {
                revision,
                compatible: true,
            } => {
                resolved_revisions.insert(
                    format!("{:?}:{}", binding.kind, binding.reference),
                    revision,
                );
            }
            OwnerAvailability::Missing => diagnostics.push(BindingDiagnostic {
                kind: binding.kind,
                reference: binding.reference.clone(),
                code: "missing".into(),
                required: binding.required,
            }),
            OwnerAvailability::UnavailableOwner => diagnostics.push(BindingDiagnostic {
                kind: binding.kind,
                reference: binding.reference.clone(),
                code: "unavailable_owner".into(),
                required: binding.required,
            }),
            OwnerAvailability::PolicyDenied => diagnostics.push(BindingDiagnostic {
                kind: binding.kind,
                reference: binding.reference.clone(),
                code: "policy_denied".into(),
                required: binding.required,
            }),
            OwnerAvailability::ScopeDenied => diagnostics.push(BindingDiagnostic {
                kind: binding.kind,
                reference: binding.reference.clone(),
                code: "scope_denied".into(),
                required: binding.required,
            }),
        }
        if matches!(
            binding.kind,
            BindingKind::ExecutionBackend
                | BindingKind::ExternalAgentPreset
                | BindingKind::Workbench
                | BindingKind::McpServer
                | BindingKind::CredentialBinding
        ) {
            boundary = SafeBoundary::NewRunOnly;
        }
    }
    let state = if diagnostics.is_empty() {
        ProfileState::Ready
    } else if diagnostics.iter().any(|d| {
        d.required
            && matches!(
                d.code.as_str(),
                "missing" | "unavailable_owner" | "policy_denied" | "scope_denied"
            )
    }) {
        ProfileState::Broken
    } else if diagnostics.iter().any(|d| d.required) {
        ProfileState::NeedsReview
    } else {
        ProfileState::Degraded
    };
    Preflight {
        state,
        boundary,
        diagnostics,
        resolved_revisions,
    }
}

pub fn effective_snapshot(
    profile: &ExecutionEnvironmentProfile,
    check: &Preflight,
) -> Result<EffectiveEnvironmentSnapshot, EnvironmentProfileError> {
    if !matches!(check.state, ProfileState::Ready | ProfileState::Degraded) {
        return Err(EnvironmentProfileError::NotActivatable(check.state));
    }
    let seed = (
        &profile.id,
        profile.revision,
        &profile.content_hash,
        check.state,
        check.boundary,
        &check.resolved_revisions,
    );
    Ok(EffectiveEnvironmentSnapshot {
        profile_id: profile.id.clone(),
        profile_revision: profile.revision,
        profile_hash: profile.content_hash.clone(),
        state: check.state,
        boundary: check.boundary,
        resolved_revisions: check.resolved_revisions.clone(),
        snapshot_hash: hash(&seed)?,
    })
}

impl crate::EventJournal {
    pub async fn execution_environment_profile_command(
        &self,
        command: EnvironmentProfileCommand,
    ) -> Result<Vec<u8>, crate::StorageError> {
        if !id(&command.owner_scope) || !id(&command.idempotency_key) {
            return Err(crate::StorageError::InvalidInput(
                "invalid_environment_command".into(),
            ));
        }
        let database = self.database.lock().await;
        let connection = database.connection();
        let mutation_hash = if matches!(
            command.operation.as_str(),
            "activate" | "rollback" | "create" | "revise"
        ) {
            let hash = command_hash(&command);
            if let Some((stored_hash, response)) = evohime_local_storage::execution_environment_profiles_store::load_idempotent_command(
                connection,
                &command.owner_scope,
                &command.idempotency_key,
            )
            .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?
            {
                if stored_hash != hash {
                    return Err(crate::StorageError::InvalidInput(
                        "environment_profile_idempotency_conflict".into(),
                    ));
                }
                return Ok(response);
            }
            Some(hash)
        } else {
            None
        };
        let result = match command.operation.as_str() {
            "list" => {
                let profiles =
                    evohime_local_storage::execution_environment_profiles_store::load_profiles(
                        connection, 128,
                    )
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                serde_json::json!({"operation":"list","profiles": profiles.into_iter().filter_map(|value| serde_json::from_slice::<serde_json::Value>(&value).ok()).collect::<Vec<_>>()})
            }
            "history" => {
                let history =
                    evohime_local_storage::execution_environment_profiles_store::load_activations(
                        connection,
                        &command.owner_scope,
                        128,
                    )
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                serde_json::json!({"operation":"history","activations": history.into_iter().filter_map(|value| serde_json::from_slice::<serde_json::Value>(&value).ok()).collect::<Vec<_>>()})
            }
            "current" => {
                let snapshot =
                    evohime_local_storage::execution_environment_profiles_store::load_current(
                        connection,
                        &command.owner_scope,
                    )
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?
                    .ok_or_else(|| {
                        crate::StorageError::InvalidInput("environment_profile_not_found".into())
                    })?;
                let snapshot: EffectiveEnvironmentSnapshot = serde_json::from_slice(&snapshot)?;
                serde_json::json!({"operation":"current","snapshot":snapshot})
            }
            "get" | "preflight" | "activate" => {
                let bytes =
                    evohime_local_storage::execution_environment_profiles_store::load_profile(
                        connection,
                        &command.profile_id,
                    )
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?
                    .ok_or_else(|| {
                        crate::StorageError::InvalidInput("environment_profile_not_found".into())
                    })?;
                let profile: ExecutionEnvironmentProfile = serde_json::from_slice(&bytes)?;
                let check = preflight(&profile, &SqliteEnvironmentResolver::new(connection));
                if command.operation == "activate" {
                    let snapshot = effective_snapshot(&profile, &check)
                        .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                    let activation = serde_json::json!({"operation":"activate","profile_id":profile.id,"revision":profile.revision,"status":format!("{:?}",check.state),"snapshot_hash":snapshot.snapshot_hash});
                    evohime_local_storage::execution_environment_profiles_store::save_activation(
                        connection,
                        evohime_local_storage::execution_environment_profiles_store::SaveActivationInput { profile_id: &profile.id, revision: profile.revision, scope: &command.owner_scope, status: "activated", snapshot_hash: &snapshot.snapshot_hash, activation_json: &serde_json::to_vec(&activation)?, snapshot_json: &serde_json::to_vec(&snapshot)?, now_ms: crate::task_memory::now_millis() as i64 },
                    )
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                    activation
                } else {
                    serde_json::json!({"operation":command.operation,"profile":profile,"preflight":check})
                }
            }
            "rollback" => {
                if command.expected_revision != 0 {
                    let current =
                        evohime_local_storage::execution_environment_profiles_store::load_current(
                            connection,
                            &command.owner_scope,
                        )?
                        .ok_or_else(|| {
                            crate::StorageError::InvalidInput(
                                "environment_profile_not_found".into(),
                            )
                        })?;
                    let current: EffectiveEnvironmentSnapshot = serde_json::from_slice(&current)?;
                    if current.profile_revision != command.expected_revision {
                        return Err(crate::StorageError::InvalidInput(
                            "stale_environment_profile_revision".into(),
                        ));
                    }
                }
                let bytes =
                    evohime_local_storage::execution_environment_profiles_store::load_profile(
                        connection,
                        &command.profile_id,
                    )
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?
                    .ok_or_else(|| {
                        crate::StorageError::InvalidInput("environment_profile_not_found".into())
                    })?;
                let profile: ExecutionEnvironmentProfile = serde_json::from_slice(&bytes)?;
                let check = preflight(&profile, &SqliteEnvironmentResolver::new(connection));
                let snapshot = effective_snapshot(&profile, &check)
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                let activation = serde_json::json!({"operation":"rollback","profile_id":profile.id,"revision":profile.revision,"status":format!("{:?}",check.state),"snapshot_hash":snapshot.snapshot_hash});
                evohime_local_storage::execution_environment_profiles_store::save_activation(
                    connection,
                    evohime_local_storage::execution_environment_profiles_store::SaveActivationInput { profile_id: &profile.id, revision: profile.revision, scope: &command.owner_scope, status: "rolled_back", snapshot_hash: &snapshot.snapshot_hash, activation_json: &serde_json::to_vec(&activation)?, snapshot_json: &serde_json::to_vec(&snapshot)?, now_ms: crate::task_memory::now_millis() as i64 },
                )
                .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                activation
            }
            "create" | "revise" => {
                let profile: ExecutionEnvironmentProfile =
                    serde_json::from_slice(&command.payload)?;
                validate_profile(&profile)
                    .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                if profile.id != command.profile_id || profile.revision == 0 {
                    return Err(crate::StorageError::InvalidInput(
                        "invalid_environment_profile".into(),
                    ));
                }
                if command.operation == "revise" {
                    let existing =
                        evohime_local_storage::execution_environment_profiles_store::load_profile(
                            connection,
                            &profile.id,
                        )
                        .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?
                        .ok_or_else(|| {
                            crate::StorageError::InvalidInput(
                                "environment_profile_not_found".into(),
                            )
                        })?;
                    let current: ExecutionEnvironmentProfile = serde_json::from_slice(&existing)?;
                    if command.expected_revision != current.revision
                        || profile.revision != current.revision.saturating_add(1)
                    {
                        return Err(crate::StorageError::InvalidInput(
                            "stale_environment_profile_revision".into(),
                        ));
                    }
                }
                let check = preflight(&profile, &SqliteEnvironmentResolver::new(connection));
                let json = serde_json::to_vec(&profile)?;
                if !evohime_local_storage::execution_environment_profiles_store::save_profile_revision(connection, evohime_local_storage::execution_environment_profiles_store::SaveProfileRevisionInput { id: &profile.id, revision: profile.revision, scope: &command.owner_scope, state: &format!("{:?}",check.state), hash: &profile.content_hash, json: &json, actor: "user", now_ms: crate::task_memory::now_millis() as i64 }).map_err(|e| crate::StorageError::InvalidInput(e.to_string()))? { return Err(crate::StorageError::InvalidInput("stale_environment_profile_revision".into())); }
                let response = serde_json::to_vec(
                    &serde_json::json!({"operation":command.operation,"profile":profile,"preflight":check}),
                )?;
                evohime_local_storage::execution_environment_profiles_store::save_idempotent_command(connection, &command.owner_scope, &command.idempotency_key, mutation_hash.as_deref().unwrap_or_default(), &response, crate::task_memory::now_millis() as i64)
                    .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
                return Ok(response);
            }
            _ => {
                return Err(crate::StorageError::InvalidInput(
                    "unsupported_environment_profile_operation".into(),
                ))
            }
        };
        let response: Vec<u8> = serde_json::to_vec(&result)?;
        if let Some(hash) = mutation_hash {
            evohime_local_storage::execution_environment_profiles_store::save_idempotent_command(
                connection,
                &command.owner_scope,
                &command.idempotency_key,
                &hash,
                &response,
                crate::task_memory::now_millis() as i64,
            )
            .map_err(|e| crate::StorageError::InvalidInput(e.to_string()))?;
        }
        Ok(response)
    }

    /// The activation/current-snapshot write is one SQLite transaction; after
    /// a restart there is therefore no partial switch to replay.  Recovery
    /// only validates durable profiles and leaves an invalid candidate absent
    /// from the active snapshot rather than guessing a replacement.
    pub async fn recover_execution_environment_profiles(
        &self,
    ) -> Result<usize, crate::StorageError> {
        let database = self.database.lock().await;
        let records = evohime_local_storage::execution_environment_profiles_store::load_profiles(
            database.connection(),
            256,
        )
        .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
        Ok(records
            .into_iter()
            .filter_map(|record| {
                serde_json::from_slice::<ExecutionEnvironmentProfile>(&record).ok()
            })
            .filter(|profile| validate_profile(profile).is_ok())
            .count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Resolver(OwnerAvailability);
    impl EnvironmentBindingResolver for Resolver {
        fn resolve(&self, _: &EnvironmentBinding) -> OwnerAvailability {
            self.0.clone()
        }
    }
    fn profile(required: bool, kind: BindingKind) -> ExecutionEnvironmentProfile {
        ExecutionEnvironmentProfile::new(
            "offline".into(),
            1,
            EnvironmentScope::Workspace,
            vec![EnvironmentBinding {
                kind,
                reference: "model.default".into(),
                revision: 1,
                mode: ReferenceMode::PinnedRevision,
                required,
            }],
        )
        .unwrap()
    }
    #[test]
    fn required_failure_is_broken_and_never_activates() {
        let p = profile(true, BindingKind::ModelRouting);
        let check = preflight(&p, &Resolver(OwnerAvailability::Missing));
        assert_eq!(check.state, ProfileState::Broken);
        assert_eq!(
            effective_snapshot(&p, &check),
            Err(EnvironmentProfileError::NotActivatable(
                ProfileState::Broken
            ))
        );
    }
    #[test]
    fn optional_failure_is_explicitly_degraded() {
        let p = profile(false, BindingKind::ModelRouting);
        let check = preflight(&p, &Resolver(OwnerAvailability::UnavailableOwner));
        assert_eq!(check.state, ProfileState::Degraded);
        assert_eq!(
            effective_snapshot(&p, &check).unwrap().state,
            ProfileState::Degraded
        );
    }
    #[test]
    fn backend_requires_new_run_and_hash_is_stable() {
        let p = profile(true, BindingKind::ExecutionBackend);
        let check = preflight(
            &p,
            &Resolver(OwnerAvailability::Available {
                revision: 1,
                compatible: true,
            }),
        );
        assert_eq!(check.boundary, SafeBoundary::NewRunOnly);
        assert_eq!(
            effective_snapshot(&p, &check).unwrap().snapshot_hash,
            effective_snapshot(&p, &check).unwrap().snapshot_hash
        );
    }
    #[test]
    fn forged_content_hash_is_rejected_before_storage() {
        let mut p = profile(true, BindingKind::ModelRouting);
        p.content_hash = "0".repeat(64);
        assert_eq!(
            validate_profile(&p),
            Err(EnvironmentProfileError::InvalidBinding)
        );
    }
    #[test]
    fn pinned_revision_drift_requires_review() {
        let p = profile(true, BindingKind::ModelRouting);
        let check = preflight(
            &p,
            &Resolver(OwnerAvailability::Available {
                revision: 2,
                compatible: true,
            }),
        );
        assert_eq!(check.state, ProfileState::NeedsReview);
        assert!(check
            .diagnostics
            .iter()
            .any(|d| d.code == "pinned_revision_drift"));
    }

    #[test]
    fn profile_json_rejects_unknown_secret_like_fields() {
        let json = br#"{"id":"offline","revision":1,"scope":"workspace","bindings":[{"kind":"model_routing","reference":"model.default","revision":1,"mode":"pinned_revision","required":true}],"content_hash":"forged","api_key":"must-not-be-accepted"}"#;
        assert!(serde_json::from_slice::<ExecutionEnvironmentProfile>(json).is_err());
    }
}
