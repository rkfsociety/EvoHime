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

/// Current schema version for side-effect-free tool simulation.
pub const CONTRACT_VERSION: u32 = 1;
/// Stable identifier included in simulation contract hashes.
pub const CONTRACT_ID: &str = "tool-simulation-runtime-v1";
/// Maximum byte length of simulation and fixture identifiers.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum serialized simulation input size.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Maximum serialized fixture or simulation output size.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
/// Maximum number of fixtures registered in one process-local runtime.
pub const MAX_FIXTURES: usize = 256;

/// Execution mode requested for a simulated tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSimulationMode {
    /// Requests real execution; rejected by the simulation runtime.
    Real,
    /// Resolves output from an exact registered fixture.
    Fixture,
    /// Uses caller-supplied synthetic output.
    Emulated,
    /// Resolves a fixture without executing the underlying tool.
    DryRun,
}

/// Processing state reported for a simulated invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationState {
    /// Request metadata is being checked.
    Validating,
    /// Request was intercepted before reaching a real tool executor.
    Intercepted,
    /// An exact registered fixture was selected.
    FixtureResolved,
    /// Simulation completed with synthetic output.
    Completed,
    /// Policy or validation prevented simulation.
    Blocked,
    /// Required simulation fixture or dependency is unavailable.
    Unavailable,
}

/// Provenance category for the output returned by simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationProvenance {
    /// Output was supplied as synthetic emulation data.
    Synthetic,
    /// Output came from an explicitly registered deterministic fixture.
    Fixture,
}

/// Authorized, bounded request intercepted by the simulation runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationRequest {
    /// Simulation contract schema version.
    pub schema_version: u32,
    /// Owning run identifier.
    pub run_id: String,
    /// Requested tool identifier.
    pub tool_id: String,
    /// Requested simulation mode.
    pub mode: ToolSimulationMode,
    /// Serialized input matched against an exact fixture.
    pub input: Value,
    /// Optional output used only by emulated mode.
    pub emulated_output: Option<Value>,
    /// Correlation identifier for tracing the request.
    pub correlation_id: String,
    /// Key used to replay completed simulation results idempotently.
    pub idempotency_key: String,
    /// Digest of the policy authorizing this simulation.
    pub policy_hash: String,
    /// Whether the caller holds the required capability.
    pub capability_granted: bool,
}

/// Exact tool/input fixture with optional structured-output schema binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixtureDefinition {
    /// Fixture schema version.
    pub schema_version: u32,
    /// Stable fixture identifier.
    pub fixture_id: String,
    /// Tool identifier matched by the fixture.
    pub tool_id: String,
    /// Digest of the exact serialized input expected by this fixture.
    pub input_hash: String,
    /// Output value returned when the fixture matches.
    pub output: Value,
    /// Optional digest of the response contract validating the output.
    pub output_schema_hash: Option<String>,
}

/// Simulation outcome with provenance and non-persistent output payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Result schema version.
    pub schema_version: u32,
    /// Run that owns this simulation result.
    pub run_id: String,
    /// Tool identifier simulated.
    pub tool_id: String,
    /// Mode used to obtain the output.
    pub mode: ToolSimulationMode,
    /// Simulation processing state.
    pub state: SimulationState,
    /// Source category of the output, when available.
    pub provenance: Option<SimulationProvenance>,
    /// Digest of the output value.
    pub output_hash: Option<String>,
    /// Registered fixture selected, if applicable.
    pub fixture_id: Option<String>,
    /// Correlation identifier copied from the request.
    pub correlation_id: String,
    /// Idempotency key used to deduplicate the request.
    pub idempotency_key: String,
    /// Response contract digest used to validate the output, if supplied.
    pub contract_hash: Option<String>,
    /// Stable machine-readable failure code, when simulation did not complete.
    pub error_code: Option<String>,
    #[serde(skip)]
    /// Output value available to the in-process caller but omitted from serialization.
    pub output: Value,
}

/// Invalid request, unsupported mode, missing fixture, or configured bound failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationError {
    /// Request or fixture metadata violates the contract.
    InvalidRequest(&'static str),
    /// Input uses an unsupported schema version.
    UnsupportedVersion(u32),
    /// No exact fixture matches the requested tool and input digest.
    FixtureMissing,
    /// Real execution was requested through a simulation-only runtime.
    RealModeNotIntercepted,
    /// Fixture or synthetic output violates the response contract.
    StructuredResponse(String),
    /// Input, output, or fixture capacity bound was exceeded.
    Limit(&'static str),
    /// Request key was already used with conflicting input.
    Duplicate,
}

impl SimulationError {
    /// Returns the stable wire-safe error code for this error kind.
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

/// Computes a deterministic digest of a JSON input or output value.
pub fn value_hash(value: &Value) -> Result<String, SimulationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| SimulationError::InvalidRequest("json"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Creates a default synthetic fixture bound to one tool and exact input value.
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

/// Process-local fixture registry and idempotent completed-result cache.
#[derive(Debug, Default)]
pub struct ToolSimulationRuntime {
    fixtures: BTreeMap<(String, String), FixtureDefinition>,
    completed: BTreeMap<String, SimulationResult>,
}

impl ToolSimulationRuntime {
    /// Validates and registers a bounded output fixture.
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

    /// Intercepts a request and returns fixture or synthetic output without tool execution.
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

    /// Returns the number of fixtures currently registered.
    pub fn fixture_count(&self) -> usize {
        self.fixtures.len()
    }
    /// Returns the number of idempotent results retained in this process.
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
