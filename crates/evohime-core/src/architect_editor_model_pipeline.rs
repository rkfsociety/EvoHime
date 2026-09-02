//! Core-owned two-phase architect/editor contract. Architect output is an
//! intent only; the editor must still pass the existing mutation boundary.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_PATHS: usize = 128;
pub const MAX_INTENT_BYTES: usize = 256 * 1024;
pub const MAX_RETRIES: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Architect,
    Editor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Created,
    Architecting,
    IntentReady,
    Editing,
    Validating,
    Completed,
    Failed,
    Drifted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub purpose: Phase,
    pub model_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditIntent {
    pub schema_version: u32,
    pub objective_hash: String,
    pub workspace_revision: String,
    pub allowed_paths: Vec<String>,
    pub operations: Vec<String>,
    pub summary: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPhasePipeline {
    pub schema_version: u32,
    pub id: String,
    pub architect: ModelProfile,
    pub editor: ModelProfile,
    pub same_model: bool,
    pub status: PipelineStatus,
    pub retries: u8,
    pub workspace_revision: String,
    pub intent: Option<EditIntent>,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PipelineError {
    #[error("unsupported pipeline schema")]
    UnsupportedVersion,
    #[error("invalid pipeline identifier")]
    InvalidId,
    #[error("pipeline bounds exceeded")]
    Bounds,
    #[error("workspace drift between phases")]
    WorkspaceDrift,
    #[error("architect output is not a valid edit intent")]
    InvalidIntent,
    #[error("retry limit exceeded")]
    RetryLimit,
}
fn valid_id(v: &str) -> bool {
    !v.is_empty() && v.len() <= MAX_ID_BYTES && !v.bytes().any(|b| b.is_ascii_control())
}
pub fn hash(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}
pub fn validate_intent(intent: &EditIntent) -> Result<(), PipelineError> {
    if intent.schema_version != SCHEMA_VERSION
        || intent.allowed_paths.len() > MAX_PATHS
        || intent.operations.len() > MAX_PATHS
        || intent.summary.len() > MAX_INTENT_BYTES
    {
        return Err(PipelineError::InvalidIntent);
    };
    if intent.allowed_paths.iter().any(|p| {
        !valid_id(p)
            || p.starts_with('/')
            || p.starts_with('\\')
            || p.contains("..")
            || p.contains(':')
    }) {
        return Err(PipelineError::InvalidIntent);
    };
    Ok(())
}
pub fn validate_pipeline(p: &ModelPhasePipeline) -> Result<(), PipelineError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(PipelineError::UnsupportedVersion);
    };
    if !valid_id(&p.id)
        || !valid_id(&p.architect.id)
        || !valid_id(&p.editor.id)
        || !valid_id(&p.architect.model_id)
        || !valid_id(&p.editor.model_id)
    {
        return Err(PipelineError::InvalidId);
    };
    if p.retries > MAX_RETRIES
        || p.architect.purpose != Phase::Architect
        || p.editor.purpose != Phase::Editor
    {
        return Err(PipelineError::Bounds);
    };
    if let Some(i) = &p.intent {
        validate_intent(i)?;
        if i.workspace_revision != p.workspace_revision {
            return Err(PipelineError::WorkspaceDrift);
        }
    };
    Ok(())
}
pub fn accept_intent(
    p: &mut ModelPhasePipeline,
    intent: EditIntent,
    observed_revision: &str,
) -> Result<(), PipelineError> {
    if observed_revision != p.workspace_revision {
        return Err(PipelineError::WorkspaceDrift);
    };
    validate_intent(&intent)?;
    p.intent = Some(intent);
    p.status = PipelineStatus::IntentReady;
    p.content_hash = hash(p);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> ModelPhasePipeline {
        ModelPhasePipeline {
            schema_version: 1,
            id: "p".into(),
            architect: ModelProfile {
                id: "a".into(),
                purpose: Phase::Architect,
                model_id: "m1".into(),
            },
            editor: ModelProfile {
                id: "e".into(),
                purpose: Phase::Editor,
                model_id: "m2".into(),
            },
            same_model: false,
            status: PipelineStatus::Created,
            retries: 0,
            workspace_revision: "r1".into(),
            intent: None,
            content_hash: String::new(),
        }
    }
    #[test]
    fn intent_is_typed_and_drift_fenced() {
        let mut p = p();
        let i = EditIntent {
            schema_version: 1,
            objective_hash: "h".into(),
            workspace_revision: "r1".into(),
            allowed_paths: vec!["src/a.rs".into()],
            operations: vec!["write".into()],
            summary: "bounded".into(),
        };
        accept_intent(&mut p, i, "r1").unwrap();
        assert_eq!(p.status, PipelineStatus::IntentReady);
        assert_eq!(
            accept_intent(
                &mut p,
                EditIntent {
                    schema_version: 1,
                    objective_hash: "h".into(),
                    workspace_revision: "r1".into(),
                    allowed_paths: vec![],
                    operations: vec![],
                    summary: String::new()
                },
                "r2"
            ),
            Err(PipelineError::WorkspaceDrift)
        );
    }
    #[test]
    fn invalid_authority_path_rejected() {
        let mut p = p();
        let i = EditIntent {
            schema_version: 1,
            objective_hash: "h".into(),
            workspace_revision: "r1".into(),
            allowed_paths: vec!["../secret".into()],
            operations: vec![],
            summary: String::new(),
        };
        assert_eq!(
            accept_intent(&mut p, i, "r1"),
            Err(PipelineError::InvalidIntent)
        );
    }
}
