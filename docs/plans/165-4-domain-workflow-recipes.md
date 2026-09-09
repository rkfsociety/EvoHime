# План 165.4 — Domain Workflow Recipes

Статус: этап 4 для [плана 165.0](./165-0-domain-workflow-recipes.md); issue: [#145](https://github.com/rkfsociety/EvoHime/issues/145).

## Зависимости

### Блокирующие

- План 165.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102, #104 и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Проверить contract/storage/runtime/recovery, migration/fault, IPC/replay/redaction/accessibility и regression cases; выполнить git diff --check. После реализации перенести подтверждённый контракт в docs/architecture.md, state в docs/current-state.md, evidence в docs/release-evidence.md и удалить полный комплект.

## Критерии выхода

- [ ] Material transitions типизированы, bounded и проверяются Core.
- [ ] Stale/conflict/restart/failure дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation и renderer-owned business logic.
