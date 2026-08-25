# План 21.1 — Единая модель approval и recovery UX

Статус: draft, зависит от утверждения обзора 21-0.

## Цель

Свести `WAITING_APPROVAL`, `RECOVERING`, `BLOCKED`, `FAILED`, `RESUMABLE` и
`UNKNOWN_OUTCOME` к одной bounded Core-owned projection, чтобы Timeline,
RecoveryBanner и OperationsPanel показывали одинаковое состояние и безопасное
следующее действие.

## Зависимости

### Блокирующие

- планы 01–20;
- authenticated desktop IPC и durable approval/receipt contracts;
- существующие `RecoveryBanner`, `TaskTimeline` и `OperationsPanel`.

### Опциональные

- новые IPC-поля: старый клиент игнорирует их и сохраняет текущую проекцию;
- real-Core E2E: при недоступном бинарнике обязательны Rust и contract tests.

## Работы

- составить таблицу состояний, reason codes, источников событий и допустимых
  действий;
- ввести bounded projection с monotonic sequence и replay-поведением;
- унифицировать тексты и действия approval: preview, expiry, stale,
  call-changed, policy denial, cancellation;
- оставить FSM и policy decisions в Core, renderer ограничить отображением;
- добавить idempotent действия reconcile, cancel, resolve approval и открытие
  redacted evidence;
- покрыть duplicate, stale, out-of-order и expired события.

## Acceptance gates

- Rust unit/contract tests на все состояния и переходы;
- Electron protocol/typecheck и UI tests для всех основных веток;
- compatibility tests для старого IPC-клиента;
- real-Core E2E approval/recovery без обхода policy;
- `git diff --check` и redaction/privacy gate.

## Результат

Пользователь всегда видит подтверждённое Core состояние, причину и только
разрешённые действия; renderer не принимает решений о переходах.
