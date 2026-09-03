# План 142.1 — Agent Program Optimizer: Core-контракт, schema и storage

Статус: этап 1 для [плана 142.0](./142-0-agent-program-optimizer.md); issue: [#123](https://github.com/rkfsociety/EvoHime/issues/123).

## Зависимости

### Блокирующие

- План 142.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Определить bounded типы для Agent Program Optimizer, lifecycle Draft/Active/Superseded/Invalid, canonical hash, scope/actor/revision/idempotency semantics. Добавить metadata-only transactional storage и additive migration с backup, rollback, corruption/expiry/size limits. Зафиксировать ownership и границы с существующими registry/policy/provenance subsystems.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных.
