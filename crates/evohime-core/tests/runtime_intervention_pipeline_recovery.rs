use evohime_core::agent_middleware_pipeline::*;

#[test]
fn security_decisions_fail_closed_and_reentrant_operations_stop() {
    let definition = PipelineDefinition::new(
        "guard",
        1,
        vec![MiddlewareSpec {
            id: "guard".into(),
            version: 1,
            priority: 0,
            phases: vec![HookPhase::BeforeExternalPublish],
            state_class: StateClass::Checkpoint,
            policy: BuiltinPolicy::Block {
                reason: "blocked".into(),
            },
            mode: HandlerMode::Policy,
            failure_policy: FailurePolicy::FailClosed,
        }],
    )
    .unwrap();
    let snapshot = PipelineRunSnapshot {
        run_id: "run".into(),
        definition_id: definition.definition_id.clone(),
        definition_revision: 1,
        contract_hash: definition.contract_hash.clone(),
        policy_hash: "p".into(),
        capability_snapshot_hash: "c".into(),
    };
    let mut service = AgentMiddlewarePipelineService::new(definition, snapshot, "c").unwrap();
    let request = MiddlewareRequest {
        run_id: "run".into(),
        correlation_id: "corr".into(),
        idempotency_key: "key".into(),
        phase: HookPhase::BeforeExternalPublish,
        input_hash: "hash".into(),
        capability_snapshot_hash: "c".into(),
        intervention_depth: MAX_INTERVENTION_DEPTH + 1,
    };
    assert_eq!(
        service.evaluate(&request).unwrap().0,
        PipelineOutcome::ReentrantLimit
    );
}
