# План 175.3 — Memory Ingestion Integrity: diagnostics, IPC и projection

## Зависимости

### Блокирующие

- [Runtime lifecycle и recovery](./175-2-memory-ingestion-integrity.md).
- Existing Core trace/journal, memory diagnostics, authenticated IPC и
  Electron OperationsPanel projection.

### Опциональные

- Existing support-bundle redaction helpers.

## Реализация

- Добавить metadata-only events для attempted/skipped/reentrant/cooling-down,
  candidate captured/rejected, publish conflict/superseded, finalization
  deferred/recovered/committed/failed с bounded reason codes.
- Расширить existing read-only projection полями состояния extractor,
  finalization backlog, suppressed reentry, conflicts and last failure; raw
  memory content, transcript, prompt, credentials and model output excluded.
- Обновить typed IPC/generated bindings только additive способами; renderer
  отображает Core state и не запускает storage/reconciliation самостоятельно.
- Preserve existing ambient pending/deletion UI and distinguish source/expiry/
  recovery state without exposing unverified content as confirmed fact.

## Критерии

- Projection is replay-safe, bounded and redacted; malformed/unknown major
  payloads fail closed.
- Diagnostics explain skip, stale, lease busy, deferred and failed states
  without implying successful persistence.
- Existing memory controls and forget semantics remain authoritative.

## Non-goals

Новая пользовательская вкладка для внутренних Core contracts и передача
private memory body в renderer.
