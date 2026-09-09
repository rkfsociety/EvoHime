# План 164.1 — Semantic Activity Motion System

Статус: этап 1 для [плана 164.0](./164-0-semantic-activity-motion-system.md); issue: [#144](https://github.com/rkfsociety/EvoHime/issues/144).

## Зависимости

### Блокирующие

- План 164.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102, #104 и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Определить bounded types для Semantic Activity Motion System, lifecycle Draft/Active/Superseded/Invalid, canonical hash, actor/scope/revision/idempotency и typed failure semantics. Добавить metadata-only transactional storage/migration с backup, rollback, expiry/size limits и corruption recovery.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и проверяются Core.
- [ ] Stale/conflict/restart/failure дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation и renderer-owned business logic.
