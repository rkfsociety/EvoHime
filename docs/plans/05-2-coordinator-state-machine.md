# Этап 05.2: Coordinator state machine

Этап плана [05 Специализированные child workflows](05-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этап 05.1 (typed report, который валидируется при переходах) и
существующие leases и task graph.

Разблокирует: 05.3 и 05.4.

## Что этап отдаёт наружу

Явные состояния child task, bounded leases и restart recovery.

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
