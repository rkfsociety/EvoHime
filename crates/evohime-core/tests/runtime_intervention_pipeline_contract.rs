use evohime_core::agent_middleware_pipeline::*;

#[test]
fn all_issue_hook_boundaries_have_typed_phases_and_ordering() {
    assert!(HookPhase::ALL.contains(&HookPhase::BeforeHandoff));
    assert!(HookPhase::ALL.contains(&HookPhase::BeforeWorkflowStateCommit));
    assert!(HookPhase::ALL.contains(&HookPhase::BeforeExternalPublish));
    let definition = PipelineDefinition::new(
        "ordered",
        1,
        vec![
            MiddlewareSpec {
                id: "z-last".into(),
                version: 1,
                priority: 10,
                phases: vec![HookPhase::BeforeTool],
                state_class: StateClass::Public,
                policy: BuiltinPolicy::Observe,
                mode: HandlerMode::ObserveOnly,
                failure_policy: FailurePolicy::FailOpen,
            },
            MiddlewareSpec {
                id: "a-first".into(),
                version: 1,
                priority: 1,
                phases: vec![HookPhase::BeforeTool],
                state_class: StateClass::Public,
                policy: BuiltinPolicy::Observe,
                mode: HandlerMode::ObserveOnly,
                failure_policy: FailurePolicy::FailClosed,
            },
        ],
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
        phase: HookPhase::BeforeTool,
        input_hash: "hash".into(),
        capability_snapshot_hash: "c".into(),
        intervention_depth: 0,
    };
    let (_, events) = service.evaluate(&request).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
}
