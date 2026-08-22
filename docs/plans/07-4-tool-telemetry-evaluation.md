# План 07-4 — Tool telemetry, cost view и evaluation

## Цель

Добавить сквозную, bounded и replayable диагностику model/tool execution,
чтобы видеть стоимость, задержки, retries, approval и причины деградации без
сохранения секретов или неограниченного prompt/output.

## Зависимости

### Блокирующие

- [07-1](07-1-tool-manifest-contract.md) для tool identity/capability;
- [07-3](07-3-action-console.md) для approval lifecycle;
- существующие model-request provenance, receipts, event journal,
  `run_policy` и Operations Panel.

### Опциональные

- [07-2](07-2-toolkit-catalog-lifecycle.md) для catalog version/source
  metadata. Без него telemetry использует manifest hash;
- [12](12-0-telemetry-and-evaluation.md) как общая telemetry-схема. 07-4 не
  вводит собственный формат журнала и retention-политику: он остаётся
  tool-focused проекцией поверх существующих provenance/`EventJournal`, а
  12-1 позже поглощает его поля без переименования;
- [08](08-0-execution-ledger.md) как Core-owned execution ledger. Telemetry не
  заводит собственный журнал: до появления 08 она пишет bounded события в уже
  существующий `EventJournal` и provenance, а после — становится проекцией
  ledger-событий без смены correlation fields;
- общий deterministic evaluation harness (`crates/evohime-core/src/evals.rs`,
  `tests/evals/`), уже покрывающий workflow orchestration. Сценарии из
  пункта 6 добавляются в этот общий harness с отдельными fixture IDs и не
  требуют отдельного workflow-evaluation этапа.

## Что уже есть в коде

- `crates/evohime-core/src/observability.rs` задаёт bounded redacted hook
  events (`before_context`, `before_tool`, `after_tool`, `before_commit`,
  `after_task`) с лимитами на количество полей, длину значений и размер
  события;
- `crates/evohime-core/src/run_policy.rs` уже считает `RunUsage` (итерации,
  wall clock, tool calls, tokens, `cost_micros`) и даёт `RunStopReason`;
- `crates/evohime-model-provenance` фиксирует `request_id`,
  `logical_request_id`, `parent_request_id` и `attempt`;
- Operations Panel показывает лимиты запуска (`max_tokens`,
  `max_time_seconds`, `max_tool_calls`).

Нет сводки по фактическому расходу на уровне запуска и шага, нет единого
correlation-набора между tool call, approval и model request, нет export/replay
проекции telemetry и нет evaluation suite для перечисленных ниже сценариев.

## Изменения

1. Зафиксировать correlation fields поверх уже существующих идентификаторов:
   `task_id`, `run_id`, `workflow_run_id`, `node_id`, `tool_call_id`,
   `attempt`, `manifest_hash`, а также `request_id`/`logical_request_id` из
   model provenance. Новые синонимы для уже существующих полей не вводятся.
2. Добавить durable metadata для model/tool lifecycle:
   started, waiting approval, approved/rejected, dispatched, succeeded,
   failed, cancelled, timed out, retried и policy-denied.
3. Сохранять bounded metrics:
   duration, token counts, estimated cost, retry count, output size, budget
   remaining, error class и degradation reason. Секреты, headers, raw prompt и
   full tool output исключить, переиспользуя существующие лимиты и redaction
   из `observability.rs`.
4. Расширить Operations Panel сводкой по запуску и шагам: calls, tokens,
   cost, latency, approval wait, failures, retries и policy decisions.
5. Добавить export/replay projection, совместимую с существующим JSONL/event
   journal и provenance verifier.
6. Добавить deterministic evaluation scenarios:
   valid manifest, invalid schema, approval approve/reject/expiry, retry,
   cancellation, restart recovery, duplicate IPC delivery, hash mismatch,
   secret redaction и capability escalation.

## Проверки

- schema/size/redaction tests для каждого event;
- correlation and replay tests across restart;
- budget/cost aggregation tests with missing or degraded model metadata;
- сверка агрегата telemetry с `RunUsage`: расхождение считается ошибкой;
- evaluation suite с ожидаемыми terminal states;
- `cargo fmt --check`, targeted `cargo test -p evohime-core` и Electron
  telemetry/UI tests (`npm run typecheck`, `npm test`).

## Готово, когда

По любому запуску можно восстановить bounded цепочку model/tool/approval
событий, объяснить расход бюджета и причину остановки, а evaluation suite
доказывает, что telemetry не меняет policy и не раскрывает secrets.
