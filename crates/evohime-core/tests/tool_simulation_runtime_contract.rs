use evohime_core::tool_simulation_runtime::{
    fixture_for, SimulationError, SimulationRequest, ToolSimulationMode, ToolSimulationRuntime,
};
use serde_json::json;

fn request(mode: ToolSimulationMode, key: &str) -> SimulationRequest {
    SimulationRequest {
        schema_version: 1,
        run_id: "contract-run".into(),
        tool_id: "fixture.echo".into(),
        mode,
        input: json!({"value":"ok"}),
        emulated_output: None,
        correlation_id: "contract-correlation".into(),
        idempotency_key: key.into(),
        policy_hash: "policy-hash".into(),
        capability_granted: true,
    }
}

#[test]
fn fixture_contract_is_versioned_bounded_and_redacts_authority() {
    let mut runtime = ToolSimulationRuntime::default();
    let input = request(ToolSimulationMode::Fixture, "contract-idem").input;
    runtime
        .register_fixture(fixture_for("fixture.echo", &input))
        .unwrap();
    let result = runtime
        .simulate(request(ToolSimulationMode::Fixture, "contract-idem"), None)
        .unwrap();
    assert_eq!(result.schema_version, 1);
    assert_eq!(result.mode, ToolSimulationMode::Fixture);
    assert_eq!(
        result.provenance.map(|value| format!("{value:?}")),
        Some("Fixture".into())
    );
    assert_eq!(runtime.completed_count(), 1);
}

#[test]
fn missing_fixture_never_becomes_real_execution() {
    let mut runtime = ToolSimulationRuntime::default();
    assert_eq!(
        runtime.simulate(request(ToolSimulationMode::DryRun, "missing"), None),
        Err(SimulationError::FixtureMissing)
    );
    assert_eq!(
        runtime.simulate(request(ToolSimulationMode::Real, "real"), None),
        Err(SimulationError::RealModeNotIntercepted)
    );
    assert_eq!(runtime.completed_count(), 0);
}
