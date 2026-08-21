# План 16.4. Приёмка automation и simulation

## Цель

Закрыть acceptance matrix для ручных, расписанных, повторных, отменённых,
восстановленных и simulation runs.

## Изменения

- Собрать deterministic fixtures для duplicate trigger, overlap, stale lease,
  restart, cancellation, provider failure, archive и replay.
- Проверить Core/IPC projection, policy/approval at launch, idempotency,
  backpressure, bounded history и redaction.
- Добавить schedule checks: timezone/clock policy, missed tick, duplicate tick,
  disable/delete и restart без повторного side effect.
- Провести security review: capability scope, credentials, egress, host action,
  child-agent limits и simulation separation.
- Зафиксировать rollback и release evidence, включая миграции SQLite и отказ
  optional adapters планов 13–15.

## Проверки

- полный matrix на clean checkout и после supervisor/Core restart;
- replay equality и отсутствие production writes/network calls в simulation;
- IPC compatibility, `git diff --check`, schema migration/backup/restore;
- проверка archive retention, audit completeness и typed diagnostics.

## Готово, когда

Все run paths bounded, cancellable, idempotent и recoverable; schedule не
дублирует side effect; simulation детерминирована и безопасна; release evidence
подтверждает Core ownership, policy enforcement и rollback.

