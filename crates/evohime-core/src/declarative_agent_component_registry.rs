//! Core-owned declarative runtime component registry (plan 74).
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PROVIDERS: usize = 512;
pub const MAX_COMPONENTS: usize = 4096;
pub const MAX_ID: usize = 128;
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
pub const MAX_REFS: usize = 256;
pub const MAX_DUMP_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentType {
    AgentRole,
    TeamCoordinationPolicy,
    TerminationCondition,
    ContextPolicy,
    ModelProfile,
    Workbench,
    OutputContract,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrustStatus {
    BuiltInTrusted,
    ExplicitlyTrusted,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentProvider {
    pub provider_id: String,
    pub component_type: ComponentType,
    pub current_version: u32,
    pub trust: TrustStatus,
    pub schema_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentRef {
    pub provider_id: String,
    pub component_version: u32,
    pub instance_config_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentDescriptor {
    pub provider_id: String,
    pub component_type: ComponentType,
    pub spec_version: u32,
    pub component_version: u32,
    pub label: String,
    pub description: Option<String>,
    pub config: Value,
    pub references: Vec<ComponentRef>,
    pub capability_refs: Vec<String>,
    pub credential_refs: Vec<String>,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Registry {
    pub schema_version: u32,
    pub revision: u64,
    pub providers: BTreeMap<String, ComponentProvider>,
    pub components: BTreeMap<String, ComponentDescriptor>,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("invalid registry value: {0}")]
    Invalid(String),
    #[error("unknown provider")]
    UnknownProvider,
    #[error("provider is not trusted")]
    UntrustedProvider,
    #[error("schema version is unsupported")]
    UnsupportedVersion,
    #[error("migration is unavailable")]
    MissingMigration,
    #[error("dependency cycle")]
    DependencyCycle,
    #[error("secret value must use a credential reference")]
    SecretValue,
}
fn bounded(v: &str, n: usize) -> Result<(), RegistryError> {
    if v.is_empty() || v.len() > n || v.chars().any(char::is_control) {
        Err(RegistryError::Invalid("bounded text".into()))
    } else {
        Ok(())
    }
}
pub fn hash<T: Serialize>(v: &T) -> String {
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap_or_default()))
    )
}
fn secret_key(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    ["secret", "password", "token", "private_key", "api_key"]
        .iter()
        .any(|x| k.contains(x))
}
fn reject_secrets(v: &Value) -> Result<(), RegistryError> {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                if secret_key(k) && !k.ends_with("_ref") {
                    return Err(RegistryError::SecretValue);
                }
                reject_secrets(v)?
            }
        }
        Value::Array(a) => {
            for v in a {
                reject_secrets(v)?
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn validate_descriptor(d: &ComponentDescriptor, r: &Registry) -> Result<(), RegistryError> {
    bounded(&d.provider_id, MAX_ID)?;
    bounded(&d.label, MAX_ID)?;
    if let Some(v) = &d.description {
        bounded(v, MAX_ID)?;
    }
    if d.spec_version != SCHEMA_VERSION {
        return Err(RegistryError::UnsupportedVersion);
    }
    if d.references.len() > MAX_REFS
        || d.capability_refs.len() > MAX_REFS
        || d.credential_refs.len() > MAX_REFS
    {
        return Err(RegistryError::Invalid("reference bounds".into()));
    }
    if serde_json::to_vec(&d.config)
        .map_err(|e| RegistryError::Invalid(e.to_string()))?
        .len()
        > MAX_CONFIG_BYTES
    {
        return Err(RegistryError::Invalid("config bound".into()));
    }
    reject_secrets(&d.config)?;
    let p = r
        .providers
        .get(&d.provider_id)
        .ok_or(RegistryError::UnknownProvider)?;
    if !matches!(
        p.trust,
        TrustStatus::BuiltInTrusted | TrustStatus::ExplicitlyTrusted
    ) {
        return Err(RegistryError::UntrustedProvider);
    }
    if p.component_type != d.component_type
        || d.component_version == 0
        || d.component_version > p.current_version
    {
        return Err(RegistryError::Invalid("provider type/version".into()));
    }
    Ok(())
}
pub fn validate_registry(r: &Registry) -> Result<(), RegistryError> {
    if r.schema_version != SCHEMA_VERSION
        || r.providers.len() > MAX_PROVIDERS
        || r.components.len() > MAX_COMPONENTS
    {
        return Err(RegistryError::Invalid("registry bounds/version".into()));
    }
    for (id, p) in &r.providers {
        bounded(id, MAX_ID)?;
        bounded(&p.provider_id, MAX_ID)?;
        if id != &p.provider_id || p.current_version == 0 {
            return Err(RegistryError::Invalid("provider identity".into()));
        }
    }
    for (id, d) in &r.components {
        bounded(id, MAX_ID)?;
        validate_descriptor(d, r)?;
    }
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, d) in &r.components {
        for dep in &d.references {
            if !r.components.contains_key(&dep.instance_config_ref) {
                return Err(RegistryError::Invalid("unknown component ref".into()));
            }
            edges
                .entry(id)
                .or_default()
                .push(dep.instance_config_ref.as_str());
        }
    }
    fn visit<'a>(
        id: &'a str,
        e: &BTreeMap<&'a str, Vec<&'a str>>,
        visiting: &mut BTreeSet<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Result<(), RegistryError> {
        if visiting.contains(id) {
            return Err(RegistryError::DependencyCycle);
        }
        if done.contains(id) {
            return Ok(());
        }
        visiting.insert(id);
        for next in e.get(id).into_iter().flatten() {
            visit(next, e, visiting, done)?;
        }
        visiting.remove(id);
        done.insert(id);
        Ok(())
    }
    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    for id in r.components.keys() {
        visit(id, &edges, &mut visiting, &mut done)?;
    }
    Ok(())
}
pub fn migrate(d: &ComponentDescriptor, to: u32) -> Result<ComponentDescriptor, RegistryError> {
    if d.component_version == to {
        return Ok(d.clone());
    }
    if d.component_version + 1 == to {
        let mut n = d.clone();
        n.component_version = to;
        n.content_hash = hash(&n);
        return Ok(n);
    }
    Err(RegistryError::MissingMigration)
}
pub fn diff(
    a: &ComponentDescriptor,
    b: &ComponentDescriptor,
) -> Result<Vec<String>, RegistryError> {
    let mut out = Vec::new();
    if a.provider_id != b.provider_id {
        out.push("provider_id".into());
    }
    if a.component_version != b.component_version {
        out.push("component_version".into());
    }
    if a.config != b.config {
        out.push("config".into());
    }
    if a.references != b.references {
        out.push("references".into());
    }
    Ok(out)
}
pub fn dump(d: &ComponentDescriptor) -> Result<String, RegistryError> {
    let s = serde_json::to_string_pretty(d).map_err(|e| RegistryError::Invalid(e.to_string()))?;
    if s.len() > MAX_DUMP_BYTES {
        Err(RegistryError::Invalid("dump bound".into()))
    } else {
        Ok(s)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn registry() -> Registry {
        let p = ComponentProvider {
            provider_id: "evohime.agent.role/v1".into(),
            component_type: ComponentType::AgentRole,
            current_version: 2,
            trust: TrustStatus::BuiltInTrusted,
            schema_hash: "sha256:schema".into(),
        };
        Registry {
            schema_version: 1,
            revision: 1,
            providers: BTreeMap::from([(p.provider_id.clone(), p)]),
            components: BTreeMap::new(),
            content_hash: String::new(),
        }
    }
    fn descriptor() -> ComponentDescriptor {
        ComponentDescriptor {
            provider_id: "evohime.agent.role/v1".into(),
            component_type: ComponentType::AgentRole,
            spec_version: 1,
            component_version: 1,
            label: "role".into(),
            description: None,
            config: serde_json::json!({"mode":"safe"}),
            references: vec![],
            capability_refs: vec![],
            credential_refs: vec![],
            content_hash: String::new(),
        }
    }
    #[test]
    fn validates_trusted_and_migrates() {
        let r = registry();
        let d = descriptor();
        assert!(validate_descriptor(&d, &r).is_ok());
        assert_eq!(migrate(&d, 2).unwrap().component_version, 2);
    }
    #[test]
    fn rejects_unknown_and_secret() {
        let r = registry();
        let mut d = descriptor();
        d.provider_id = "unknown".into();
        assert_eq!(
            validate_descriptor(&d, &r),
            Err(RegistryError::UnknownProvider)
        );
        let mut d = descriptor();
        d.config = serde_json::json!({"api_token":"raw"});
        assert_eq!(validate_descriptor(&d, &r), Err(RegistryError::SecretValue));
    }
}
