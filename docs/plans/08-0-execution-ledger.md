# План 08 — Core-owned execution ledger и typed receipts

## Цель

Завершить единый Core-owned журнал выполнения поверх существующих SQLite
events, signed receipts и model-request provenance. UI, supervisor, diagnostics,
memory и evaluation должны видеть одну воспроизводимую историю действий, а не
набор несвязанных mutable projections.

## Что уже есть в checkout

- append-only `events` с глобальной sequence и bounded replay;
- диагностический replay gap и Core/session revision в IPC envelope;
- signed `receipts_v1` для execution/approval audit;
- recovery и unknown-outcome semantics для model requests;
- Core/SQLite как durable source of truth.

Новый план не заменяет эти контракты и не создаёт вторую базу данных.

## Границы

Входит: устойчивые `run_id`, `session_id`, `event_id`, typed action/tool/
observation/receipt/failure events, атомарные state transitions, redaction,
reconnect/replay и supervisor recovery.

Не входит: UI как durable state, произвольный unversioned JSON, новая база,
model-generated authority для опасных действий, внешний telemetry backend или
новый runtime.

## Зависимости

### Блокирующие

- текущие `EventJournal`, `LocalDatabase`, authenticated desktop IPC и schema
  migration/backup path;
- существующие `receipts_v1`, approval policy и model provenance.

### Опциональные

- планы 06–07 могут использовать новые projections после их появления, но не
  блокируют выполнение плана 08;
- evaluation harness из 06-4 и telemetry из 07-4 используются при наличии,
  иначе план поставляется со своими deterministic fixtures.

## Этапы

- [08-1 — versioned ledger contract](08-1-ledger-contract.md)
- [08-2 — SQLite storage и recovery](08-2-ledger-storage-and-recovery.md)
- [08-3 — IPC replay и Core projection](08-3-ledger-ipc-and-projection.md)
- [08-4 — acceptance и закрытие](08-4-ledger-acceptance.md)

Порядок: 08-1 → 08-2 → 08-3 → 08-4.

## Готово, когда

Каждый action имеет начало и связанный typed receipt либо typed failure; запись
атомарна; replay сохраняет порядок и сообщает gap/stale revision; restart не
создаёт blind retry; renderer получает только bounded redacted projection и не
может менять durable ledger напрямую.
