use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current serialized schema version for agent programs.
pub const SCHEMA_VERSION: u32 = 1;
const MAX_ID: usize = 128;
const MAX_STEPS: usize = 64;
const MAX_TEXT: usize = 512;

/// Lifecycle state of an agent program revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Program is being assembled and cannot be optimized.
    Draft,
    /// Program is valid and available for deterministic optimization review.
    Active,
    /// Program revision has been replaced by a newer one.
    Superseded,
    /// Program revision is invalid and cannot be used.
    Invalid,
}

/// One bounded operation in an agent program.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramStep {
    /// Stable step identifier, unique within the program.
    pub id: String,
    /// Operation kind declared for this step.
    pub kind: String,
    /// Digest of the step's canonical input.
    pub input_hash: String,
    /// Capability required to execute the step.
    pub capability: String,
}

/// Content-addressed sequence of bounded agent operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProgram {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Stable program identifier.
    pub id: String,
    /// Monotonically increasing revision.
    pub revision: u64,
    /// Lifecycle state controlling whether optimization may inspect the program.
    pub lifecycle: Lifecycle,
    /// Scope in which the program may be used.
    pub scope: String,
    /// Digest of the objective used to derive this program.
    pub objective_hash: String,
    /// Ordered steps in the program.
    pub steps: Vec<ProgramStep>,
    /// SHA-256 digest of the canonical program with this field cleared.
    pub content_hash: String,
}

/// Deterministic metadata-only assessment of an agent program.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationVerdict {
    /// Identifier of the assessed program.
    pub program_id: String,
    /// Revision of the assessed program.
    pub revision: u64,
    /// Digest of the assessed program content.
    pub content_hash: String,
    /// Whether the metadata-only assessment accepted the program.
    pub accepted: bool,
    /// Deterministic score derived from the number of steps.
    pub score: u32,
    /// Stable explanation code for the assessment result.
    pub reason: String,
}

/// Validation or optimization failure for an agent program.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OptimizerError {
    /// The program violated schema, bounds, uniqueness, lifecycle, or digest requirements.
    #[error("invalid agent program: {0}")]
    Invalid(String),
}

fn bounded(value: &str, limit: usize, name: &str) -> Result<(), OptimizerError> {
    if value.trim().is_empty() || value.len() > limit {
        return Err(OptimizerError::Invalid(format!("{name}_out_of_bounds")));
    }
    Ok(())
}

/// Computes the canonical SHA-256 digest with `content_hash` cleared.
pub fn canonical_hash(program: &AgentProgram) -> Result<String, OptimizerError> {
    let mut normalized = program.clone();
    normalized.content_hash.clear();
    let bytes = serde_json::to_vec(&normalized)
        .map_err(|_| OptimizerError::Invalid("program_not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Validates schema, field bounds, unique steps, and the canonical digest.
pub fn validate(program: &AgentProgram) -> Result<(), OptimizerError> {
    if program.schema_version != SCHEMA_VERSION {
        return Err(OptimizerError::Invalid("unsupported_schema_version".into()));
    }
    bounded(&program.id, MAX_ID, "program_id")?;
    bounded(&program.scope, MAX_TEXT, "scope")?;
    bounded(&program.objective_hash, MAX_ID, "objective_hash")?;
    if program.revision == 0 || program.steps.is_empty() || program.steps.len() > MAX_STEPS {
        return Err(OptimizerError::Invalid(
            "step_count_or_revision_invalid".into(),
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for step in &program.steps {
        bounded(&step.id, MAX_ID, "step_id")?;
        bounded(&step.kind, MAX_ID, "step_kind")?;
        bounded(&step.input_hash, MAX_ID, "input_hash")?;
        bounded(&step.capability, MAX_ID, "capability")?;
        if !ids.insert(step.id.as_str()) {
            return Err(OptimizerError::Invalid("duplicate_step_id".into()));
        }
    }
    if canonical_hash(program)? != program.content_hash {
        return Err(OptimizerError::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}

/// Produces a deterministic metadata-only verdict for a valid active program.
///
/// # Example
///
/// ```
/// use evohime_core::agent_program_optimizer::{
///     canonical_hash, optimize, AgentProgram, Lifecycle, ProgramStep, SCHEMA_VERSION,
/// };
///
/// let mut program = AgentProgram {
///     schema_version: SCHEMA_VERSION,
///     id: "summarize".into(),
///     revision: 1,
///     lifecycle: Lifecycle::Active,
///     scope: "workspace".into(),
///     objective_hash: "objective-digest".into(),
///     steps: vec![ProgramStep {
///         id: "inspect".into(),
///         kind: "observe".into(),
///         input_hash: "input-digest".into(),
///         capability: "workspace.read".into(),
///     }],
///     content_hash: String::new(),
/// };
/// program.content_hash = canonical_hash(&program)?;
/// let verdict = optimize(&program)?;
/// assert!(verdict.accepted);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn optimize(program: &AgentProgram) -> Result<OptimizationVerdict, OptimizerError> {
    validate(program)?;
    if program.lifecycle != Lifecycle::Active {
        return Err(OptimizerError::Invalid("program_is_not_active".into()));
    }
    let score =
        100u32.saturating_sub((program.steps.len().saturating_sub(1) as u32).saturating_mul(2));
    Ok(OptimizationVerdict {
        program_id: program.id.clone(),
        revision: program.revision,
        content_hash: program.content_hash.clone(),
        accepted: true,
        score,
        reason: "deterministic_metadata_only".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn program(lifecycle: Lifecycle) -> AgentProgram {
        let mut value = AgentProgram {
            schema_version: SCHEMA_VERSION,
            id: "program".into(),
            revision: 1,
            lifecycle,
            scope: "workspace".into(),
            objective_hash: "objective".into(),
            steps: vec![ProgramStep {
                id: "step".into(),
                kind: "observe".into(),
                input_hash: "input".into(),
                capability: "runtime.observe".into(),
            }],
            content_hash: String::new(),
        };
        value.content_hash = canonical_hash(&value).expect("hash");
        value
    }
    #[test]
    fn optimizes_only_active_valid_program() {
        assert!(
            optimize(&program(Lifecycle::Active))
                .expect("verdict")
                .accepted
        );
        assert!(optimize(&program(Lifecycle::Draft)).is_err());
    }
    #[test]
    fn rejects_duplicate_steps_and_tampering() {
        let mut value = program(Lifecycle::Active);
        value.steps.push(value.steps[0].clone());
        assert!(validate(&value).is_err());
        let mut tampered = program(Lifecycle::Active);
        tampered.scope = "other".into();
        assert!(validate(&tampered).is_err());
    }
}
