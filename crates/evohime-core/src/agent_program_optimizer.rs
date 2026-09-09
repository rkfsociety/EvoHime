use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_ID: usize = 128;
const MAX_STEPS: usize = 64;
const MAX_TEXT: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle { Draft, Active, Superseded, Invalid }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramStep { pub id: String, pub kind: String, pub input_hash: String, pub capability: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProgram { pub schema_version: u32, pub id: String, pub revision: u64, pub lifecycle: Lifecycle, pub scope: String, pub objective_hash: String, pub steps: Vec<ProgramStep>, pub content_hash: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationVerdict { pub program_id: String, pub revision: u64, pub content_hash: String, pub accepted: bool, pub score: u32, pub reason: String }

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OptimizerError { #[error("invalid agent program: {0}")] Invalid(String) }

fn bounded(value: &str, limit: usize, name: &str) -> Result<(), OptimizerError> {
    if value.trim().is_empty() || value.len() > limit { return Err(OptimizerError::Invalid(format!("{name}_out_of_bounds"))); }
    Ok(())
}

pub fn canonical_hash(program: &AgentProgram) -> Result<String, OptimizerError> {
    let mut normalized = program.clone(); normalized.content_hash.clear();
    let bytes = serde_json::to_vec(&normalized).map_err(|_| OptimizerError::Invalid("program_not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn validate(program: &AgentProgram) -> Result<(), OptimizerError> {
    if program.schema_version != SCHEMA_VERSION { return Err(OptimizerError::Invalid("unsupported_schema_version".into())); }
    bounded(&program.id, MAX_ID, "program_id")?; bounded(&program.scope, MAX_TEXT, "scope")?; bounded(&program.objective_hash, MAX_ID, "objective_hash")?;
    if program.revision == 0 || program.steps.is_empty() || program.steps.len() > MAX_STEPS { return Err(OptimizerError::Invalid("step_count_or_revision_invalid".into())); }
    let mut ids = std::collections::BTreeSet::new();
    for step in &program.steps {
        bounded(&step.id, MAX_ID, "step_id")?; bounded(&step.kind, MAX_ID, "step_kind")?; bounded(&step.input_hash, MAX_ID, "input_hash")?; bounded(&step.capability, MAX_ID, "capability")?;
        if !ids.insert(step.id.as_str()) { return Err(OptimizerError::Invalid("duplicate_step_id".into())); }
    }
    if canonical_hash(program)? != program.content_hash { return Err(OptimizerError::Invalid("content_hash_mismatch".into())); }
    Ok(())
}

pub fn optimize(program: &AgentProgram) -> Result<OptimizationVerdict, OptimizerError> {
    validate(program)?;
    if program.lifecycle != Lifecycle::Active { return Err(OptimizerError::Invalid("program_is_not_active".into())); }
    let score = 100u32.saturating_sub((program.steps.len().saturating_sub(1) as u32).saturating_mul(2));
    Ok(OptimizationVerdict { program_id: program.id.clone(), revision: program.revision, content_hash: program.content_hash.clone(), accepted: true, score, reason: "deterministic_metadata_only".into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn program(lifecycle: Lifecycle) -> AgentProgram {
        let mut value = AgentProgram { schema_version: SCHEMA_VERSION, id: "program".into(), revision: 1, lifecycle, scope: "workspace".into(), objective_hash: "objective".into(), steps: vec![ProgramStep { id: "step".into(), kind: "observe".into(), input_hash: "input".into(), capability: "runtime.observe".into() }], content_hash: String::new() };
        value.content_hash = canonical_hash(&value).expect("hash"); value
    }
    #[test]
    fn optimizes_only_active_valid_program() { assert!(optimize(&program(Lifecycle::Active)).expect("verdict").accepted); assert!(optimize(&program(Lifecycle::Draft)).is_err()); }
    #[test]
    fn rejects_duplicate_steps_and_tampering() { let mut value = program(Lifecycle::Active); value.steps.push(value.steps[0].clone()); assert!(validate(&value).is_err()); let mut tampered = program(Lifecycle::Active); tampered.scope = "other".into(); assert!(validate(&tampered).is_err()); }
}
