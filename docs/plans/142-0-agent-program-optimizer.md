# План 142.0 — Agent Program Optimizer

Статус: предложено по [issue #123](https://github.com/rkfsociety/EvoHime/issues/123). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Реализовать Core-owned контур **Agent Program Optimizer** как отдельный versioned/revision-aware слой поверх существующей архитектуры EvoHime. Он должен иметь bounded контракт, явного владельца состояния, проверяемые переходы, recovery и metadata-only Electron projection. Renderer не получает authority над runtime, workspace, SQLite, secrets или policy.

## Архитектурная граница

```text
Core contract + registry -> validated storage -> runtime/recovery
-> authenticated IPC/replay -> projection/UI -> evidence/release docs
```

Новый слой не создаёт второй gateway, scheduler, permission system, event log, artifact store или knowledge source там, где соответствующий authority уже существует. Все внешние claims и volatile capabilities должны быть подтверждены evidence/fixture; неизвестное состояние остаётся Unknown/NeedsReview.

## Этапы

- [Этап 1 — Core-контракт, schema и storage](./142-1-agent-program-optimizer.md)
- [Этап 2 — runtime-интеграция и recovery](./142-2-agent-program-optimizer.md)
- [Этап 3 — IPC, projection и UI](./142-3-agent-program-optimizer.md)
- [Этап 4 — verification, release evidence и закрытие](./142-4-agent-program-optimizer.md)

## Зависимости

### Блокирующие

- Существующие Core policy/capability/approval, SQLite migration/backup, event/replay, ArtifactStore и authenticated desktop IPC primitives.
- Канонические owners смежных подсистем; новый план обязан ссылаться на них, а не дублировать их authority.
- Точная проверка checkout на evidence freeze перед выбором schema revision, IPC tags и конкретных module paths.

### Опциональные

- Verification Evidence Ledger (#102), Project Quality Contract (#104), Diagnostics Bundle и Agent Benchmark Matrix; при недоступности сохраняется typed degraded/Unknown result.

## Критерии готовности

- [ ] Есть versioned Core-owned contract, immutable revision/hash и bounded validation.
- [ ] Storage транзакционен, recoverable, idempotent и не содержит secrets/raw prompts/raw logs.
- [ ] Runtime не обходит существующие policy, approval, cancellation, provenance и recovery boundaries.
- [ ] IPC replay-safe, authenticated, redacted; renderer остаётся projection-only.
- [ ] Тесты покрывают happy path, invalid/unknown/stale/conflict/restart/fault cases и security invariants.
- [ ] Evidence переносится в canonical docs только после фактической реализации.

## Non-goals

Не входят второй источник истины, unrestricted code execution, silent fallback/approval bypass, автоматическое изменение пользовательских данных без обычного workflow, обязательный внешний сервис и функциональная готовность вместо одного лишь планирования.

## Связанный issue

- [#123 Agent Program Optimizer](https://github.com/rkfsociety/EvoHime/issues/123)
