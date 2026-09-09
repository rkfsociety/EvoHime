# План 167.1 — Command Center

Статус: этап 1 для [плана 167.0](./167-0-command-center.md); issue: [#147](https://github.com/rkfsociety/EvoHime/issues/147).

## Зависимости

### Блокирующие

- План 167.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102, #104 и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Определить bounded types для Command Center, lifecycle Draft/Active/Superseded/Invalid, canonical hash, actor/scope/revision/idempotency и typed failure semantics. Добавить metadata-only transactional storage/migration с backup, rollback, expiry/size limits и corruption recovery.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и проверяются Core.
- [ ] Stale/conflict/restart/failure дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation и renderer-owned business logic.
