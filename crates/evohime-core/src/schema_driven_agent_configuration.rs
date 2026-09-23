//! Core-owned, schema-driven configuration contract (plan 67).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized schema version accepted by the agent configuration API.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum configurable fields in one schema.
pub const MAX_FIELDS: usize = 64;
/// Maximum configuration patches validated in one operation.
pub const MAX_OPERATIONS: usize = 32;
/// Maximum serialized JSON input or schema size in bytes.
pub const MAX_JSON_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Application and execution layer where configuration applies.
pub enum ConfigurationScope {
    /// Lowest-level defaults shared by the application.
    ApplicationDefaults,
    /// Defaults that apply to one workspace.
    WorkspaceDefaults,
    /// Configuration stored with one agent profile.
    AgentProfile,
    /// Defaults applied to a conversation.
    ConversationDefaults,
    /// Values supplied only for one execution run.
    RunOverride,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Value or reference type accepted for a configuration field.
pub enum FieldType {
    /// Boolean JSON value.
    Boolean,
    /// Integer JSON number.
    Integer,
    /// Finite numeric JSON value.
    Number,
    /// String JSON value.
    String,
    /// One value from a declared option set.
    Enum,
    /// A list of values from a declared option set.
    MultiEnum,
    /// Reference to a model profile registry entry.
    ModelProfileRef,
    /// Reference to an execution backend registry entry.
    BackendRef,
    /// Reference to an agent role profile.
    RoleProfileRef,
    /// Reference to a policy registry entry.
    PolicyRef,
    /// Reference to a protected credential binding.
    CredentialRef,
    /// Reference to a path validated by its owning subsystem.
    PathRef,
    /// JSON object validated against the field schema.
    Object,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Authoritative registry that supplies selectable reference values.
pub enum RegistrySource {
    /// Model profile registry.
    Models,
    /// Execution backend registry.
    ExecutionBackends,
    /// External agent preset registry.
    ExternalAgents,
    /// Skill registry.
    Skills,
    /// Agent role profile registry.
    RoleProfiles,
    /// Run continuation policy registry.
    ContinuationPolicies,
    /// Credential binding registry.
    Credentials,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Lifecycle boundary required for a configuration change to take effect.
pub enum RestartSemantics {
    /// Apply without waiting for a lifecycle boundary.
    Immediate,
    /// Apply at the next turn boundary.
    NextTurn,
    /// Apply to a newly created conversation.
    NextConversation,
    /// Apply to a newly started run.
    NextRun,
    /// Apply after the Core process restarts.
    CoreRestart,
    /// Apply after the desktop application restarts.
    AppRestart,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Mutation operation applied to a configuration field or section.
pub enum PatchKind {
    /// Set a field value.
    SetField,
    /// Remove an override and reveal the next value layer.
    ClearOverride,
    /// Reset a section to its defaults.
    ResetSection,
    /// Bind a field to a registry-backed resource.
    BindReference,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Severity assigned to a patch diagnostic.
pub enum DiagnosticSeverity {
    /// Value is accepted with a non-blocking diagnostic.
    Warning,
    /// Value is rejected as invalid.
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
/// Migration or review state for a configuration schema.
pub enum Compatibility {
    /// Schema can be consumed without migration.
    Compatible,
    /// Existing values require migration to this schema.
    NeedsMigration,
    /// The schema removes a field used by existing configuration.
    RemovedField,
    /// Compatibility needs an explicit owner decision.
    ManualReviewRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Typed, sensitivity-aware definition of one configuration value.
pub struct ConfigurationField {
    /// Stable field identifier used in configuration layers.
    pub key: String,
    /// Data or reference type accepted for this field.
    pub field_type: FieldType,
    /// Human-readable field label.
    pub title: String,
    /// Optional non-secret default value encoded as JSON.
    pub default_json: Option<serde_json::Value>,
    /// Whether the effective configuration must provide this value.
    pub required: bool,
    /// Registry used to validate selectable references or enum values.
    pub enum_source: Option<RegistrySource>,
    /// Whether the field value must be excluded from ordinary snapshots.
    pub secret: bool,
    /// Sensitivity classification used by UI and storage boundaries.
    pub sensitivity: String,
    /// Earliest execution lifecycle boundary for applying changes.
    pub restart: RestartSemantics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Versioned schema describing valid configuration fields and scope.
pub struct ConfigurationSchema {
    /// Stable identifier for this configuration schema.
    pub schema_id: String,
    /// Schema revision checked against the supported version.
    pub version: u32,
    /// Configuration scope governed by the schema.
    pub scope: ConfigurationScope,
    /// Field definitions accepted by this schema.
    pub fields: Vec<ConfigurationField>,
    /// Compatibility status of this schema revision.
    pub compatibility: Compatibility,
    /// Integrity hash over the canonical schema content.
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// One field-level operation submitted for validation.
pub struct ConfigurationPatch {
    /// Mutation type to apply to a configuration field.
    pub kind: PatchKind,
    /// Stable field key targeted by this patch.
    pub field: String,
    /// Optional serialized value for set or bind operations.
    pub value_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Non-fatal diagnostic produced while validating patches.
pub struct ConfigurationDiagnostic {
    /// Stable field key targeted by this patch.
    pub field: String,
    /// Warning or error classification for this diagnostic.
    pub severity: DiagnosticSeverity,
    /// Stable machine-readable diagnostic code.
    pub code: String,
    /// Human-readable explanation of the diagnostic.
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Resolved effective values, secret flags, and source layers.
pub struct ConfigurationSnapshot {
    /// Configuration scope governed by the schema.
    pub scope: ConfigurationScope,
    /// Stable identifier for this configuration schema.
    pub schema_id: String,
    /// Schema revision used to resolve this snapshot.
    pub schema_version: u32,
    /// Configuration revision used to build this effective snapshot.
    pub revision: u64,
    /// Resolved non-secret configuration values.
    pub values: serde_json::Map<String, serde_json::Value>,
    /// Presence-only state for secret configuration fields.
    pub secret_states: serde_json::Map<String, serde_json::Value>,
    /// Configuration layer that supplied each effective value.
    pub source_layers: serde_json::Map<String, serde_json::Value>,
    /// Integrity hash of the resolved effective snapshot.
    pub effective_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
/// Invalid schema, oversized input, unavailable reference, or revision conflict.
pub enum ConfigurationError {
    /// The schema version is not supported.
    #[error("unsupported schema version")]
    UnsupportedVersion,
    /// Serialized configuration exceeds its size limit.
    #[error("configuration input exceeds limit")]
    TooLarge,
    /// A field or configuration invariant is invalid.
    #[error("invalid configuration: {0}")]
    Invalid(String),
    /// A required registry reference cannot be resolved.
    #[error("registry reference is unavailable")]
    UnavailableReference,
    /// The current configuration revision differs from the expected revision.
    #[error("configuration revision conflict")]
    RevisionConflict,
}

/// Returns a size-bounded SHA-256 hash of a serialized configuration value.
pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, ConfigurationError> {
    let bytes =
        serde_json::to_vec(value).map_err(|e| ConfigurationError::Invalid(e.to_string()))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(ConfigurationError::TooLarge);
    }
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

/// Checks schema version, field bounds, secret defaults, and registry requirements.
pub fn validate_schema(schema: &ConfigurationSchema) -> Result<(), ConfigurationError> {
    if schema.version != SCHEMA_VERSION {
        return Err(ConfigurationError::UnsupportedVersion);
    }
    if schema.schema_id.is_empty() || schema.fields.len() > MAX_FIELDS {
        return Err(ConfigurationError::Invalid("schema bounds".into()));
    }
    let mut keys = std::collections::HashSet::new();
    for field in &schema.fields {
        if field.key.is_empty() || field.key.len() > 128 || !keys.insert(field.key.clone()) {
            return Err(ConfigurationError::Invalid("field key".into()));
        }
        if field.secret && field.default_json.is_some() {
            return Err(ConfigurationError::Invalid("secret default".into()));
        }
        if matches!(
            field.field_type,
            FieldType::ModelProfileRef
                | FieldType::BackendRef
                | FieldType::RoleProfileRef
                | FieldType::PolicyRef
                | FieldType::CredentialRef
        ) && field.enum_source.is_none()
        {
            return Err(ConfigurationError::Invalid(
                "registry source required".into(),
            ));
        }
    }
    let expected = canonical_hash(&(
        &schema.schema_id,
        schema.version,
        schema.scope,
        &schema.fields,
    ))?;
    if !schema.content_hash.is_empty() && schema.content_hash != expected {
        return Err(ConfigurationError::Invalid("content hash".into()));
    }
    Ok(())
}

/// Validates patch targets and bounds without exposing secret values.
pub fn validate_patches(
    schema: &ConfigurationSchema,
    patches: &[ConfigurationPatch],
) -> Result<Vec<ConfigurationDiagnostic>, ConfigurationError> {
    validate_schema(schema)?;
    if patches.len() > MAX_OPERATIONS {
        return Err(ConfigurationError::TooLarge);
    }
    let fields = schema
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f))
        .collect::<std::collections::HashMap<_, _>>();
    let mut diagnostics = Vec::new();
    for patch in patches {
        let Some(field) = fields.get(patch.field.as_str()) else {
            return Err(ConfigurationError::Invalid("unknown field".into()));
        };
        if matches!(patch.kind, PatchKind::BindReference) && field.enum_source.is_none() {
            return Err(ConfigurationError::UnavailableReference);
        }
        if field.secret && patch.value_json.is_some() {
            diagnostics.push(ConfigurationDiagnostic {
                field: field.key.clone(),
                severity: DiagnosticSeverity::Warning,
                code: "secret_redacted".into(),
                message: "значение применяется через защищённый credential-контур".into(),
            });
        }
        if patch.value_json.as_ref().is_some_and(|v| {
            serde_json::to_vec(v)
                .map(|b| b.len() > MAX_JSON_BYTES)
                .unwrap_or(true)
        }) {
            return Err(ConfigurationError::TooLarge);
        }
    }
    Ok(diagnostics)
}

/// Returns the application-owned schema for the requested configuration scope.
pub fn builtin_schema(
    scope: ConfigurationScope,
) -> Result<ConfigurationSchema, ConfigurationError> {
    let fields = vec![
        ConfigurationField {
            key: "model_profile".into(),
            field_type: FieldType::ModelProfileRef,
            title: "Профиль модели".into(),
            default_json: None,
            required: false,
            enum_source: Some(RegistrySource::Models),
            secret: false,
            sensitivity: "non_sensitive".into(),
            restart: RestartSemantics::NextRun,
        },
        ConfigurationField {
            key: "backend".into(),
            field_type: FieldType::BackendRef,
            title: "Backend".into(),
            default_json: None,
            required: false,
            enum_source: Some(RegistrySource::ExecutionBackends),
            secret: false,
            sensitivity: "non_sensitive".into(),
            restart: RestartSemantics::NextConversation,
        },
        ConfigurationField {
            key: "reasoning_effort".into(),
            field_type: FieldType::Enum,
            title: "Глубина рассуждения".into(),
            default_json: Some(serde_json::json!("auto")),
            required: false,
            enum_source: None,
            secret: false,
            sensitivity: "non_sensitive".into(),
            restart: RestartSemantics::NextTurn,
        },
        ConfigurationField {
            key: "provider_credential".into(),
            field_type: FieldType::CredentialRef,
            title: "Учётные данные провайдера".into(),
            default_json: None,
            required: false,
            enum_source: Some(RegistrySource::Credentials),
            secret: true,
            sensitivity: "secret".into(),
            restart: RestartSemantics::CoreRestart,
        },
    ];
    let mut schema = ConfigurationSchema {
        schema_id: "evohime.agent-configuration".into(),
        version: SCHEMA_VERSION,
        scope,
        fields,
        compatibility: Compatibility::Compatible,
        content_hash: String::new(),
    };
    schema.content_hash = canonical_hash(&(
        &schema.schema_id,
        schema.version,
        schema.scope,
        &schema.fields,
    ))?;
    Ok(schema)
}

/// Merges schema defaults and supplied layers while redacting secret values.
pub fn effective_snapshot(
    scope: ConfigurationScope,
    schema: &ConfigurationSchema,
    revision: u64,
    layers: &[(&str, &serde_json::Map<String, serde_json::Value>)],
) -> Result<ConfigurationSnapshot, ConfigurationError> {
    validate_schema(schema)?;
    let mut values = serde_json::Map::new();
    let mut source_layers = serde_json::Map::new();
    let mut secret_states = serde_json::Map::new();
    for field in &schema.fields {
        if let Some(default) = &field.default_json {
            values.insert(field.key.clone(), default.clone());
            source_layers.insert(
                field.key.clone(),
                serde_json::Value::String("schema_default".into()),
            );
        }
        if field.secret {
            secret_states.insert(field.key.clone(), serde_json::json!({"configured": false}));
        }
    }
    for (layer, map) in layers {
        for (key, value) in *map {
            if let Some(field) = schema.fields.iter().find(|f| f.key == *key) {
                if !field.secret {
                    values.insert(key.clone(), value.clone());
                } else {
                    secret_states.insert(key.clone(), serde_json::json!({"configured": true}));
                }
                source_layers.insert(key.clone(), serde_json::Value::String((*layer).into()));
            }
        }
    }
    let effective_hash = canonical_hash(&(
        &schema.schema_id,
        schema.version,
        &values,
        &secret_states,
        &source_layers,
    ))?;
    Ok(ConfigurationSnapshot {
        scope,
        schema_id: schema.schema_id.clone(),
        schema_version: schema.version,
        revision,
        values,
        secret_states,
        source_layers,
        effective_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> ConfigurationSchema {
        let mut s = ConfigurationSchema {
            schema_id: "agent.settings".into(),
            version: 1,
            scope: ConfigurationScope::ApplicationDefaults,
            fields: vec![ConfigurationField {
                key: "model".into(),
                field_type: FieldType::ModelProfileRef,
                title: "Model".into(),
                default_json: None,
                required: true,
                enum_source: Some(RegistrySource::Models),
                secret: false,
                sensitivity: "non_sensitive".into(),
                restart: RestartSemantics::NextRun,
            }],
            compatibility: Compatibility::Compatible,
            content_hash: String::new(),
        };
        s.content_hash = canonical_hash(&(&s.schema_id, s.version, s.scope, &s.fields)).unwrap();
        s
    }
    #[test]
    fn version_and_registry_are_enforced() {
        let mut s = schema();
        s.version = 2;
        assert_eq!(
            validate_schema(&s),
            Err(ConfigurationError::UnsupportedVersion)
        );
        let mut s = schema();
        s.fields[0].enum_source = None;
        assert!(validate_schema(&s).is_err());
    }
    #[test]
    fn effective_hash_and_secret_redaction_are_deterministic() {
        let mut s = schema();
        s.fields.push(ConfigurationField {
            key: "token".into(),
            field_type: FieldType::CredentialRef,
            title: "Token".into(),
            default_json: None,
            required: false,
            enum_source: Some(RegistrySource::Credentials),
            secret: true,
            sensitivity: "secret".into(),
            restart: RestartSemantics::CoreRestart,
        });
        let hash = canonical_hash(&(&s.schema_id, s.version, s.scope, &s.fields)).unwrap();
        s.content_hash = hash;
        let mut m = serde_json::Map::new();
        m.insert("model".into(), serde_json::json!("trusted"));
        m.insert("token".into(), serde_json::json!("plaintext"));
        let snap = effective_snapshot(s.scope, &s, 1, &[("workspace", &m)]).unwrap();
        assert!(!snap.values.contains_key("token"));
        assert_eq!(snap.secret_states["token"]["configured"], true);
        assert_eq!(
            snap.effective_hash,
            effective_snapshot(s.scope, &s, 1, &[("workspace", &m)])
                .unwrap()
                .effective_hash
        );
    }
    #[test]
    fn executable_like_unknown_fields_are_rejected() {
        let s = schema();
        let p = ConfigurationPatch {
            kind: PatchKind::SetField,
            field: "on_execute".into(),
            value_json: Some(serde_json::json!("run")),
        };
        assert!(validate_patches(&s, &[p]).is_err());
    }
}
