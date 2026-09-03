# План 130.4 — Task Ownership & Lease Fencing: verification, release evidence и закрытие

Статус: этап 4 для [плана 130.0](./130-0-task-ownership-lease-fencing.md); issue: [#111](https://github.com/rkfsociety/EvoHime/issues/111).

## Зависимости

### Блокирующие

- План 130.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Сформировать focused contract/storage/runtime/recovery tests, migration/fault fixtures, IPC/replay/redaction/accessibility checks и подходящую workspace regression. Выполнить git diff --check и evidence review. После реализации перенести подтверждённый контракт в docs/architecture.md, состояние в docs/current-state.md и release procedure в docs/release-evidence.md; затем удалить полный комплект по правилам.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных.
