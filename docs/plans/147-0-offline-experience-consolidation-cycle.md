# План 147.0 — Offline Experience Consolidation Cycle

Статус: предложено по [issue #127](https://github.com/rkfsociety/EvoHime/issues/127). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Реализовать Core-owned контур **Offline Experience Consolidation Cycle** как versioned/revision-aware capability поверх существующей архитектуры EvoHime. Контур должен иметь bounded contract, одного владельца состояния, безопасные transitions, recovery и metadata-only Electron projection. Renderer не получает authority над runtime, workspace, SQLite, secrets или policy.

## Архитектурная граница

~~~text
Core contract + registry -> validated storage -> runtime/recovery
-> authenticated IPC/replay -> projection/UI -> evidence/release docs
~~~

План расширяет существующие subsystems и не создаёт дубликаты Model Gateway, scheduler, permissions, event log, artifact store или knowledge source. Неизвестные внешние capabilities и недоказанные результаты остаются Unknown/NeedsReview; автоматические действия проходят обычные policy/approval/cancellation boundaries.

## Этапы

- [Этап 1 — Core-контракт, schema и storage](./147-1-offline-experience-consolidation-cycle.md)
- [Этап 2 — runtime-интеграция и recovery](./147-2-offline-experience-consolidation-cycle.md)
- [Этап 3 — IPC, projection и UI](./147-3-offline-experience-consolidation-cycle.md)
- [Этап 4 — verification, release evidence и закрытие](./147-4-offline-experience-consolidation-cycle.md)

## Зависимости

### Блокирующие

- Existing Core policy/capability/approval, cancellation, SQLite migration/backup, event/replay, provenance и authenticated IPC primitives.
- Канонические owners смежных систем; точные module paths, schema revision и IPC tags подтверждаются на evidence freeze.
- Для внешних устройств, провайдеров или IDE — explicit opt-in adapters, scoped credentials и offline fixtures.

### Опциональные

- Verification Evidence Ledger (#102), Project Quality Contract (#104), Diagnostics Bundle и Agent Benchmark Matrix; при недоступности результат остаётся typed degraded/Unknown.

## Критерии готовности

- [ ] Есть versioned Core-owned contract, immutable revision/hash и bounded validation.
- [ ] Storage транзакционен, recoverable, idempotent и secret-free.
- [ ] Runtime соблюдает policy, approval, timeout, cancellation, provenance и recovery boundaries.
- [ ] IPC replay-safe, authenticated, redacted; renderer остаётся projection-only.
- [ ] Тесты покрывают success, invalid/unknown/stale/conflict/restart/fault и security cases.
- [ ] Подтверждённый контракт переносится в canonical docs только после реализации.

## Non-goals

Не входят второй источник истины, unrestricted code/network execution, silent fallback/approval bypass, автоматическое изменение пользовательских данных без governed workflow, обязательный внешний сервис и functional completion вместо плана.

## Связанный issue

- [#127 Offline Experience Consolidation Cycle](https://github.com/rkfsociety/EvoHime/issues/127)
