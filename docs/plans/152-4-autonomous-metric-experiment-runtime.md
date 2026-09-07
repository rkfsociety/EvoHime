# План 152.4 — Autonomous Metric Experiment Runtime: verification, release evidence и закрытие

Статус: этап 4 для [плана 152.0](./152-0-autonomous-metric-experiment-runtime.md); issue: [#132](https://github.com/rkfsociety/EvoHime/issues/132).

## Зависимости

### Блокирующие

- План 152.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Сформировать focused contract/storage/runtime/recovery tests, migration/fault fixtures, IPC/replay/redaction/accessibility checks и appropriate workspace regression. Выполнить git diff --check и evidence review. После реализации перенести contract в docs/architecture.md, state в docs/current-state.md, release procedure в docs/release-evidence.md и удалить полный комплект.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных.
