# 11-3 — Context budget, compaction и projections

## Цель

Управлять context budget и reflection/compaction как cancellable versioned
projection поверх существующего context ledger, без незаметного удаления
истории.

## Что уже есть в checkout

- `context_budget.rs`: `ContextRuntime`, `assemble`/`replan`,
  `record_actual_usage`, `ModelContextProjection`, `DroppedItemProjection`,
  `CompressionProjection`, artifact/message offload и summarizer
  (`deterministic_summarizer`, `model_summarizer`, `PrecomputedSummaryModel`);
- `context_ledger_store.rs`: `append`, `record_usage`, `register_receipt`,
  `projection`, `prune`;
- `RagLedgerProjection` в `workspace_rag.rs`;
- `compact_chain` в `evohime-receipts` для receipts prefix.

Этап связывает retrieval output с этим ledger и добавляет cancellation,
idempotency и source linkage summaries.

## Зависимости

### Блокирующие

- 11-2: retrieval result с provenance и breakdown;
- текущие `context_budget.rs` и `context_ledger_store.rs`.

### Опциональные

- provider reflection. Без него используется `deterministic_summarizer`, а
  результат помечается `degraded`; новые факты не подтверждаются;
- embeddings для derived summary. Без них summary хранится без вектора и
  находится через FTS5.

## Контракт

1. Retrieval output попадает в существующий context plan/ledger: обязательные
   элементы, dropped items и причины pruning видимы в projection.
2. Reflection/compaction cancellable: bounded budget, snapshot revision,
   идемпотентный ключ и deterministic fallback при provider failure. Повтор с
   тем же ключом не создаёт вторую summary.
3. Derived summary и embedding сохраняются как versioned projection со
   ссылками на исходные event ID. Исходные execution/evidence events
   compaction не удаляет; удаление receipts prefix остаётся отдельной
   операцией `compact_chain` с checkpoint.
4. Redaction применяется до memory write и до context projection. Raw prompt
   и полный tool output не становятся durable memory автоматически.
5. UI получает только metadata, score breakdown, citations и bounded
   preview; любая mutation идёт через Core command path.

## Изменения по слоям

- Rust core: cancellation/idempotency в compaction, linkage summary → events;
- storage: versioned projection rows и prune, не затрагивающий источники;
- IPC/Electron: bounded preview и projection replay после reconnect.

## Проверки

- context budget overflow и deterministic pruning;
- compaction cancel, retry, stale snapshot и идемпотентный повтор;
- provider failure даёт `degraded`/`unknown` без потери items;
- у каждой summary есть ссылки на исходные event ID;
- reconnect/replay projection после Core restart;
- `cargo test --locked -p evohime-core -p evohime-local-storage`,
  Electron `npm test`.

## Готово, когда

Compaction не теряет происхождение данных, соблюдает budget и cancellation и
не может скрыто удалить исходные execution или evidence events.
