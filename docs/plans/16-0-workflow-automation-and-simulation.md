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

