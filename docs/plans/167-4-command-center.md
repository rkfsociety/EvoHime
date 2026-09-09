# План 167.4 — Command Center

Статус: этап 4 для [плана 167.0](./167-0-command-center.md); issue: [#147](https://github.com/rkfsociety/EvoHime/issues/147).

## Зависимости

### Блокирующие

- План 167.0 и предыдущий этап этого направления.
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
