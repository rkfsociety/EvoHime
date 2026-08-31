//! Core-owned registry for execution environments.
//!
//! This module deliberately describes remote backends without connecting to or
//! launching them. A remote transport must be introduced by a later,
//! separately reviewed contract. The registry is therefore safe to use for
//! selection and capability negotiation while unknown remote state remains
//! unavailable.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "execution-backend-registry-v1";
pub const MAX_BACKENDS: usize = 64;
pub const MAX_ID_BYTES: usize = 96;
pub const MAX_ENDPOINT_BYTES: usize = 512;
pub const MAX_CAPABILITIES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Registered,
    Probing,
    Healthy,
    Degraded,
    Unavailable,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendFailure {
    InvalidEndpoint,
    UnsupportedProtocol,
    IncompatibleContract,
    CapabilityDenied,
    AuthRefMissing,
    Timeout,
    TransportUnavailable,
    Disabled,
    StaleVersion,
}

impl BackendFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidEndpoint => "invalid_endpoint",
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::IncompatibleContract => "incompatible_contract",
            Self::CapabilityDenied => "capability_denied",
            Self::AuthRefMissing => "auth_ref_missing",
            Self::Timeout => "timeout",
            Self::TransportUnavailable => "transport_unavailable",
            Self::Disabled => "disabled",
            Self::StaleVersion => "stale_version",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendDefinition {
    pub id: String,
    pub kind: BackendKind,
    pub endpoint: Option<String>,
    pub auth_ref: Option<String>,
    pub enabled: bool,
    pub capabilities: Vec<String>,
    pub version: u64,
    pub health: HealthState,
    pub health_failure: Option<BackendFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityHandshake {
    pub protocol_major: u32,
    pub protocol_minor: u32,
    pub backend_id: String,
    pub capabilities: Vec<String>,
    pub capability_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendRunSnapshot {
    pub backend_id: String,
    pub registry_version: u64,
    pub handshake_hash: String,
    pub policy_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Invalid(String),
    Conflict { expected: u64, actual: u64 },
    NotFound,
    Unsupported(BackendFailure),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(v) => write!(f, "invalid_{v}"),
            Self::Conflict { .. } => write!(f, "stale_version"),
            Self::NotFound => write!(f, "not_found"),
            Self::Unsupported(v) => f.write_str(v.code()),
        }
    }
}
impl std::error::Error for RegistryError {}

pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn validate_endpoint(endpoint: &str) -> Result<String, RegistryError> {
    if endpoint.len() > MAX_ENDPOINT_BYTES
        || !endpoint.starts_with("https://")
        || endpoint.contains('@')
        || endpoint.contains('#')
        || endpoint.contains('?')
        || endpoint.contains("..")
    {
        return Err(RegistryError::Unsupported(BackendFailure::InvalidEndpoint));
    }
    let rest = endpoint.strip_prefix("https://").unwrap_or_default();
    let host = rest
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    if host.is_empty()
        || host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
    {
        return Err(RegistryError::Unsupported(BackendFailure::InvalidEndpoint));
    }
    Ok(endpoint.trim_end_matches('/').to_owned())
}

pub fn validate_capabilities(capabilities: &[String]) -> Result<Vec<String>, RegistryError> {
    if capabilities.len() > MAX_CAPABILITIES {
        return Err(RegistryError::Invalid("capabilities_limit".into()));
    }
    let mut result = capabilities.to_vec();
    result.sort();
    result.dedup();
    if result.iter().any(|v| {
        v.is_empty()
            || v.len() > MAX_ID_BYTES
            || !v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    }) {
        return Err(RegistryError::Invalid("capability_id".into()));
    }
    Ok(result)
}

pub fn default_local() -> BackendDefinition {
    BackendDefinition {
        id: "local.core".into(),
        kind: BackendKind::Local,
        endpoint: None,
        auth_ref: None,
        enabled: true,
        capabilities: vec!["agent.execute".into(), "workflow.execute".into()],
        version: 1,
        health: HealthState::Healthy,
        health_failure: None,
    }
}

