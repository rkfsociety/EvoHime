//! Core-owned contract for version-pinned invocation presets (plan 35).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const INVOCATION_PRESET_SCHEMA_VERSION: u32 = 1;
pub const MAX_PRESET_ID_BYTES: usize = 128;
pub const MAX_NAME_CHARS: usize = 160;
pub const MAX_DESCRIPTION_CHARS: usize = 1_024;
pub const MAX_INPUTS: usize = 64;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_CREDENTIAL_BINDINGS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationPresetState {
    Ready,
    NeedsRebinding,
    NeedsMigration,
    IncompatibleSchema,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationPreset {
    pub schema_version: u32,
    pub id: String,
    pub owner_scope: String,
    pub name: String,
    pub description: Option<String>,
    pub workflow_id: String,
    pub workflow_version: u32,
    pub workflow_definition_hash: String,
    pub input_schema_hash: String,
    pub input_values: BTreeMap<String, Value>,
    pub credential_bindings: BTreeMap<String, String>,
    pub execution_options: BTreeMap<String, Value>,
    pub created_from_run_id: Option<String>,
    pub revision: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub content_hash: String,
    pub state: InvocationPresetState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedRunPreview {
    pub input_values: BTreeMap<String, Value>,
    pub credential_bindings: BTreeMap<String, String>,
    pub execution_options: BTreeMap<String, Value>,
    pub retained_fields: Vec<String>,
    pub removed_fields: Vec<String>,
    pub rejected_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetValidationError {
    InvalidField(String),
    UnsupportedSchema(u32),
    TooManyInputs,
    InputTooLarge,
    InvalidSensitiveField(String),
}

impl std::fmt::Display for PresetValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "invalid field: {field}"),
            Self::UnsupportedSchema(version) => write!(f, "unsupported schema: {version}"),
            Self::TooManyInputs => write!(f, "too many inputs"),
            Self::InputTooLarge => write!(f, "input payload too large"),
            Self::InvalidSensitiveField(field) => {
                write!(f, "sensitive field is not allowed: {field}")
            }
        }
    }
}

impl std::error::Error for PresetValidationError {}

fn bounded_text(name: &str, value: &str, max: usize) -> Result<(), PresetValidationError> {
    if value.is_empty() || value.chars().count() > max {
        return Err(PresetValidationError::InvalidField(name.into()));
    }
    Ok(())
}

fn forbidden_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "api_key",
        "bearer",
        "credential_value",
        "capability",
        "grant",
        "approval",
        "provider",
        "action",
        "executable",
        "path",
        "url",
        "network",
        "workflow_graph",
        "node_identity",
        "prompt",
        "output",
        "transcript",
    ]
    .iter()
    .any(|part| key.contains(part))
}

fn validate_value(path: &str, value: &Value) -> Result<(), PresetValidationError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}.{key}");
                if forbidden_key(key) {
                    return Err(PresetValidationError::InvalidSensitiveField(child_path));
                }
                validate_value(&child_path, child)?;
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                validate_value(&format!("{path}[{index}]"), child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl InvocationPreset {
    pub fn validate(&self) -> Result<(), PresetValidationError> {
        if self.schema_version != INVOCATION_PRESET_SCHEMA_VERSION {
            return Err(PresetValidationError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        bounded_text("id", &self.id, MAX_PRESET_ID_BYTES)?;
        bounded_text("owner_scope", &self.owner_scope, MAX_PRESET_ID_BYTES)?;
        bounded_text("name", &self.name, MAX_NAME_CHARS)?;
        if let Some(description) = &self.description {
            if description.chars().count() > MAX_DESCRIPTION_CHARS {
                return Err(PresetValidationError::InvalidField("description".into()));
            }
        }
        bounded_text("workflow_id", &self.workflow_id, MAX_PRESET_ID_BYTES)?;
        bounded_text(
            "workflow_definition_hash",
            &self.workflow_definition_hash,
            128,
        )?;
        bounded_text("input_schema_hash", &self.input_schema_hash, 128)?;
        if self.input_values.len() > MAX_INPUTS || self.execution_options.len() > MAX_INPUTS {
            return Err(PresetValidationError::TooManyInputs);
        }
        if self.credential_bindings.len() > MAX_CREDENTIAL_BINDINGS {
            return Err(PresetValidationError::TooManyInputs);
        }
        for (key, value) in self
            .input_values
            .iter()
            .chain(self.execution_options.iter())
        {
            if forbidden_key(key) {
                return Err(PresetValidationError::InvalidSensitiveField(key.clone()));
            }
            validate_value(key, value)?;
        }
        for (key, reference) in &self.credential_bindings {
            bounded_text("credential_binding", key, MAX_PRESET_ID_BYTES)?;
            bounded_text("credential_reference", reference, MAX_PRESET_ID_BYTES)?;
            if forbidden_key(key) || forbidden_key(reference) {
                return Err(PresetValidationError::InvalidSensitiveField(key.clone()));
            }
        }
        let bytes = serde_json::to_vec(&self.input_values).expect("preset values serialize");
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(PresetValidationError::InputTooLarge);
        }
        Ok(())
    }

    pub fn canonical_content_hash(&self) -> String {
        #[derive(Serialize)]
        struct Content<'a> {
            schema_version: u32,
            id: &'a str,
            owner_scope: &'a str,
            name: &'a str,
            description: &'a Option<String>,
            workflow_id: &'a str,
            workflow_version: u32,
            workflow_definition_hash: &'a str,
            input_schema_hash: &'a str,
            input_values: &'a BTreeMap<String, Value>,
            credential_bindings: &'a BTreeMap<String, String>,
            execution_options: &'a BTreeMap<String, Value>,
            created_from_run_id: &'a Option<String>,
            revision: u64,
        }
        let content = Content {
            schema_version: self.schema_version,
            id: &self.id,
            owner_scope: &self.owner_scope,
            name: &self.name,
            description: &self.description,
            workflow_id: &self.workflow_id,
            workflow_version: self.workflow_version,
            workflow_definition_hash: &self.workflow_definition_hash,
            input_schema_hash: &self.input_schema_hash,
            input_values: &self.input_values,
            credential_bindings: &self.credential_bindings,
            execution_options: &self.execution_options,
            created_from_run_id: &self.created_from_run_id,
            revision: self.revision,
        };
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(&content).expect("preset content serializes"));
        hex::encode(hasher.finalize())
    }
}

