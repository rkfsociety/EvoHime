use criterion::{black_box, criterion_group, criterion_main, Criterion};
use evohime_core::{task_memory, visible_agent_text, workspace};
use std::path::Path;

fn visible_text_benchmark(c: &mut Criterion) {
    let response =
        "Проверяю проект. <function_calls>[{\"name\":\"filesystem.list\"}]</function_calls>";
    c.bench_function("legacy_visible_agent_text", |benchmark| {
        benchmark.iter(|| visible_agent_text(black_box(response)))
    });
}

fn content_hash_benchmark(c: &mut Criterion) {
    let content = vec![b'x'; 64 * 1024];
    c.bench_function("workspace_content_hash", |benchmark| {
        benchmark.iter(|| workspace::content_hash(black_box(&content)))
    });
}

fn project_scope_benchmark(c: &mut Criterion) {
    let workspace_root = Path::new(r"D:\github\EvoHime");
    c.bench_function("task_memory_project_scope_id", |benchmark| {
        benchmark.iter(|| task_memory::workspace_scope_id(black_box(workspace_root)))
    });
}

criterion_group!(
    benches,
    visible_text_benchmark,
    content_hash_benchmark,
    project_scope_benchmark
);
criterion_main!(benches);
