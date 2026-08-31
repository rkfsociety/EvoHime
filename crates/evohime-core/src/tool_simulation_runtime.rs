//! Core-owned, side-effect-free tool simulation runtime.
//!
//! Simulation is an interception boundary, not a second executor: it validates
//! the same bounded request metadata, resolves an exact fixture, and never calls
//! a ToolRegistry effect adapter. Run state is intentionally process-local.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::structured_response_contract::ResponseContract;

pub const CONTRACT_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "tool-simulation-runtime-v1";
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_FIXTURES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSimulationMode {
    Real,
    Fixture,
    Emulated,
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationState {
    Validating,
    Intercepted,
    FixtureResolved,
    Completed,
    Blocked,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationProvenance {
    Synthetic,
    Fixture,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationRequest {
    pub schema_version: u32,
    pub run_id: String,
    pub tool_id: String,
    pub mode: ToolSimulationMode,
    pub input: Value,
    pub emulated_output: Option<Value>,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub policy_hash: String,
    pub capability_granted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixtureDefinition {
    pub schema_version: u32,
    pub fixture_id: String,
    pub tool_id: String,
    pub input_hash: String,
    pub output: Value,
    pub output_schema_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub schema_version: u32,
    pub run_id: String,
    pub tool_id: String,
    pub mode: ToolSimulationMode,
    pub state: SimulationState,
    pub provenance: Option<SimulationProvenance>,
    pub output_hash: Option<String>,
    pub fixture_id: Option<String>,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub contract_hash: Option<String>,
    pub error_code: Option<String>,
    #[serde(skip)]
    pub output: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationError {
    InvalidRequest(&'static str),
    UnsupportedVersion(u32),
    FixtureMissing,
    RealModeNotIntercepted,
    StructuredResponse(String),
    Limit(&'static str),
    Duplicate,
}

impl SimulationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::UnsupportedVersion(_) => "unsupported_schema",
            Self::FixtureMissing => "fixture_missing",
            Self::RealModeNotIntercepted => "real_mode_requires_real_runtime",
            Self::StructuredResponse(_) => "structured_response_invalid",
            Self::Limit(_) => "limit_exceeded",
            Self::Duplicate => "duplicate",
        }
    }
}
impl std::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for SimulationError {}

pub fn value_hash(value: &Value) -> Result<String, SimulationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| SimulationError::InvalidRequest("json"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn fixture_for(tool_id: &str, input: &Value) -> FixtureDefinition {
    FixtureDefinition {
        schema_version: CONTRACT_VERSION,
        fixture_id: format!("fixture-{tool_id}"),
        tool_id: tool_id.to_owned(),
        input_hash: value_hash(input).unwrap_or_default(),
        output: serde_json::json!({"tool_id": tool_id, "simulated": true}),
        output_schema_hash: None,
    }
}

#[derive(Debug, Default)]
pub struct ToolSimulationRuntime {
    fixtures: BTreeMap<(String, String), FixtureDefinition>,
    completed: BTreeMap<String, SimulationResult>,
}

impl ToolSimulationRuntime {
    pub fn register_fixture(&mut self, fixture: FixtureDefinition) -> Result<(), SimulationError> {
        validate_id(&fixture.fixture_id)?;
        validate_id(&fixture.tool_id)?;
        if fixture.schema_version != CONTRACT_VERSION {
            return Err(SimulationError::UnsupportedVersion(fixture.schema_version));
        }
        let bytes = serde_json::to_vec(&fixture.output)
            .map_err(|_| SimulationError::InvalidRequest("output"))?;
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(SimulationError::Limit("output"));
        }
        if self.fixtures.len() >= MAX_FIXTURES {
            return Err(SimulationError::Limit("fixtures"));
        }
        self.fixtures.insert(
            (fixture.tool_id.clone(), fixture.input_hash.clone()),
            fixture,
        );
        Ok(())
    }

    pub fn simulate(
        &mut self,
        request: SimulationRequest,
        contract: Option<&ResponseContract>,
    ) -> Result<SimulationResult, SimulationError> {
        validate_request(&request)?;
        if let Some(previous) = self.completed.get(&request.idempotency_key) {
            return Ok(previous.clone());
        }
        if request.mode == ToolSimulationMode::Real {
            return Err(SimulationError::RealModeNotIntercepted);
        }
        let input_hash = value_hash(&request.input)?;
        let (output, provenance, fixture_id) = match request.mode {
            ToolSimulationMode::Fixture | ToolSimulationMode::DryRun => {
                let fixture = self
                    .fixtures
                    .get(&(request.tool_id.clone(), input_hash))
                    .ok_or(SimulationError::FixtureMissing)?;
                (
                    fixture.output.clone(),
                    SimulationProvenance::Fixture,
                    Some(fixture.fixture_id.clone()),
                )
            }
            ToolSimulationMode::Emulated => (
                request
                    .emulated_output
                    .clone()
                    .ok_or(SimulationError::InvalidRequest("emulated_output"))?,
                SimulationProvenance::Synthetic,
                None,
            ),
            ToolSimulationMode::Real => unreachable!(),
        };
        if let Some(contract) = contract {
            contract
                .validate_value(&output)
                .map_err(|e| SimulationError::StructuredResponse(e.to_string()))?;
        }
        let result = SimulationResult {
            schema_version: CONTRACT_VERSION,
            run_id: request.run_id,
            tool_id: request.tool_id,
            mode: request.mode,
            state: if fixture_id.is_some() {
                SimulationState::FixtureResolved
            } else {
                SimulationState::Completed
            },
            provenance: Some(provenance),
            output_hash: Some(value_hash(&output)?),
            fixture_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.idempotency_key.clone(),
            contract_hash: contract.map(|v| v.contract_hash.clone()),
            error_code: None,
            output,
        };
        self.completed
            .insert(request.idempotency_key, result.clone());
        Ok(result)
    }

    pub fn fixture_count(&self) -> usize {
        self.fixtures.len()
    }
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }
}

fn validate_id(value: &str) -> Result<(), SimulationError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
    {
        return Err(SimulationError::InvalidRequest("id"));
    }
    Ok(())
}

fn validate_request(request: &SimulationRequest) -> Result<(), SimulationError> {
    if request.schema_version != CONTRACT_VERSION {
        return Err(SimulationError::UnsupportedVersion(request.schema_version));
    }
    validate_id(&request.run_id)?;
    validate_id(&request.tool_id)?;
    validate_id(&request.correlation_id)?;
    validate_id(&request.idempotency_key)?;
    if request.policy_hash.is_empty() || !request.capability_granted {
        return Err(SimulationError::InvalidRequest("authority"));
    }
    let bytes =
        serde_json::to_vec(&request.input).map_err(|_| SimulationError::InvalidRequest("input"))?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(SimulationError::Limit("input"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(mode: ToolSimulationMode) -> SimulationRequest {
        SimulationRequest {
            schema_version: 1,
            run_id: "run-1".into(),
            tool_id: "fixture.echo".into(),
            mode,
            input: json!({"value":"x"}),
            emulated_output: None,
            correlation_id: "corr-1".into(),
            idempotency_key: "idem-1".into(),
            policy_hash: "policy".into(),
            capability_granted: true,
        }
    }

    #[test]
    fn exact_fixture_is_deterministic_and_synthetic() {
        let mut runtime = ToolSimulationRuntime::default();
        let mut fixture = fixture_for("fixture.echo", &request(ToolSimulationMode::Fixture).input);
        fixture.output = json!({"ok":true});
        runtime.register_fixture(fixture).unwrap();
        let first = runtime
            .simulate(request(ToolSimulationMode::DryRun), None)
            .unwrap();
        let second = runtime
            .simulate(request(ToolSimulationMode::DryRun), None)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.provenance, Some(SimulationProvenance::Fixture));
        assert_eq!(runtime.completed_count(), 1);
    }

    #[test]
    fn missing_fixture_and_real_mode_fail_closed() {
        let mut runtime = ToolSimulationRuntime::default();
        assert_eq!(
            runtime.simulate(request(ToolSimulationMode::Fixture), None),
            Err(SimulationError::FixtureMissing)
        );
        assert_eq!(
            runtime.simulate(request(ToolSimulationMode::Real), None),
            Err(SimulationError::RealModeNotIntercepted)
        );
        assert_eq!(runtime.fixture_count(), 0);
    }

    #[test]
    fn emulated_output_is_structured_response_validated() {
        let contract = ResponseContract::new(
            "simulation",
            1,
            json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
            crate::structured_response_contract::ResponseStrategy::SyntheticTool,
        )
        .unwrap();
        let mut req = request(ToolSimulationMode::Emulated);
        req.emulated_output = Some(json!({"ok":true}));
        let result = ToolSimulationRuntime::default()
            .simulate(req, Some(&contract))
            .unwrap();
        assert_eq!(result.provenance, Some(SimulationProvenance::Synthetic));
        assert_eq!(result.contract_hash, Some(contract.contract_hash));
    }
}
