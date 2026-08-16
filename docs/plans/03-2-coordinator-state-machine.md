# Этап 03.2: Coordinator state machine

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этап 03.1 (typed report, который валидируется при переходах) и
существующие leases и task graph.

Разблокирует: 03.3 и 03.4.

## Что этап отдаёт наружу

Явные состояния child task, bounded leases и restart recovery.

## Что уже есть в коде

Есть: `ChildLifecycleState` ровно в описанных ниже состояниях (плюс `TimedOut`
и `Aborted`), проверяемые переходы и события lifecycle с порядковым номером.

Нет: восстановления только из durable checkpoint после restart с повторной
валидацией report/evidence, bounded дочерних leases и fan-in нескольких
отчётов.

## Содержание

- Зафиксировать Created → Queued → Running → Validating →
  WaitingParentAcceptance → Accepted/Rejected/Failed/Cancelled.
- Не считать child success финальным task success.
- Дочерние leases, cancellation и restart recovery должны быть bounded.
- После restart coordinator восстанавливает только durable checkpoint и
  повторно валидирует report/evidence.

## Проверки

- sequential, concurrent, conditional workflow fixtures;
- cancellation/restart/lease-loss recovery;
- reviewer rejection → bounded revision;
- fan-in deterministic ordering and conflict reporting.

## Критерии готовности

- parent никогда не принимает child result без validation;
- restart/cancellation не оставляют orphan processes or leases.
