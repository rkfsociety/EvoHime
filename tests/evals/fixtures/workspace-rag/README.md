# Workspace RAG fixtures v1

Локальное воспроизведение из корня репозитория:

```powershell
cargo run -p evohime-eval -- --fixture tests/evals/fixtures --case workspace-rag-lexical-001 --mode deterministic --verbose
cargo test -p evohime-core workspace_rag::tests
```

Baseline зафиксирован на synthetic corpus unit-тестов `workspace_rag::tests`:
два текстовых документа, 1–4 chunks, Windows x64, SQLite bundled FTS5. Для
малого corpus lexical p99 gate равен 500 ms, hybrid p99 gate — не более 2x
lexical. Качество задаётся обязательным попаданием ожидаемого документа в
top-3; hybrid не может потерять lexical candidate и автоматически остаётся в
режиме FTS5 при несовместимом/недоступном vector index.

Исполняемый gate находится в тесте
`vector_publication_is_atomic_and_hybrid_has_bounded_rrf_explanation`: 24
одинаковых lexical/hybrid запросов измеряются через monotonic clock в
наносекундах на одном трёхдокументном snapshot, P99 hybrid ограничен
`2 * P99(FTS5)`, а NDCG@3 и precision@3 hybrid обязаны быть не ниже FTS5.
Ожидаемый документ остаётся первым в обоих режимах. Публикация vector index, fallback и отсутствие
vector-only утечек за scope проверяются тем же тестовым модулем.
