use evohime_core::workflow_optimization_lab::*;
fn run() -> OptimizationRun {
    let mut r = OptimizationRun {
        id: "r".into(),
        base_strategy_hash: "b".into(),
        benchmark_suite_hash: "s".into(),
        objective: Objective {
            quality_weight: 1,
            cost_weight: 1,
            latency_weight: 1,
        },
        constraints: vec![],
        rounds: 2,
        state: RunState::Draft,
        policy_hash: "p".into(),
        content_hash: String::new(),
    };
    let mut c = r.clone();
    c.content_hash.clear();
    r.content_hash = hash(&c).unwrap();
    r
}
#[test]
fn contract_is_bounded_and_promotion_is_explicit() {
    assert!(validate_run(&run()).is_ok());
    assert!(promotion_allowed(
        &run(),
        &Candidate {
            id: "c".into(),
            parent_hash: "p".into(),
            mutations: serde_json::json!({}),
            version: 1,
            security_rejected: false,
            content_hash: hash(&Candidate {
                id: "c".into(),
                parent_hash: "p".into(),
                mutations: serde_json::json!({}),
                version: 1,
                security_rejected: false,
                content_hash: String::new()
            })
            .unwrap()
        },
        true,
        true
    )
    .is_ok());
}
