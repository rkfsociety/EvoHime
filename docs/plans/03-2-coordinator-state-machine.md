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
  повторно валидирует report/evidence. Checkpoint — атомарная SQLite-запись,
  содержащая child state, revision, reports, evidence locators и hashes,
  active leases, parent sequence и last transition event; запись выполняется
  после каждого state transition в одной транзакции с событием.
- После restart `Running`, `Validating` и `WaitingParentAcceptance` без
  подтверждённого живого lease помечаются `Failed` с причиной `restart`, а
  cleanup lease/process выполняется идемпотентно.
- Reviewer `revise` содержит evidence и список нарушенных acceptance criteria.
  Coordinator создаёт новую revision только в пределах `max_revisions`;
  после лимита действует правило из 03-0.
- Fan-in выполняется до implementer по правилам 03-0; выбранные evidence,
  конфликты и причины выбора входят в checkpoint и trace.

## Проверки

- sequential, concurrent, conditional workflow fixtures;
- cancellation/restart/lease-loss recovery;
- reviewer rejection → bounded revision;
- fan-in deterministic ordering and conflict reporting.
- partial tester failure: обязательный criterion → revise, необязательный →
  Accepted с риском и coordinator approval;
- checkpoint round-trip и restart cleanup без orphan leases/processes.

## Критерии готовности

- parent никогда не принимает child result без validation;
- restart/cancellation не оставляют orphan processes or leases.
