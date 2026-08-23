# План 16. Workflow, automation и длительные simulation jobs

## Цель

Добавить длительные, повторяемые и расписанные операции, оставив scheduler,
state machine, durable history и policy в Rust Core. UI только изменяет
definition и показывает projection; simulation не получает production side
effects.

## Что уже есть в checkout

В checkout уже есть Core-owned task/child-workflow projections, leases,
checkpoint/dead-letter recovery и schedule-related projection. Отдельный
контракт `AutomationDefinition` с полноценными idempotent runs, activity log,
health и deterministic simulation replay должен быть выделен явно и не должен
дублировать существующие child-workflow guarantees.

## Границы

Входит Core-owned input queue и single-owner state machine, generation/lease
protection, разделение tick/step и high-frequency messages от durable state,
operation lock для async provider calls, snapshots/diffs, history separation,
trigger/run/activity log/health/cancellation, idempotency key и permission /
approval snapshot на каждый запуск.

Не входит неконтролируемая multi-agent autonomy, scheduler в renderer,
production side effects из simulation и unbounded child-agent chain.

## Зависимости

**Блокирующие:** планы 08–12 для ledger, policy, IPC/provider, memory и
evaluation contracts; существующие workflow/child contracts должны быть
сверены до добавления новой state machine.

**Опциональные:** планы 13–15 могут поставлять browser, voice или vision
capability adapters. До их готовности automation работает с явным
unsupported capability и не меняет общий run contract.

## Этапы

1. Зафиксировать definition, trigger, run, activity и health contracts.
2. Реализовать Core-owned queue, state machine, lease/generation и operation
   lock с backpressure и cancellation.
3. Добавить snapshots/diffs, crash recovery и deterministic simulation replay.
4. Закрыть schedule, security, release и acceptance matrix.

## Готово, когда

Повторный trigger не создаёт повторный side effect, stale generation не может
перезаписать новый state, run/history восстанавливаются после crash, policy и
approval проверяются при фактическом запуске, а UI не исполняет scheduler.

## Нормативные уточнения после ревью

- `AutomationDefinitionV1` содержит definition/trigger IDs, version, typed
  workflow graph reference, schedule, concurrency policy, retry policy,
  capability requirements, approval mode, input schema and retention. Unknown
  major/unsafe fields fail closed; existing workflow/child contracts remain
  the execution source of truth, with no duplicate lease/checkpoint owner.
- Run states are `queued|starting|running|waiting_approval|retrying|cancelling|
  completed|failed|cancelled|dead_letter`; activity states are
  `pending|leased|running|succeeded|failed|cancelled|unknown`. Guards,
  terminal transitions and stale generation checks are Core-owned and durable.
- Trigger idempotency key is `(definition_id, definition_version, trigger_key,
  scheduled_slot)` with 30-day retention; duplicate returns the first run.
  Concurrent runs are limited by definition policy (default 1); lease expiry
  produces `unknown` and requires reconciliation, never blind retry.
- Simulation uses a separate ephemeral SQLite/temp workspace, frozen clock,
  RNG seed, provider responses, capability snapshot and concurrency=1;
  filesystem/network/process/shell/host mutations are denied. Replay compares
  canonical activity trace, terminal state, policy/approval decisions and
  receipt hashes.
- Core owns queue, durable run/activity state and audit history; UI owns only
  input/projection. Operation locks are lease-bound, idempotent and released
  on terminal/cancel/crash; backpressure rejects new triggers with typed
  `queue_full` rather than dropping durable work.
- Retry is limited to 2 attempts for allow-listed transient provider/storage
  errors; policy denial, invalid definition, approval expiry and unsupported
  capability are non-retriable. Every run snapshots policy/approval before
  dispatch and revalidates at effect boundary.
- Acceptance requires deterministic replay, overlap/idempotency, crash/restart,
  cancellation, stale generation, approval/policy change, queue saturation and
  UI projection tests; metrics include queue depth, lease age, recovery time,
  retries, dead letters and unknown outcomes.
