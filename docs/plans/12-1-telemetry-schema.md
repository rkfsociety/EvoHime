# 12-1 — Telemetry schema и metrics

## Цель

Зафиксировать bounded correlation и метрики полного model/tool execution path.

## Изменения

1. Ввести versioned projection `run → model/tool → result/error` с `trace_id`,
   `task_id`, `run_id`, `workflow_run_id`, `node_id`, `tool_call_id`, attempt,
   manifest hash и model request ID.
2. Сохранять token counts, estimated cost, latency, timeout, cancellation,
   retry count, output size, remaining budget и degradation reason.
3. Привязать approval/policy decision, action/observation/receipt и
   provider/model revision к одному report provenance chain.
4. Исключить secrets, headers, raw prompt, full tool output и unbounded
   payload; применять существующие redaction/retention limits.
5. Разделить advisory judge signal и deterministic release-gate result.

## Проверки

- schema/size/redaction fixtures;
- missing/degraded model usage metadata;
- correlation integrity across model/tool/approval events;
- retention and malformed trace typed diagnostic errors.

## Готово, когда

По run можно получить bounded cost/latency/failure view, не раскрывая секреты
и не создавая новый durable source of truth.
