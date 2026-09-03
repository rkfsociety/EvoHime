# План 143.3 — Project Knowledge Notebook: IPC, projection и UI

Статус: этап 3 для [плана 143.0](./143-0-project-knowledge-notebook.md); issue: [#124](https://github.com/rkfsociety/EvoHime/issues/124).

## Зависимости

### Блокирующие

- План 143.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Добавить additive authenticated IPC commands/events после проверки highest tag, correlation/idempotency, replay/resync и bounded errors. Проецировать только redacted metadata: status, revision/hash prefix, scope, evidence refs and next action. Создать минимальную Electron surface; renderer не вычисляет verdict, не пишет storage и не получает secrets/raw payloads.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных.
