# План 175.4 — Memory Ingestion Integrity: verification и закрытие

## Зависимости

### Блокирующие

- [Diagnostics, IPC и projection](./175-3-memory-ingestion-integrity.md).
- Memory migrations, recovery tests, security policy, GitHub CI и canonical
  documentation.

### Опциональные

- Minimal-dependency CI lane for lexical/basic recall fallback.

## Реализация

- Добавить tests для origin/depth/reentrancy, restricted extractor context,
  candidate durability, idempotency, freshness/CAS, concurrent finalizers,
  malformed output, throttling, cancellation, crash/restart, deletion and
  stale provenance.
- Проверить migration upgrade/rollback/fresh DB, no inline heavy rebuild,
  optional backend fallback and absence of raw memory/secrets in logs/events/
  IPC/support bundles.
- Выполнить только быстрые необходимые локальные checks; полный acceptance и
  platform-specific verification оставить CI, сохранив фактическое evidence.
- Перенести contract/state/evidence в `docs/architecture.md`,
  `docs/current-state.md`, `docs/release-evidence.md` и
  `docs/development-plan.md`, проверить ссылки и удалить `175-*.md` после
  реального завершения implementation.

## Критерии

- Все критерии обзора 175 подтверждены кодом и CI evidence.
- `git diff --check` и migration/recovery gates проходят; исторический issue
  #153 не является gate закрытия.

## Non-goals

Считать наличие candidate или повтор модели доказательством confirmed memory.
