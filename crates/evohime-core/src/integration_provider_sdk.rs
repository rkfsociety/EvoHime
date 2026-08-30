//! Core-owned, metadata-only Integration Provider SDK contract (plan 33).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const SDK_SCHEMA_VERSION: u32 = 1;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_SCHEMA_DEPTH: usize = 8;
pub const MAX_SCHEMA_PROPERTIES: usize = 64;
pub const MAX_SCHEMA_ITEMS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    OAuth2,
    ApiKey,
    UserPassword,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    ReadOnly,
    Write,
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    Connected,
    Expired,
    Invalid,
    Revoked,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationActionV1 {
    pub id: String,
    pub version: u32,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    #[serde(default)]
    pub required_scopes: Vec<String>,
    pub risk_class: RiskClass,
    pub side_effect_class: String,
    pub idempotency_support: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationTriggerV1 {
    pub id: String,
    pub event_schema: Value,
    #[serde(default)]
    pub required_scopes: Vec<String>,
    pub subscription_capability: String,
    pub validation_strategy: String,
    pub delivery_security: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationProviderManifestV1 {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub display_name: String,
    pub description: String,
    pub auth_methods: Vec<AuthMethod>,
    pub actions: Vec<IntegrationActionV1>,
    #[serde(default)]
    pub triggers: Vec<IntegrationTriggerV1>,
    pub credential_schema: Value,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialRefV1 {
    pub id: String,
    pub provider_id: String,
    pub auth_kind: AuthMethod,
    #[serde(default)]
    pub granted_scopes: BTreeSet<String>,
    #[serde(default)]
    pub account_label: Option<String>,
    pub status: CredentialStatus,
    pub created_at_ms: i64,
    #[serde(default)]
    pub last_verified_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowIntegrationBindingV1 {
    pub node_id: String,
    pub provider_id: String,
    pub action_id: String,
    pub action_version: u32,
    pub credential_slot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFixtureV1 {
    pub provider_id: String,
    pub action_id: String,
    pub version: u32,
    pub synthetic_input: Value,
    pub expected_schema: Value,
    pub mock_response: Value,
    #[serde(default)]
    pub error_cases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SdkError {
    UnsupportedVersion(u32),
    InvalidField(&'static str),
    InvalidSchema(String),
    OversizedManifest,
    DuplicateIdentity(String),
    MissingScope(String),
    Unavailable(String),
    UnresolvedBinding,
}

pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

pub fn validate_manifest(manifest: &IntegrationProviderManifestV1) -> Result<(), SdkError> {
    if manifest.schema_version != SDK_SCHEMA_VERSION {
        return Err(SdkError::UnsupportedVersion(manifest.schema_version));
    }
    let bytes = serde_json::to_vec(manifest).map_err(|_| SdkError::OversizedManifest)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(SdkError::OversizedManifest);
    }
    if manifest.id.is_empty() || manifest.display_name.is_empty() {
        return Err(SdkError::InvalidField("identity"));
    }
    let mut ids = BTreeSet::new();
    for action in &manifest.actions {
        if action.id.is_empty() || !ids.insert(format!("{}:{}", action.id, action.version)) {
            return Err(SdkError::DuplicateIdentity(action.id.clone()));
        }
        validate_schema(&action.input_schema, 0)?;
        validate_schema(&action.output_schema, 0)?;
    }
    validate_schema(&manifest.credential_schema, 0)
}

fn validate_schema(value: &Value, depth: usize) -> Result<(), SdkError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(SdkError::InvalidSchema("depth".into()));
    }
    match value {
        Value::Object(map) => {
            if map.len() > MAX_SCHEMA_PROPERTIES {
                return Err(SdkError::InvalidSchema("properties".into()));
            }
            for key in map.keys() {
                if !matches!(
                    key.as_str(),
                    "type"
                        | "properties"
                        | "required"
                        | "items"
                        | "enum"
                        | "minLength"
                        | "maxLength"
                        | "minimum"
                        | "maximum"
                ) {
                    return Err(SdkError::InvalidSchema(format!("keyword:{key}")));
                }
            }
            for (key, child) in map {
                if key == "properties" {
                    if let Value::Object(properties) = child {
                        if properties.len() > MAX_SCHEMA_PROPERTIES {
                            return Err(SdkError::InvalidSchema("properties".into()));
                        }
                        for property in properties.values() {
                            validate_schema(property, depth + 1)?;
                        }
                    } else {
                        return Err(SdkError::InvalidSchema("properties_type".into()));
                    }
                } else {
                    validate_schema(child, depth + 1)?;
                }
            }
        }
        Value::Array(items) => {
            if items.len() > MAX_SCHEMA_ITEMS {
                return Err(SdkError::InvalidSchema("items".into()));
            }
            for child in items {
                validate_schema(child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn fixture_echo_manifest() -> IntegrationProviderManifestV1 {
    let action = IntegrationActionV1 {
        id: "echo".into(),
        version: 1,
        description: "Deterministic fixture action".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"value":{"type":"string"}}}),
        output_schema: serde_json::json!({"type":"object"}),
        required_scopes: vec![],
        risk_class: RiskClass::ReadOnly,
        side_effect_class: "read_only".into(),
        idempotency_support: true,
    };
    let mut manifest = IntegrationProviderManifestV1 {
        schema_version: 1,
        id: "fixture.echo".into(),
        version: 1,
        display_name: "Fixture Echo".into(),
        description: "Deterministic offline provider".into(),
        auth_methods: vec![AuthMethod::None],
        actions: vec![action],
        triggers: vec![],
        credential_schema: serde_json::json!({"type":"object"}),
        content_hash: String::new(),
    };
    manifest.content_hash = canonical_hash(&manifest).expect("fixture hash");
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_is_valid_and_hashed() {
        let manifest = fixture_echo_manifest();
        assert!(validate_manifest(&manifest).is_ok());
        assert!(!manifest.content_hash.is_empty());
    }
    #[test]
    fn unknown_schema_keyword_is_rejected() {
        let mut manifest = fixture_echo_manifest();
        manifest.actions[0].input_schema = serde_json::json!({"type":"object","x-unsafe":true});
        assert!(matches!(
            validate_manifest(&manifest),
            Err(SdkError::InvalidSchema(_))
        ));
    }
    #[test]
    fn duplicate_action_version_is_rejected() {
        let mut manifest = fixture_echo_manifest();
        manifest.actions.push(manifest.actions[0].clone());
        assert!(matches!(
            validate_manifest(&manifest),
            Err(SdkError::DuplicateIdentity(_))
        ));
    }
}
