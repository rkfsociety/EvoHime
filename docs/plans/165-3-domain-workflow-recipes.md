# План 165.3 — Domain Workflow Recipes

Статус: этап 3 для [плана 165.0](./165-0-domain-workflow-recipes.md); issue: [#145](https://github.com/rkfsociety/EvoHime/issues/145).

## Зависимости

### Блокирующие

- План 165.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102, #104 и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Добавить additive authenticated IPC commands/events после проверки highest tag, correlation/idempotency и replay/resync. Проецировать status, revision/hash prefix, bounded explanation и next action; renderer не пишет storage, не вычисляет verdict и не получает secrets/raw payloads.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и проверяются Core.
- [ ] Stale/conflict/restart/failure дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation и renderer-owned business logic.
