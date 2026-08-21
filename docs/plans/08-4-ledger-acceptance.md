# 08-4 — Acceptance, security и закрытие плана

## Цель

Подтвердить execution ledger end-to-end и перенести завершённый контракт в
канонические документы перед удалением плана как выполненного.

## Deterministic acceptance fixtures

- action → tool call → observation → successful typed receipt;
- approval approve/reject/expiry;
- timeout, cancellation, provider failure и unknown result;
- reconnect во время каждой промежуточной фазы;
- duplicate IPC delivery и duplicate terminal attempt;
- SQLite failure с полным rollback;
- supervisor restart с `stuck`/`unknown_outcome` без blind retry;
- replay gap, stale Core revision и bounded snapshot;
- secret/PII/raw output redaction.

## Release checks

- `cargo test` для Core, local storage, desktop IPC и receipt integration;
- `npm run check:protocol`, `npm run typecheck`, `npm test`;
- migration, backup, rollback и package smoke;
- security review для redaction, approval binding, frame/payload limits и
  renderer isolation;
- `git diff --check` и проверка всех внутренних Markdown-ссылок.

## Закрытие

После прохождения критериев контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, а исполняемые файлы
плана 08 удаляются по правилу репозитория. Если хотя бы один критерий не
закрыт, план остаётся в каталоге с секцией о фактически реализованном и
оставшемся поведении.