#[derive(Debug, Clone)]
pub struct Registry {
    entries: BTreeMap<String, BackendDefinition>,
    default_id: String,
    version: u64,
}
impl Default for Registry {
    fn default() -> Self {
        let local = default_local();
        let mut entries = BTreeMap::new();
        entries.insert(local.id.clone(), local);
        Self {
            entries,
            default_id: "local.core".into(),
            version: 1,
        }
    }
}
impl Registry {
    pub fn entries(&self) -> impl Iterator<Item = &BackendDefinition> {
        self.entries.values()
    }
    pub fn default_id(&self) -> &str {
        &self.default_id
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn register(
        &mut self,
        mut backend: BackendDefinition,
        expected_version: u64,
    ) -> Result<(), RegistryError> {
        if expected_version != self.version {
            return Err(RegistryError::Conflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if backend.id.is_empty()
            || backend.id.len() > MAX_ID_BYTES
            || !backend
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
            || self.entries.len() >= MAX_BACKENDS
        {
            return Err(RegistryError::Invalid("backend_id_or_limit".into()));
        }
        backend.capabilities = validate_capabilities(&backend.capabilities)?;
        backend.endpoint = match backend.kind {
            BackendKind::Local => None,
            BackendKind::Remote => {
                Some(validate_endpoint(backend.endpoint.as_deref().ok_or(
                    RegistryError::Unsupported(BackendFailure::InvalidEndpoint),
                )?)?)
            }
        };
        backend.version = self.version + 1;
        backend.health = HealthState::Registered;
        backend.health_failure = None;
        self.entries.insert(backend.id.clone(), backend);
        self.version += 1;
        Ok(())
    }
    pub fn remove(&mut self, id: &str, expected_version: u64) -> Result<(), RegistryError> {
        if expected_version != self.version {
            return Err(RegistryError::Conflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if id == "local.core" {
            return Err(RegistryError::Invalid("local_backend_required".into()));
        }
        if self.entries.remove(id).is_none() {
            return Err(RegistryError::NotFound);
        }
        self.version += 1;
        if self.default_id == id {
            self.default_id = "local.core".into();
        }
        Ok(())
    }
    pub fn set_default(&mut self, id: &str, expected_version: u64) -> Result<(), RegistryError> {
        if expected_version != self.version {
            return Err(RegistryError::Conflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        let entry = self.entries.get(id).ok_or(RegistryError::NotFound)?;
        if !entry.enabled || entry.health == HealthState::Disabled {
            return Err(RegistryError::Unsupported(BackendFailure::Disabled));
        }
        self.default_id = id.into();
        self.version += 1;
        Ok(())
    }
    pub fn handshake(
        &self,
        id: &str,
        advertised: CapabilityHandshake,
        policy_capabilities: &[String],
    ) -> Result<BackendRunSnapshot, RegistryError> {
        let backend = self.entries.get(id).ok_or(RegistryError::NotFound)?;
        if !backend.enabled {
            return Err(RegistryError::Unsupported(BackendFailure::Disabled));
        }
        if advertised.protocol_major != CONTRACT_VERSION || advertised.backend_id != id {
            return Err(RegistryError::Unsupported(
                BackendFailure::IncompatibleContract,
            ));
        }
        let caps = validate_capabilities(&advertised.capabilities)?;
        if caps
            .iter()
            .any(|v| !backend.capabilities.contains(v) || !policy_capabilities.contains(v))
        {
            return Err(RegistryError::Unsupported(BackendFailure::CapabilityDenied));
        }
        let hash =
            canonical_hash(&advertised).map_err(|e| RegistryError::Invalid(e.to_string()))?;
        if matches!(backend.kind, BackendKind::Remote) {
            return Err(RegistryError::Unsupported(
                BackendFailure::TransportUnavailable,
            ));
        }
        Ok(BackendRunSnapshot {
            backend_id: id.into(),
            registry_version: self.version,
            handshake_hash: hash,
            policy_hash: canonical_hash(&policy_capabilities).unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_handshake_is_pinned_and_policy_bounded() {
        let registry = Registry::default();
        let hs = CapabilityHandshake {
            protocol_major: 1,
            protocol_minor: 0,
            backend_id: "local.core".into(),
            capabilities: vec!["agent.execute".into()],
            capability_hash: "advisory".into(),
        };
        let snap = registry
            .handshake("local.core", hs, &["agent.execute".into()])
            .unwrap();
        assert_eq!(snap.backend_id, "local.core");
        assert_eq!(snap.registry_version, 1);
    }
    #[test]
    fn remote_never_becomes_implicit_success() {
        let mut r = Registry::default();
        r.register(
            BackendDefinition {
                id: "remote.host".into(),
                kind: BackendKind::Remote,
                endpoint: Some("https://executor.example".into()),
                auth_ref: Some("cred:slot".into()),
                enabled: true,
                capabilities: vec!["agent.execute".into()],
                version: 0,
                health: HealthState::Registered,
                health_failure: None,
            },
            1,
        )
        .unwrap();
        let hs = CapabilityHandshake {
            protocol_major: 1,
            protocol_minor: 0,
            backend_id: "remote.host".into(),
            capabilities: vec!["agent.execute".into()],
            capability_hash: "x".into(),
        };
        assert_eq!(
            r.handshake("remote.host", hs, &["agent.execute".into()])
                .unwrap_err(),
            RegistryError::Unsupported(BackendFailure::TransportUnavailable)
        );
    }
    #[test]
    fn invalid_endpoint_and_stale_mutation_fail_closed() {
        assert!(validate_endpoint("http://executor.example").is_err());
        let mut r = Registry::default();
        let b = BackendDefinition {
            id: "remote.x".into(),
            kind: BackendKind::Remote,
            endpoint: Some("https://executor.example".into()),
            auth_ref: None,
            enabled: true,
            capabilities: vec![],
            version: 0,
            health: HealthState::Registered,
            health_failure: None,
        };
        assert!(matches!(
            r.register(b.clone(), 0),
            Err(RegistryError::Conflict { .. })
        ));
        assert!(r.register(b, 1).is_ok());
    }
}
