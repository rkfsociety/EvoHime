# План 08 — Core-owned execution ledger и typed receipts

## Цель

Завершить единый Core-owned журнал выполнения поверх существующих SQLite
events, signed receipts и model-request provenance. UI, supervisor, diagnostics,
memory и evaluation должны видеть одну воспроизводимую историю действий, а не
набор несвязанных mutable projections.

## Что уже есть в checkout

- append-only `events` с канонической глобальной `sequence_id` и bounded replay;
- `core_instance_id`/`session_epoch`, `ReplayGap` и `FullSnapshot` в IPC, но
  пока без typed execution projection;
- signed `receipts_v1` для execution/approval audit и отдельный model-request
  provenance;
- durable workflow runtime 06 со своими `workflow_run_events.run_sequence`,
  lease и recovery;
- текущая общая SQLite schema v29, Core/SQLite как durable source of truth.

Новый план не заменяет эти контракты и не создаёт вторую базу данных. Ledger
становится канонической глобальной последовательностью typed execution events.
`workflow_run_events.run_sequence` остаётся bounded per-run projection и должен
ссылаться на глобальное событие, а не конкурировать с `events.sequence_id`.
`receipts_v1`, model provenance и context ledgers остаются доменными
immutable-слоями, связанными по IDs и hashes.

## Границы

Входит: устойчивые `run_id`, логический `session_id`, `event_id`, typed
action/tool/observation/receipt/failure events, атомарные state transitions,
legacy mapping, redaction, reconnect/replay, bounded snapshot и Core startup
recovery.

Не входит: UI как durable state, произвольный unversioned JSON, новая база,
model-generated authority для опасных действий, внешний telemetry backend или
новый runtime.

## Зависимости

### Блокирующие

- текущие `EventJournal`, `LocalDatabase` (schema v29), workflow storage,
  authenticated desktop IPC и schema migration/backup path;
- текущий durable workflow runtime 06: его run/node/attempt state и recovery
  должны сохранить compatibility и получить linkage на ledger events;
- существующие `receipts_v1`, approval policy и model provenance; ledger не
  переписывает их таблицы без явной cross-reference migration.

### Опциональные

- план 07 и его telemetry могут использовать новую projection после её
  появления, но не блокируют выполнение плана 08;
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
typed failure, cancellation или explicit unknown outcome); запись и переход
проекции атомарны; legacy rows доступны через детерминированный mapping; replay
сохраняет глобальный порядок и сообщает gap по `sequence_id`, а смена
`core_instance_id`/`session_epoch` не смешивает поколения projection; restart
не создаёт blind retry; renderer получает только bounded redacted projection и
не может менять durable ledger напрямую.
