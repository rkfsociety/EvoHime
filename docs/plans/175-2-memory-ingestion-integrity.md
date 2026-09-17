# План 175.2 — Memory Ingestion Integrity: runtime lifecycle и recovery

## Зависимости

### Блокирующие

- [Core contract и storage](./175-1-memory-ingestion-integrity.md).
- `core_agent_memory.rs`, `memory_extraction` gates, ambient episode close,
  existing cancellation, background execution and restart reconciliation.

### Опциональные

- Existing model/policy execution context builder.

## Реализация

- Перед expensive transcript/context work выполнять origin eligibility,
  reentrancy/depth, cooldown, already-processed и budget gates.
- Запускать extractor в restricted context: suppress recursive extraction and
  recall as configured, no arbitrary shell/MCP/filesystem/tools, explicit
  parent/root execution refs; unsupported isolation fail-closed.
- Разделить capture, validation/governance и final reconciliation. Reply may be
  delivered without waiting for non-critical finalizer only when unfinished
  work is durable queued or typed deferred/failed.
- На restart reconcile unfinished leases/candidates/finalizations by basis and
  idempotency; do not replay recovery as new user evidence, do not retry a
  committed effect, and fence late/old generations.
- Apply attempt-level throttling/circuit and bounded retries; extraction
  failure remains observable and does not fail the primary user run.

## Критерии

- Primary user turn, ambient, delegated, background and recovery origins follow
  explicit policy; delegated/background extraction is suppressed by default.
- Concurrent old/new finalizers resolve deterministically without stale
  overwrite or duplicate semantic memory.
- Crash between reply/capture/publish leaves inspectable durable state and
  bounded recovery outcome.

## Non-goals

Ускорение модели, новый scheduler, автоматическое подтверждение candidate и
изменение ambient privacy/retention policy вне необходимого lifecycle contract.
