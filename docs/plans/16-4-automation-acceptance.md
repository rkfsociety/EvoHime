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

## Нормативная acceptance matrix

Каждая строка выполняется на clean checkout и повторно после supervisor/Core
restart; evidence содержит test ID, input fixture, фактический outcome,
durable event sequence и соответствующий log/metric artifact.

| Test ID | Сценарий | Pass/fail критерий |
|---|---|---|
| A01 | manual/scheduled trigger, timezone и missed tick | ровно один run на idempotency key, bounded latency ≤2 s |
| A02 | duplicate trigger/tick и overlap | существующий run возвращается, новый effect не создаётся |
| A03 | stale lease/takeover и Core restart | старый generation получает `stale_generation`, новый owner восстанавливает run |
| A04 | cancel + restart, provider timeout/failure | cooperative cancel ≤5 s, максимум 2 retry, terminal typed outcome |
| A05 | snapshot/diff/replay и migration | одинаковый fixture даёт одинаковый replay hash; incompatible schema получает typed-block |
| A06 | simulation filesystem/network/host/credentials | каждый запрещённый effect fail-closed с `simulation_external_effect`, production state неизменен |
| A07 | archive/retention/redaction/restore | active и archive транзакционно согласованы, secrets/paths удалены, лимиты 64/10 000/30 дней соблюдены |
| A08 | IPC projection и optional adapters 13–15 | projection sequence не теряет durable events; отсутствие optional adapter даёт typed `backend_unavailable`, не нарушая Core contract |

## Release gate и evidence

- Release blocker — любой failed/missing A01–A08, security finding severity
  `high`/`critical`, потеря durable event, production side effect в simulation,
  повторный effect после restart или нарушение backup/restore. `medium` и `low`
  допускаются только с зафиксированным follow-up.
- Evidence bundle обязан содержать acceptance report с test IDs и hashes,
  sanitized Core/supervisor logs, metrics snapshot, IPC compatibility result,
  migration backup/restore checksum, simulation guard audit, security findings
  с severity и rollback report.
- Rollback считается успешным, если из backup восстанавливаются schema и runs,
  checksum совпадает, terminal/history events не теряются, active effects не
  повторяются, а IPC остаётся совместимым с предыдущей major-версией. Rollback
  проверяется на supervisor restart и требует audit event.
- Deterministic fixture фиксирует frozen clock/timezone, RNG seed, definition
  revision, ordered inputs/events, provider responses, retry/backoff и
  capability/policy snapshot; replay запускается минимум дважды и сравнивает
  hash и redacted history.
- `launch` означает момент передачи run из admitted/queued в starting; именно
  тогда и непосредственно перед каждым effect проверяются policy и approval.
  `bounded history` — не более 256 durable events/run, snapshot chain — не более
  64, archive — не более 10 000 runs за 30 дней.

## Готово, когда

Матрица A01–A08 пройдена без release blockers, каждый результат имеет
воспроизводимый evidence artifact, все критерии pass/fail измеримы, simulation
имеет fail-closed техническое подтверждение, а rollback, migration, security,
IPC compatibility и Core ownership подтверждены отчетом.
