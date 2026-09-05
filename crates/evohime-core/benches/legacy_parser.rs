use criterion::{black_box, criterion_group, criterion_main, Criterion};
use evohime_core::visible_agent_text;

fn visible_text_benchmark(c: &mut Criterion) {
    let response =
        "Проверяю проект. <function_calls>[{\"name\":\"filesystem.list\"}]</function_calls>";
    c.bench_function("legacy_visible_agent_text", |benchmark| {
        benchmark.iter(|| visible_agent_text(black_box(response)))
    });
}

criterion_group!(benches, visible_text_benchmark);
criterion_main!(benches);
