//! Core-owned, schema-driven configuration contract (plan 67).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_FIELDS: usize = 64;
pub const MAX_OPERATIONS: usize = 32;
pub const MAX_JSON_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfigurationScope {
    ApplicationDefaults,
    WorkspaceDefaults,
    AgentProfile,
    ConversationDefaults,
    RunOverride,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldType {
    Boolean,
    Integer,
    Number,
    String,
    Enum,
    MultiEnum,
    ModelProfileRef,
    BackendRef,
    RoleProfileRef,
    PolicyRef,
    CredentialRef,
    PathRef,
    Object,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegistrySource {
    Models,
    ExecutionBackends,
    ExternalAgents,
    Skills,
    RoleProfiles,
    ContinuationPolicies,
    Credentials,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RestartSemantics {
    Immediate,
    NextTurn,
    NextConversation,
    NextRun,
    CoreRestart,
    AppRestart,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatchKind {
    SetField,
    ClearOverride,
    ResetSection,
    BindReference,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Compatibility {
    Compatible,
    NeedsMigration,
    RemovedField,
    ManualReviewRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationField {
    pub key: String,
    pub field_type: FieldType,
    pub title: String,
    pub default_json: Option<serde_json::Value>,
    pub required: bool,
    pub enum_source: Option<RegistrySource>,
    pub secret: bool,
    pub sensitivity: String,
    pub restart: RestartSemantics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationSchema {
    pub schema_id: String,
    pub version: u32,
    pub scope: ConfigurationScope,
    pub fields: Vec<ConfigurationField>,
    pub compatibility: Compatibility,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationPatch {
    pub kind: PatchKind,
    pub field: String,
    pub value_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationDiagnostic {
    pub field: String,
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationSnapshot {
    pub scope: ConfigurationScope,
    pub schema_id: String,
    pub schema_version: u32,
    pub revision: u64,
    pub values: serde_json::Map<String, serde_json::Value>,
    pub secret_states: serde_json::Map<String, serde_json::Value>,
    pub source_layers: serde_json::Map<String, serde_json::Value>,
    pub effective_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigurationError {
    #[error("unsupported schema version")]
    UnsupportedVersion,
    #[error("configuration input exceeds limit")]
    TooLarge,
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("registry reference is unavailable")]
    UnavailableReference,
    #[error("configuration revision conflict")]
    RevisionConflict,
}

pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, ConfigurationError> {
    let bytes =
        serde_json::to_vec(value).map_err(|e| ConfigurationError::Invalid(e.to_string()))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(ConfigurationError::TooLarge);
    }
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

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

pub fn builtin_schema(scope: ConfigurationScope) -> ConfigurationSchema {
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
    ))
    .expect("builtin schema hash");
    schema
}

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