pub fn sanitize_completed_run(
    metadata: &Value,
) -> Result<SanitizedRunPreview, PresetValidationError> {
    let Some(object) = metadata.as_object() else {
        return Err(PresetValidationError::InvalidField(
            "completed_run_metadata".into(),
        ));
    };
    let mut preview = SanitizedRunPreview {
        input_values: BTreeMap::new(),
        credential_bindings: BTreeMap::new(),
        execution_options: BTreeMap::new(),
        retained_fields: Vec::new(),
        removed_fields: Vec::new(),
        rejected_fields: Vec::new(),
    };
    for (key, value) in object {
        match key.as_str() {
            "input_values" | "execution_options" => {
                let Some(values) = value.as_object() else {
                    return Err(PresetValidationError::InvalidField(key.clone()));
                };
                for (field, item) in values {
                    if forbidden_key(field) {
                        preview.rejected_fields.push(format!("{key}.{field}"));
                        continue;
                    }
                    validate_value(&format!("{key}.{field}"), item)?;
                    let target = if key == "input_values" {
                        &mut preview.input_values
                    } else {
                        &mut preview.execution_options
                    };
                    target.insert(field.clone(), item.clone());
                    preview.retained_fields.push(format!("{key}.{field}"));
                }
            }
            "credential_bindings" => {
                let Some(bindings) = value.as_object() else {
                    return Err(PresetValidationError::InvalidField(key.clone()));
                };
                for (slot, reference) in bindings {
                    let Some(reference) = reference.as_str() else {
                        preview.rejected_fields.push(format!("{key}.{slot}"));
                        continue;
                    };
                    if forbidden_key(slot) || forbidden_key(reference) {
                        preview.rejected_fields.push(format!("{key}.{slot}"));
                        continue;
                    }
                    preview
                        .credential_bindings
                        .insert(slot.clone(), reference.into());
                    preview.retained_fields.push(format!("{key}.{slot}"));
                }
            }
            _ => preview.removed_fields.push(key.clone()),
        }
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset() -> InvocationPreset {
        InvocationPreset {
            schema_version: 1,
            id: "p1".into(),
            owner_scope: "owner".into(),
            name: "Preset".into(),
            description: None,
            workflow_id: "wf".into(),
            workflow_version: 1,
            workflow_definition_hash: "def".into(),
            input_schema_hash: "schema".into(),
            input_values: BTreeMap::from([("topic".into(), Value::String("rust".into()))]),
            credential_bindings: BTreeMap::from([("github".into(), "ref:github".into())]),
            execution_options: BTreeMap::new(),
            created_from_run_id: None,
            revision: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
            content_hash: String::new(),
            state: InvocationPresetState::Ready,
        }
    }

    #[test]
    fn validates_and_hashes_deterministically() {
        let mut value = preset();
        value.validate().unwrap();
        value.content_hash = value.canonical_content_hash();
        assert_eq!(value.content_hash.len(), 64);
    }

    #[test]
    fn rejects_authority_and_secret_fields() {
        let mut value = preset();
        value
            .input_values
            .insert("capability".into(), Value::String("admin".into()));
        assert!(matches!(
            value.validate(),
            Err(PresetValidationError::InvalidSensitiveField(_))
        ));
    }

    #[test]
    fn sanitizer_reports_removed_and_rejected_fields() {
        let preview = sanitize_completed_run(&serde_json::json!({"input_values":{"topic":"x","token":"no"},"credential_bindings":{"github":"ref:github"},"prompt":"no"})).unwrap();
        assert_eq!(preview.input_values.len(), 1);
        assert_eq!(preview.rejected_fields, vec!["input_values.token"]);
        assert_eq!(preview.removed_fields, vec!["prompt"]);
    }
}
