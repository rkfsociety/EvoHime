//! Core-owned, data-only runtime component configurations.
//!
//! The provider identity is resolved through the registry from plan 74.  This
//! module stores no executable identity, secret value, process memory, or
//! capability grant; it only records typed references and an immutable
//! definition/state snapshot.
use crate::declarative_agent_component_registry::{ComponentType, Registry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID: usize = 128;
pub const MAX_DEFINITION_BYTES: usize = 64 * 1024;
pub const MAX_SECRET_BINDINGS: usize = 64;
pub const MAX_CAPABILITIES: usize = 128;
pub const MAX_PROVENANCE: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretBinding {
    pub name: String,
    pub credential_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeState {
    Defined,
    Starting,
    Ready,
    Failed,
    UnknownOutcome,
    Reconciliation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentConfig {
    pub schema_version: u32,
    pub component_id: String,
    pub component_type: ComponentType,
    pub provider_id: String,
    pub provider_version: u32,
    pub definition_config: Value,
    pub runtime_state: RuntimeState,
    pub secret_bindings: Vec<SecretBinding>,
    pub capability_refs: Vec<String>,
    pub policy_version: String,
    pub provenance_ref: String,
    pub revision: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySnapshot {
    pub policy_version: String,
    pub allowed_capabilities: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ComponentError {
    #[error("invalid component config: {0}")]
    Invalid(&'static str),
    #[error("unsupported component schema version")]
    UnsupportedVersion,
    #[error("provider is not registered for this component")]
    UnknownProvider,
    #[error("component policy snapshot is stale or capability is denied")]
    PolicyDenied,
    #[error("secret bindings must contain credential references only")]
    SecretValue,
    #[error("migration is unavailable")]
    MissingMigration,
}

fn bounded(v: &str, max: usize) -> bool {
    !v.is_empty() && v.len() <= max && !v.chars().any(char::is_control)
}

pub fn canonical_hash(config: &ComponentConfig) -> Result<String, ComponentError> {
    let mut copy = config.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| ComponentError::Invalid("serialization"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

pub fn validate(config: &ComponentConfig, providers: &Registry) -> Result<(), ComponentError> {
    if config.schema_version != SCHEMA_VERSION {
        return Err(ComponentError::UnsupportedVersion);
    }
    if !bounded(&config.component_id, MAX_ID)
        || !bounded(&config.provider_id, MAX_ID)
        || !bounded(&config.policy_version, MAX_ID)
        || !bounded(&config.provenance_ref, MAX_PROVENANCE)
        || config.revision == 0
    {
        return Err(ComponentError::Invalid("identity or revision"));
    }
    if config.capability_refs.len() > MAX_CAPABILITIES
        || config.secret_bindings.len() > MAX_SECRET_BINDINGS
        || serde_json::to_vec(&config.definition_config)
            .map_err(|_| ComponentError::Invalid("definition"))?
            .len()
            > MAX_DEFINITION_BYTES
    {
        return Err(ComponentError::Invalid("bounds"));
    }
    if config
        .secret_bindings
        .iter()
        .any(|b| !bounded(&b.name, MAX_ID) || !bounded(&b.credential_ref, MAX_ID))
    {
        return Err(ComponentError::SecretValue);
    }
    let provider = providers
        .providers
        .get(&config.provider_id)
        .ok_or(ComponentError::UnknownProvider)?;
    if provider.component_type != config.component_type
        || provider.current_version < config.provider_version
        || config.provider_version == 0
    {
        return Err(ComponentError::UnknownProvider);
    }
    if config.content_hash != canonical_hash(config)? {
        return Err(ComponentError::Invalid("content hash"));
    }
    Ok(())
}

pub fn rehydrate(
    config: &ComponentConfig,
    providers: &Registry,
    current: &PolicySnapshot,
) -> Result<(), ComponentError> {
    validate(config, providers)?;
    if config.policy_version != current.policy_version
        || config
            .capability_refs
            .iter()
            .any(|c| !current.allowed_capabilities.contains(c))
    {
        return Err(ComponentError::PolicyDenied);
    }
    Ok(())
}

pub fn migrate_json(input: Value, from: u32, to: u32) -> Result<Value, ComponentError> {
    if from == to {
        return Ok(input);
    }
    if from == 0 && to == 1 {
        let mut obj = input
            .as_object()
            .cloned()
            .ok_or(ComponentError::MissingMigration)?;
        obj.insert("schema_version".into(), Value::from(1));
        obj.entry("runtime_state")
            .or_insert_with(|| Value::String("Defined".into()));
        obj.entry("secret_bindings")
            .or_insert_with(|| Value::Array(Vec::new()));
        obj.entry("capability_refs")
            .or_insert_with(|| Value::Array(Vec::new()));
        return Ok(Value::Object(obj));
    }
    Err(ComponentError::MissingMigration)
}

pub fn validate_transition(from: &RuntimeState, to: &RuntimeState) -> Result<(), ComponentError> {
    let allowed = matches!(
        (from, to),
        (RuntimeState::Defined, RuntimeState::Starting)
            | (RuntimeState::Starting, RuntimeState::Ready)
            | (RuntimeState::Starting, RuntimeState::Failed)
            | (RuntimeState::Starting, RuntimeState::UnknownOutcome)
            | (RuntimeState::UnknownOutcome, RuntimeState::Reconciliation)
            | (RuntimeState::Reconciliation, RuntimeState::Ready)
            | (RuntimeState::Reconciliation, RuntimeState::Failed)
    );
    if allowed {
        Ok(())
    } else {
        Err(ComponentError::Invalid("state transition"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative_agent_component_registry::{ComponentProvider, TrustStatus};
    use std::collections::BTreeMap;
    fn fixture() -> (ComponentConfig, Registry) {
        let mut providers = BTreeMap::new();
        providers.insert(
            "p".into(),
            ComponentProvider {
                provider_id: "p".into(),
                component_type: ComponentType::AgentRole,
                current_version: 1,
                trust: TrustStatus::BuiltInTrusted,
                schema_hash: "s".into(),
            },
        );
        let r = Registry {
            schema_version: 1,
            revision: 1,
            providers,
            components: BTreeMap::new(),
            content_hash: "h".into(),
        };
        let mut c = ComponentConfig {
            schema_version: 1,
            component_id: "c".into(),
            component_type: ComponentType::AgentRole,
            provider_id: "p".into(),
            provider_version: 1,
            definition_config: serde_json::json!({"mode":"safe"}),
            runtime_state: RuntimeState::Defined,
            secret_bindings: vec![SecretBinding {
                name: "key".into(),
                credential_ref: "cred-1".into(),
            }],
            capability_refs: vec!["read".into()],
            policy_version: "v1".into(),
            provenance_ref: "event-1".into(),
            revision: 1,
            content_hash: String::new(),
        };
        c.content_hash = canonical_hash(&c).unwrap();
        (c, r)
    }
    #[test]
    fn envelope_is_valid_and_hash_is_deterministic() {
        let (c, r) = fixture();
        assert!(validate(&c, &r).is_ok());
        assert_eq!(canonical_hash(&c).unwrap(), c.content_hash);
    }
    #[test]
    fn rehydration_fails_closed_on_policy_change() {
        let (c, r) = fixture();
        let p = PolicySnapshot {
            policy_version: "v2".into(),
            allowed_capabilities: ["read".into()].into_iter().collect(),
        };
        assert_eq!(rehydrate(&c, &r, &p), Err(ComponentError::PolicyDenied));
    }
    #[test]
    fn migration_and_unknown_transition_are_typed() {
        let migrated = migrate_json(serde_json::json!({"component_id":"c"}), 0, 1).unwrap();
        assert_eq!(migrated["schema_version"], 1);
        assert!(validate_transition(&RuntimeState::Defined, &RuntimeState::Ready).is_err());
    }
}
