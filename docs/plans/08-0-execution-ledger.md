# План 08 — Core-owned execution ledger и typed receipts

## Цель

Завершить единый Core-owned журнал выполнения поверх существующих SQLite
events, signed receipts и model-request provenance. UI, supervisor, diagnostics,
memory и evaluation должны видеть одну воспроизводимую историю действий, а не
набор несвязанных mutable projections.

## Что уже есть в checkout

- append-only `events` (`sequence_id INTEGER PRIMARY KEY AUTOINCREMENT`,
  `task_id`, `event_type`, `payload`, `created_at`) с канонической глобальной
  `sequence_id` и bounded replay;
- `core_instance_id`/`session_epoch` в `EventEnvelope`, `ReplayGap`
  (`requested_after_sequence`, `earliest_available_sequence`,
  `latest_available_sequence`, `reason`) и `FullSnapshot`
  (`sequence_id`, `snapshot_json`) в IPC, но пока без typed execution
  projection;
- signed `receipts_v1` (`receipt_actions`, `receipt_records`,
  `receipt_approval_intents`, chain heads и checkpoints) для execution/approval
  audit и отдельный model-request provenance;
- durable workflow runtime 06 со своими `workflow_run_events.run_sequence`,
  `workflow_runs`/`workflow_run_nodes`/`workflow_node_attempts`, lease и
  recovery;
- готовый словарь состояний в
  `crates/evohime-local-storage/src/workflow_store.rs`: run-level `RunState`
  (`pending`, `running`, `waiting_approval`, `completed`, `failed`,
  `cancelled`, `degraded`, `interrupted`) и action-level `NodeState`
  (включая `waiting_approval`, `unknown_outcome`, `dead_letter`); оба
  множества дополнительно зафиксированы CHECK-ограничениями в
  `workflow_runs.state` и `workflow_run_nodes.state`;
- recovery-механика в общей schema (`crates/evohime-local-storage/src/lib.rs`):
  dispatch marker и идемпотентность в `run_effects.idempotency_key`, решения
  recovery в `run_recovery`/`run_reconciliations`, аренды в `run_leases`;
- два существующих пространства run-идентичности: `runs.id` (run рабочего
  элемента в общей schema) и `workflow_runs.run_id` (workflow runtime 06); они
  не объединены и могут пересекаться по значению;
- `interrupted`/`unknown_outcome` уже применяются в model provenance
  (`crates/evohime-local-storage/src/model_provenance.rs`) и в workflow
  runtime;
- текущая общая SQLite schema v29 при минимально поддерживаемой для миграции
  v26 (`LEGACY_SCHEMA_VERSION`), Core/SQLite как durable source of truth.

Нет единой typed execution projection поверх `events`, нет устойчивого
`event_id` и `run_id`/`session_id` в глобальном журнале, нет атомарной
публикации «событие + переход проекции», нет linkage `workflow_run_events` ↔
глобальное событие и нет typed snapshot action-проекции в IPC.

Новый план не заменяет эти контракты и не создаёт вторую базу данных. Ledger
становится канонической глобальной последовательностью typed execution events.
`workflow_run_events.run_sequence` остаётся bounded per-run projection и должен
ссылаться на глобальное событие, а не конкурировать с `events.sequence_id`.
`receipts_v1`, model provenance и context ledgers остаются доменными
immutable-слоями, связанными по IDs и hashes. Новая терминология состояний и
recovery не вводится там, где уже есть рабочая: ledger переиспользует словарь
`RunState`/`NodeState` и идемпотентность `run_effects`.

## Границы

Входит: устойчивые `run_id`, логический `session_id`, `event_id`, typed
action/tool/observation/receipt/failure events, атомарные state transitions,
legacy mapping, redaction, reconnect/replay, bounded snapshot и Core startup
recovery.

Не входит: UI как durable state, произвольный unversioned JSON, новая база,
model-generated authority для опасных действий, внешний telemetry backend,
новый runtime и переименование уже используемых состояний workflow.

## Зависимости

### Блокирующие

- текущие `EventJournal`, `LocalDatabase` (schema v29), workflow storage,
  authenticated desktop IPC и schema migration/backup path;
- текущий durable workflow runtime 06: его run/node/attempt state, `run_effects`
  и recovery должны сохранить compatibility и получить linkage на ledger events;
- существующие `receipts_v1`, approval policy и model provenance; ledger не
  переписывает их таблицы без явной cross-reference migration.

### Опциональные

- план 07 и его telemetry могут использовать новую projection после её
  появления, но не блокируют выполнение плана 08; до их появления ledger
  сохраняет только собственные bounded поля, а tool identity берётся из
  текущего `ToolRegistry`;
- общий evaluation harness (`crates/evohime-core/src/evals.rs`,
  `tests/evals/`) и telemetry из 07-4 используются при наличии, иначе план
  поставляется со своими deterministic fixtures.

## Этапы

- [08-1 — versioned ledger contract](08-1-ledger-contract.md)
- [08-2 — SQLite storage и recovery](08-2-ledger-storage-and-recovery.md)
- [08-3 — IPC replay и Core projection](08-3-ledger-ipc-and-projection.md)
- [08-4 — acceptance и закрытие](08-4-ledger-acceptance.md)

Порядок: 08-1 → 08-2 → 08-3 → 08-4.

## Готово, когда

Каждый новый action имеет начало и ровно один terminal outcome (typed receipt,
typed failure, cancellation или explicit `unknown_outcome`); запись и переход
проекции атомарны; legacy rows доступны через детерминированный mapping; replay
сохраняет глобальный порядок и сообщает gap по `sequence_id`, а смена
`core_instance_id`/`session_epoch` не смешивает поколения projection; restart
не создаёт blind retry; renderer получает только bounded redacted projection и
не может менять durable ledger напрямую.
