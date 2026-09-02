use evohime_core::agent_middleware_pipeline::*;

fn service(policy: BuiltinPolicy) -> AgentMiddlewarePipelineService {
    let definition = PipelineDefinition::new(
        "recovery",
        1,
        vec![MiddlewareSpec {
            id: "guard".into(),
            version: 1,
            priority: 1,
            phases: vec![HookPhase::BeforeTool],
            state_class: StateClass::Checkpoint,
            policy,
            mode: HandlerMode::Policy,
            failure_policy: FailurePolicy::FailClosed,
        }],
    )
    .unwrap();
    let snapshot = PipelineRunSnapshot {
        run_id: "run".into(),
        definition_id: definition.definition_id.clone(),
        definition_revision: definition.revision,
        contract_hash: definition.contract_hash.clone(),
        policy_hash: "policy".into(),
        capability_snapshot_hash: "caps".into(),
    };
    AgentMiddlewarePipelineService::new(definition, snapshot, "caps").unwrap()
}

fn request(key: &str) -> MiddlewareRequest {
    MiddlewareRequest {
        run_id: "run".into(),
        correlation_id: "corr".into(),
        idempotency_key: key.into(),
        phase: HookPhase::BeforeTool,
        input_hash: "input-hash".into(),
        capability_snapshot_hash: "caps".into(),
        intervention_depth: 0,
    }
}

#[test]
fn block_is_typed_and_does_not_dispatch_an_effect() {
    let mut pipeline = service(BuiltinPolicy::Block {
        reason: "policy".into(),
    });
    let (outcome, events) = pipeline.evaluate(&request("one")).unwrap();
    assert_eq!(
        outcome,
        PipelineOutcome::Blocked {
            reason: "policy".into()
        }
    );
    assert_eq!(events.len(), 1);
}

#[test]
fn replay_after_crash_is_duplicate_not_retry() {
    let mut pipeline = service(BuiltinPolicy::Observe);
    let first = pipeline.evaluate(&request("one")).unwrap();
    let second = pipeline.evaluate(&request("one")).unwrap();
    assert!(!first.1.is_empty());
    assert_eq!(second, (PipelineOutcome::Duplicate, Vec::new()));
}

#[test]
fn capability_snapshot_drift_is_rejected_before_effect() {
    let mut pipeline = service(BuiltinPolicy::Observe);
    let mut request = request("one");
    request.capability_snapshot_hash = "changed".into();
    assert!(matches!(
        pipeline.evaluate(&request),
        Err(PipelineError::Invalid("request snapshot"))
    ));
}
