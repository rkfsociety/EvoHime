# План 16.1. Definition, trigger и run contract

## Цель

Описать versioned модель автоматизации, которая одинаково работает для ручного,
расписанного, повторного и simulation запуска.

## Изменения

- Ввести `AutomationDefinition` с stable id/revision, bounded steps,
  capability references, schedule/trigger policy, owner scope и retention.
- Разделить trigger request, accepted run, activity event, health snapshot и
  terminal outcome; UI получает read-only projection.
- Сделать обязательными idempotency key, correlation id, permission snapshot,
  approval policy snapshot и generation при создании run.
- Описать cancellation, pause/resume, retry и typed provider/supervisor errors;
  повторная доставка события должна быть безопасной.
- Ограничить число шагов, payload, history и parallelism; arbitrary child-agent
  graph не является частью definition.

## Проверки

- schema/version/size validation и отказ неизвестных опасных полей;
- duplicate trigger с тем же idempotency key;
- одинаковая definition с разными revision и scope;
- approval/permission snapshot фиксируется для run и не подменяется UI;
- projection не содержит секретов или неограниченного provider output.

## Готово, когда

Каждый run имеет stable identity, bounded definition, immutable launch
snapshots и typed lifecycle, а trigger можно безопасно повторить после timeout
или reconnect.

## Нормативный contract

- `AutomationDefinitionV1` separates immutable graph/schedule/capability
  references from runtime policy snapshots. IDs are <=128 bytes, graph <=64
  activities, input <=64 KiB, history/projection <=256 events; unknown major or
  unsafe fields fail closed. Revision is monotonic per definition and a run
  binds permanently to `(definition_id, revision, owner_scope)`.
- Run lifecycle is `admitted|queued|starting|running|waiting_approval|paused|
  retrying|cancelling|completed|failed|cancelled|dead_letter`; only Core may
  transition it. Concurrent transition uses generation compare-and-set;
  pause/complete/cancel races resolve by terminal priority.
- Trigger idempotency key `(owner_scope, definition_id, revision, trigger_key,
  scheduled_slot)` is retained 30 days. Same key and same payload returns the
  existing run; same key with another payload returns typed
  `idempotency_conflict`. Timeout/reconnect never creates a second run.
- Activity events carry event ID, run generation, sequence, activity ID,
  attempt, outcome and redacted diagnostics. Projection is derived from the
  durable event source, ordered by sequence and deduplicated by event ID;
  missing sequence yields a typed replay gap.
- Retryable errors are bounded provider/storage timeouts and supervisor
  unavailable; validation, permission/approval denial, unsupported capability,
  schema and terminal failures are non-retryable. Simulation creates a full
  derived run in ephemeral storage with no production effects.
