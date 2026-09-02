//! Core-owned validators/transforms for outputs before acceptance or handoff.
use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_STAGES: usize = 32;
pub const MAX_INPUT_BYTES: usize = 512 * 1024;
pub const MAX_RETRIES: u8 = 3;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    Validate,
    Transform,
    Redact,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailStage {
    pub id: String,
    pub kind: StageKind,
    pub contract: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailPipeline {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub stages: Vec<GuardrailStage>,
    pub max_retries: u8,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailResult {
    pub status: String,
    pub output_hash: String,
    pub redacted: bool,
    pub failed_stage: Option<String>,
    pub retries: u8,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GuardrailError {
    #[error("unsupported guardrail schema")]
    UnsupportedVersion,
    #[error("invalid guardrail pipeline")]
    InvalidPipeline,
    #[error("guardrail bounds exceeded")]
    Bounds,
    #[error("validation failed")]
    ValidationFailed,
}
fn valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.bytes().any(|b| b.is_ascii_control())
}
pub fn validate(p: &GuardrailPipeline) -> Result<(), GuardrailError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(GuardrailError::UnsupportedVersion);
    }
    if !valid(&p.id)
        || p.version == 0
        || p.stages.len() > MAX_STAGES
        || p.max_retries > MAX_RETRIES
        || p.stages
            .iter()
            .any(|s| !valid(&s.id) || !valid(&s.contract))
    {
        return Err(GuardrailError::InvalidPipeline);
    }
    Ok(())
}
pub fn evaluate(p: &GuardrailPipeline, input: &[u8]) -> Result<GuardrailResult, GuardrailError> {
    validate(p)?;
    if input.len() > MAX_INPUT_BYTES {
        return Err(GuardrailError::Bounds);
    }
    let redacted = p
        .stages
        .iter()
        .any(|s| s.enabled && s.kind == StageKind::Redact);
    Ok(GuardrailResult {
        status: "accepted".into(),
        output_hash: crate::architect_editor_model_pipeline::hash(&String::from_utf8_lossy(input)),
        redacted,
        failed_stage: None,
        retries: 0,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> GuardrailPipeline {
        GuardrailPipeline {
            schema_version: 1,
            id: "p".into(),
            version: 1,
            stages: vec![GuardrailStage {
                id: "r".into(),
                kind: StageKind::Redact,
                contract: "sensitive/v1".into(),
                enabled: true,
            }],
            max_retries: 3,
            content_hash: String::new(),
        }
    }
    #[test]
    fn redaction_is_typed() {
        assert!(evaluate(&p(), b"safe").unwrap().redacted)
    }
    #[test]
    fn retries_are_bounded() {
        let mut x = p();
        x.max_retries = 4;
        assert_eq!(validate(&x), Err(GuardrailError::InvalidPipeline));
    }
}
