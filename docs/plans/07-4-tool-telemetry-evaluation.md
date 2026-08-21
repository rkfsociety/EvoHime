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
- [06-4](06-4-workflow-acceptance.md) для общего workflow evaluation harness.

## Изменения

1. Зафиксировать correlation fields:
   `trace_id`, `task_id`, `run_id`, `workflow_run_id`, `node_id`, `tool_call_id`,
   `attempt`, `manifest_hash` и `model_request_id`.
2. Добавить durable metadata для model/tool lifecycle:
   started, waiting approval, approved/rejected, dispatched, succeeded,
   failed, cancelled, timed out, retried и policy-denied.
3. Сохранять bounded metrics:
   duration, token counts, estimated cost, retry count, output size, budget
   remaining, error class и degradation reason. Секреты, headers, raw prompt и
   full tool output исключить.
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
- evaluation suite с ожидаемыми terminal states;
- `git diff --check`, targeted Rust tests и Electron telemetry/UI tests.

## Готово, когда

По любому запуску можно восстановить bounded цепочку model/tool/approval
событий, объяснить расход бюджета и причину остановки, а evaluation suite
доказывает, что telemetry не меняет policy и не раскрывает secrets.
