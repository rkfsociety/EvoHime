# План 169.4 — Agent Client Protocol Bridge

Статус: этап 4 для [плана 169.0](./169-0-agent-client-protocol-bridge.md); issue: [#149](https://github.com/rkfsociety/EvoHime/issues/149).

## Зависимости

### Блокирующие

- План 169.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance, cancellation и authenticated IPC boundaries.

### Опциональные

- #102/#104 и diagnostics; при отсутствии их evidence результат остаётся explicit Unknown/degraded, без optimistic pass.

## Реализация

Сформировать focused contract/storage/runtime/recovery tests, fault and migration fixtures, IPC/replay/redaction/accessibility checks и подходящую CI evidence. Выполнить git diff --check. После реальной реализации обновить architecture/current-state/release-evidence, закрыть acceptance gaps и удалить полный комплект 169.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и Core-validated.
- [ ] Stale/conflict/restart/failure cases имеют безопасный non-success outcome.
- [ ] Нет обхода existing authority, secrets, raw user data или approval.
- [ ] Есть reproducible tests/evidence и rollback/recovery path.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация.

