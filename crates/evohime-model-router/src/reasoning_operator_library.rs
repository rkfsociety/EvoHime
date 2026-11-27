//! Core-owned typed reasoning operators. Operators propose data; they never
//! acquire tools, grants, or mutation authority.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_OPERATORS: usize = 64;
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorKind {
    Generate,
    Review,
    Revise,
    Ensemble,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningOperatorDefinition {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub kind: OperatorKind,
    pub model_profile: String,
    pub input_contract: String,
    pub output_contract: String,
    pub max_attempts: u8,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorRequest {
    pub schema_version: u32,
    pub operator_id: String,
    pub input_json: Vec<u8>,
    pub idempotency_key: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OperatorError {
    #[error("unsupported operator schema")]
    UnsupportedVersion,
    #[error("invalid operator definition")]
    InvalidDefinition,
    #[error("operator bounds exceeded")]
    Bounds,
    #[error("unknown operator")]
    NotFound,
}
fn valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.bytes().any(|b| b.is_ascii_control())
}
pub fn hash<T: Serialize>(v: &T) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap_or_default()))
}
pub fn validate(d: &ReasoningOperatorDefinition) -> Result<(), OperatorError> {
    if d.schema_version != SCHEMA_VERSION {
        return Err(OperatorError::UnsupportedVersion);
    }
    if !valid(&d.id)
        || !valid(&d.model_profile)
        || !valid(&d.input_contract)
        || !valid(&d.output_contract)
        || d.version == 0
        || d.max_attempts > 3
    {
        return Err(OperatorError::InvalidDefinition);
    }
    Ok(())
}
pub fn validate_request(r: &OperatorRequest) -> Result<(), OperatorError> {
    if r.schema_version != SCHEMA_VERSION {
        return Err(OperatorError::UnsupportedVersion);
    }
    if !valid(&r.operator_id) || !valid(&r.idempotency_key) || r.input_json.len() > MAX_INPUT_BYTES
    {
        return Err(OperatorError::Bounds);
    }
    Ok(())
}
pub fn builtins() -> Vec<ReasoningOperatorDefinition> {
    [
        ("builtin.generate", OperatorKind::Generate),
        ("builtin.review", OperatorKind::Review),
        ("builtin.revise", OperatorKind::Revise),
        ("builtin.ensemble", OperatorKind::Ensemble),
    ]
    .into_iter()
    .map(|(id, kind)| ReasoningOperatorDefinition {
        schema_version: 1,
        id: id.into(),
        version: 1,
        kind,
        model_profile: "default".into(),
        input_contract: "bounded_json".into(),
        output_contract: "typed_json".into(),
        max_attempts: 3,
        content_hash: String::new(),
    })
    .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_builtins() {
        for d in builtins() {
            validate(&d).unwrap()
        }
    }
    #[test]
    fn rejects_oversized_request() {
        let r = OperatorRequest {
            schema_version: 1,
            operator_id: "x".into(),
            input_json: vec![0; MAX_INPUT_BYTES + 1],
            idempotency_key: "i".into(),
        };
        assert_eq!(validate_request(&r), Err(OperatorError::Bounds));
    }
}
