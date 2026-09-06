use criterion::{black_box, criterion_group, criterion_main, Criterion};
use evohime_context_budget::{
    ContextItemBuilder, ContextPlanner, HeuristicEstimator, ItemKind, OwnedContent, PlanInput,
    PlanRequest,
};
use evohime_core::workflow::WorkflowGraph;
use evohime_desktop_ipc::generated;
use prost::Message;
use std::sync::Arc;

fn context_request() -> PlanRequest {
    let make_input = |id: &str, kind: ItemKind, text: &str| {
        let item = ContextItemBuilder::new(id, kind, "")
            .task("bench-task", "bench-session")
            .priority(100)
            .created_at(1)
            .build();
        PlanInput::new(item, OwnedContent::text(text))
    };
    PlanRequest {
        task_id: "bench-task".into(),
        session_id: "bench-session".into(),
        model_call_id: "bench-call".into(),
        provider: "literouter".into(),
        model: "gpt-4o-mini".into(),
        provider_window: None,
        now: 1_000,
        inputs: vec![
            make_input("policy", ItemKind::SafetyPolicy, "не раскрывать секреты"),
            make_input(
                "prompt",
                ItemKind::UserPrompt,
                "проверь изменения в репозитории",
            ),
        ],
        loadout: None,
        replan_of: None,
        force_reduction: false,
    }
}

fn context_planner_benchmark(c: &mut Criterion) {
    let request = context_request();
    let mut planner = ContextPlanner::with_builtin_catalog(Some(Arc::new(
        HeuristicEstimator::default_for("bench-model"),
    )));
    c.bench_function("context_planner_plan", |benchmark| {
        benchmark.iter(|| planner.plan(black_box(&request)))
    });
}

fn workflow_hash_benchmark(c: &mut Criterion) {
    let graph: WorkflowGraph = serde_json::from_value(serde_json::json!({
        "graph_id": "bench",
        "version": 1,
        "entry_node": "a",
        "nodes": [{
            "id": "a",
            "node_type": {"type": "transform"},
            "inputs": [],
            "outputs": [],
            "execution": {
                "retry": {"max_attempts": 1, "backoff_ms": 0},
                "timeout_ms": 1000,
                "cancellation": "cooperative",
                "approval": {"required": false}
            }
        }],
        "edges": []
    }))
    .expect("valid workflow fixture");
    c.bench_function("workflow_canonical_hash", |benchmark| {
        benchmark.iter(|| black_box(&graph).canonical_hash())
    });
}

fn ipc_envelope_benchmark(c: &mut Criterion) {
    let envelope = generated::CommandEnvelope {
        protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
        request_id: "bench-request".into(),
        client_id: "bench-client".into(),
        core_instance_id: "bench-core".into(),
        session_epoch: 1,
        command: None,
    };
    c.bench_function("ipc_command_envelope_encode", |benchmark| {
        benchmark.iter(|| black_box(&envelope).encode_to_vec())
    });
}

criterion_group!(
    benches,
    context_planner_benchmark,
    workflow_hash_benchmark,
    ipc_envelope_benchmark
);
criterion_main!(benches);
