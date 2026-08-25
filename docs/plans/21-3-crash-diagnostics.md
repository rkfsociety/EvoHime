# План 21.3 — Crash recovery и диагностический bundle

Статус: draft, зависит от утверждения обзора 21-0.

## Цель

Надёжно восстанавливать shell, Core, supervisor, transaction worker и repair
после сбоя, а пользователю и сопровождению давать bounded диагностический
материал без утечки секретов или данных workspace.

## Зависимости

### Блокирующие

- обзор 21-0;
- план 21.1;
- supervisor lifecycle и Job Object;
- startup reconciliation receipt, updater и durable event replay;
- локальные redacted JSONL logs.

### Опциональные

- Windows crash dump: отсутствие dump не блокирует recovery;
- расширенные OS diagnostics: typed `unsupported` на неподдержанной системе.

## Работы

- определить классификацию `crash`, `interrupted`, `unknown_outcome`,
  `rollback`, `core_unavailable` и `recoverable_failure`;
- описать startup reconciliation и защиту от blind retry;
- создать bounded diagnostic bundle с версиями компонентов, commit, sequence,
  typed states и redacted log excerpts;
- добавить безопасное поведение для corrupted projection, stale events и
  незавершённой transaction;
- проверить retention, export и удаление bundle.

## Acceptance gates

- deterministic tests для каждого сбоя и повторного запуска;
- supervisor/Core IPC и event replay tests;
- privacy gate запрещает provider keys, DPAPI payloads, prompts, tool output и
  workspace contents;
- bundle имеет размер, срок хранения и redaction limits;
- Windows smoke проверяет single-instance, Job Object и restart recovery.

## Результат

Сбой классифицируется явно, безопасно восстанавливается или остаётся
recoverable failure с понятным следующим действием.
