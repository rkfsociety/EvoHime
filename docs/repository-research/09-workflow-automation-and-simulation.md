# 09. Workflow, automation и длительные simulation jobs

## Цель

Добавить длительные, повторяемые и расписанные операции, не превращая UI в
scheduler/executor и не разрешая неконтролируемую multi-agent autonomy.

## Scope

- Core-owned input queue и single-owner state machine;
- generation/lease protection от stale и overlapping runs;
- разделение tick/step и high-frequency messages от durable state;
- operation lock для async provider calls;
- snapshots/diffs и history separation;
- `AutomationDefinition`, trigger, run, activity log, health и cancellation;
- idempotency key, permission snapshot и approval policy на каждый запуск;
- UI только изменяет definition и показывает projection.

## Инварианты

- Scheduler, state machine и durable run history принадлежат Core.
- Повторный trigger с тем же idempotency key не создаёт side effect повторно.
- Stale generation не может перезаписать новый state.
- Завершённые сущности архивируются отдельно от active state.
- Сбой provider, supervisor restart и cancellation дают восстанавливаемый
  typed outcome.
- Simulation/benchmark окружения не имеют production side effects.
- Непредсказуемая цепочка child agents не является базовой capability.

## Тестовый контур

- duplicate trigger и idempotency;
- overlapping runs и stale generation;
- tick/step ordering и queue backpressure;
- snapshot/diff recovery после crash;
- schedule cancellation и restart;
- permission/approval snapshot при фактическом запуске;
- history/archive consistency;
- deterministic simulation replay.

## Критерии готовности

- run, activity log, health и cancellation видны через Core projection;
- automation не обходит policy и supervisor;
- recovery не теряет audit/state;
- side effects bounded, cancellable и idempotent;
- UI не исполняет scheduler или worker самостоятельно.

## Зависимости

Требует 01–05. Browser, voice и vision подключаются только как отдельные
capability adapters после прохождения их собственных release gates.
