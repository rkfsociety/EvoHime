# План 138.3 — Skill Source & Update Lifecycle: IPC, projection и UI

Статус: этап 3 для [плана 138.0](./138-0-skill-source-update-lifecycle.md); issue: [#119](https://github.com/rkfsociety/EvoHime/issues/119).

## Зависимости

### Блокирующие

- План 138.0 и предыдущий этап этого направления.
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
