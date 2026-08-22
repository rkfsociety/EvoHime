# 11-3 — Context budget, compaction и projections

## Цель

Управлять context budget и reflection/compaction как cancellable versioned
projection поверх существующего context ledger, без незаметного удаления
истории.

## Что уже есть в checkout

- крейт `evohime-context-budget` (`crates/context-budget`): `ContextPlanner`
  и `ContextPlan`, `ContextItem`, `ladder` с `OffloadSink`/`OffloadOutcome`,
  `BoundedSummarizer` с deterministic fallback и запретом каскадного повтора,
  `ContextLedgerEntry` с `SelectedItemRecord`, `DroppedItemRecord` и
  `CompressionRecord` (`summary_id`, `source_ids`, `compression_ratio`,
  `summarizer_version`, `summary_budget`, `fallback`, `fallback_reason`);
- `crates/evohime-core/src/context_budget.rs` — интеграция в agent loop:
  `ContextRuntime`, `assemble`/`replan`, `record_actual_usage`,
  `ModelContextProjection`, `DroppedItemProjection`, `CompressionProjection`,
  artifact/message offload и summarizers (`deterministic_summarizer`,
  `model_summarizer`, `PrecomputedSummaryModel`);
- `context_ledger_store.rs`: `append`, `record_usage`, `register_receipt`,
  `get`, `find_by_hash`, `projection`, `prune`, `count`;
- `RagLedgerProjection` в `workspace_rag.rs`;
- `compact_chain` в `evohime-receipts` для receipts prefix с checkpoint.

Этап связывает retrieval output с этим ledger и добавляет cancellation,
idempotency и source linkage summaries.

## Зависимости

### Блокирующие

- 11-2: retrieval result с provenance и breakdown;
- крейт `evohime-context-budget`, `context_budget.rs` и
  `context_ledger_store.rs`.

### Опциональные

- provider reflection. Без него используется `deterministic_summarizer`, а
  результат помечается существующими средствами: `CompressionRecord.fallback
  = true`, `fallback_reason` и суффикс `+fallback` в `summarizer_version`;
  новые факты не подтверждаются;
- вектора для derived summary. Без них summary хранится без вектора и
  находится через FTS5.

## Контракт

1. Retrieval output попадает в существующий context plan/ledger: обязательные
   элементы, dropped items и причины pruning видимы в projection.
2. Reflection/compaction cancellable: bounded budget, snapshot revision,
   идемпотентный ключ и deterministic fallback при provider failure. Повтор с
   тем же ключом не создаёт вторую summary. Cancellation в текущем
   summarizer/planner отсутствует и вводится этапом целиком; идемпотентность
   строится на существующем `ContextLedgerStore::find_by_hash`, а не на новом
   каталоге ключей.
3. Отдельный `degraded`-статус не вводится: `LedgerOutcome` остаётся парой
   `sent`/`budget_unavailable`, а деградация summarizer выражается
   `CompressionRecord.fallback`/`fallback_reason`. UI-проекция читает эти
   поля, а не собственный enum.
4. Derived summary сохраняется как versioned projection со ссылками на
   исходные event ID. Сегодня `CompressionRecord.source_ids` — идентификаторы
   `ContextItem`, поэтому этап добавляет отображение item → `sequence_id`
   события ledger и хранит его вместе с summary. Исходные execution/evidence
   events compaction не удаляет; удаление receipts prefix остаётся отдельной
   операцией `compact_chain` с checkpoint. `ContextLedgerStore::prune` не
   должен затрагивать строки, на которые ссылается живая summary.
5. Redaction применяется до memory write и до context projection. Raw prompt
   и полный tool output не становятся durable memory автоматически.
6. UI получает только metadata, score breakdown, citations и bounded
   preview; любая mutation идёт через Core command path.

## Изменения по слоям

- Rust core: cancellation/idempotency в compaction, linkage summary → events;
- storage: versioned projection rows и prune, не затрагивающий источники;
- IPC/Electron: bounded preview и projection replay после reconnect.

## Проверки

- context budget overflow и deterministic pruning;
- compaction cancel, retry, stale snapshot и идемпотентный повтор с тем же
  hash не создаёт вторую summary;
- provider failure даёт `fallback = true` с причиной и без потери items;
- у каждой summary есть ссылки на исходные `sequence_id`, и `prune` их не
  удаляет;
- reconnect/replay projection после Core restart;
- `cargo test --locked -p evohime-core -p evohime-context-budget
  -p evohime-local-storage`, Electron `npm test`.

## Готово, когда

Compaction не теряет происхождение данных, соблюдает budget и cancellation и
не может скрыто удалить исходные execution или evidence events.
