use evohime_core::tool_simulation_runtime::{
    fixture_for, SimulationRequest, ToolSimulationMode, ToolSimulationRuntime,
};
use serde_json::json;

#[test]
fn duplicate_delivery_is_idempotent_and_restart_discards_ephemeral_state() {
    let input = json!({"value":"replay"});
    let mut runtime = ToolSimulationRuntime::default();
    runtime
        .register_fixture(fixture_for("fixture.echo", &input))
        .unwrap();
    let request = SimulationRequest {
        schema_version: 1,
        run_id: "recovery-run".into(),
        tool_id: "fixture.echo".into(),
        mode: ToolSimulationMode::DryRun,
        input,
        emulated_output: None,
        correlation_id: "recovery-correlation".into(),
        idempotency_key: "recovery-idem".into(),
        policy_hash: "policy".into(),
        capability_granted: true,
    };
    let first = runtime.simulate(request.clone(), None).unwrap();
    let duplicate = runtime.simulate(request, None).unwrap();
    assert_eq!(first, duplicate);
    assert_eq!(runtime.completed_count(), 1);
    let restarted = ToolSimulationRuntime::default();
    assert_eq!(restarted.completed_count(), 0);
    assert_eq!(restarted.fixture_count(), 0);
}
