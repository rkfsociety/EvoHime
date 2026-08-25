# План 21.2 — Evidence для self-repair и обновлений

Статус: draft, зависит от утверждения обзора 21-0.

## Цель

Сделать ход repair-run и обновления проверяемым: отдельные этапы, commit SHA,
CI checks, health-marker и причина rollback должны быть видны пользователю в
bounded redacted projection.

## Зависимости

### Блокирующие

- обзор 21-0;
- план 19.0 и существующие repair FSM/transaction worker;
- authenticated Core startup и health-marker contract;
- GitHub green-commit/update policy.

### Опциональные

- GitHub Check Runs API: typed `ci_status_unavailable`, продвижение repair
  блокируется;
- CI provider details: показывается общий typed result без raw provider output.

## Работы

- описать FSM `diagnose → patch → tests → commit → push → CI → staging →
  restart → health → complete/rollback`;
- связать каждый этап с bounded evidence: SHA, timestamp, result code и
  redacted summary;
- показывать check-runs и причины failure без секретов и содержимого workspace;
- запретить переход к следующему опасному шагу без отдельного клика;
- зафиксировать rollback evidence и retention/redaction policy;
- добавить recovery незавершённого repair-run после закрытия shell.

## Acceptance gates

- FSM, idempotency и restart-recovery tests;
- Electron repair UI и real-Core E2E;
- CI failure не запускает restart или update; commit и push остаются отдельными кликами и не инициируются CI;
- health timeout приводит к rollback и видимому evidence;
- evidence не содержит secrets, raw prompts, tool output или workspace files.

## Результат

Repair остаётся строго пользовательским процессом, но каждый его шаг можно
проверить и объяснить.
