# План 129.2 — Confidence-Gated Model Cascade: runtime-интеграция и recovery

Статус: этап 2 для [плана 129.0](./129-0-confidence-gated-model-cascade.md); issue: [#110](https://github.com/rkfsociety/EvoHime/issues/110).

## Зависимости

### Блокирующие

- План 129.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Подключить Confidence-Gated Model Cascade к Core runtime через явные commands/state transitions. Реализовать cancellation, timeout, approval/policy checks, optimistic concurrency, crash recovery и last-known-safe behavior. Unknown, stale, denied, conflict и partial failure не превращаются в успешный результат; активный run pin-ит immutable revision.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных.
