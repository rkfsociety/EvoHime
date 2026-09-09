# План 167.0 — Command Center

Статус: предложено по [issue #147](https://github.com/rkfsociety/EvoHime/issues/147). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Реализовать Core-owned контур **Command Center** поверх существующей Electron/Rust/SQLite архитектуры EvoHime. Состояние, policy, permissions, recovery и итоговый verdict принадлежат Core; renderer получает только bounded authenticated projection.

## Граница

План не создаёт параллельный runtime, storage, event log, provider gateway или permission system. Все переходы versioned и revision-aware, ошибки/stale/conflict/restart дают явный non-success result, а внешние устройства и volatile capabilities работают только через opt-in adapters и offline fixtures.

## Этапы

- [Этап 1 — Core-контракт, schema и storage](./167-1-command-center.md)
- [Этап 2 — runtime-интеграция и recovery](./167-2-command-center.md)
- [Этап 3 — IPC, projection и UI](./167-3-command-center.md)
- [Этап 4 — verification, release evidence и закрытие](./167-4-command-center.md)

## Зависимости

### Блокирующие

- Existing Core policy/capability/approval, cancellation, SQLite migration/backup, event/replay, provenance и authenticated IPC.
- Точная сверка checkout перед фиксацией schema revision, IPC tags и module paths.

### Опциональные

- Verification Evidence Ledger (#102), Project Quality Contract (#104), Diagnostics Bundle и Agent Benchmark Matrix; без них сохраняется typed Unknown/degraded state.

## Критерии готовности

- [ ] Versioned Core-owned contract, immutable revision/hash и bounded validation.
- [ ] Transactional recovery-safe storage без secrets/raw prompts/raw logs.
- [ ] Runtime не обходит policy, approval, timeout, cancellation или provenance.
- [ ] Replay-safe authenticated IPC и projection-only UI.
- [ ] Focused tests для success, invalid/stale/conflict/restart/fault/security случаев.
- [ ] После реализации evidence переносится в canonical docs, затем комплект удаляется.

## Связанный issue

- [#147 Command Center](https://github.com/rkfsociety/EvoHime/issues/147)
