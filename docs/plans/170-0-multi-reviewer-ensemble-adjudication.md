# План 170.0 — Multi-Reviewer Ensemble & Adjudication

Статус: предложено по [issue #150](https://github.com/rkfsociety/EvoHime/issues/150). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Multi-Reviewer Ensemble & Adjudication должен стать Core-owned, versioned и revision-aware capability в локальном Windows desktop-продукте EvoHime. Review lane должен параллельно собирать независимые model-family verdicts, хранить corroboration/disagreement и proof-gated completion; model consensus не заменяет Core policy, evidence или human approval.

Результат обязан иметь bounded contracts, явного владельца state, policy/capability/approval boundary, recovery после restart и metadata-only projection в Electron. Renderer не обращается к workspace/SQLite/provider напрямую и не может подделать verdict, identity, evidence или permissions.

## Архитектурная граница

~~~text
Core contract/registry -> transactional storage -> supervised runtime
-> authenticated IPC/replay -> bounded UI projection -> verification evidence
~~~

Не создавать вторую authority для уже существующих gateway, event log, artifact store, memory/knowledge registry, scheduler или permission system. Внешние adapters работают opt-in, с явной credential scope, offline fixtures и fail-closed degradation.

## Этапы

- [Этап 1 — Core-контракт, schema и storage](./170-1-multi-reviewer-ensemble-adjudication.md)
- [Этап 2 — runtime-интеграция и recovery](./170-2-multi-reviewer-ensemble-adjudication.md)
- [Этап 3 — IPC, projection и UI](./170-3-multi-reviewer-ensemble-adjudication.md)
- [Этап 4 — verification, release evidence и закрытие](./170-4-multi-reviewer-ensemble-adjudication.md)

## Зависимости

### Блокирующие

- Existing Core policy/capability/approval, cancellation, timeout, provenance, SQLite migration/backup и authenticated IPC/replay primitives.
- Canonical owner contracts, перечисленные в issue и текущих docs; точные module paths, schema revision и IPC tags подтверждаются на evidence freeze.
- Для external tools/devices/providers — explicit opt-in, supervised process/network boundary и deterministic fixtures.

### Опциональные

- Verification Evidence Ledger (#102), Project Quality Contract (#104), Diagnostics Bundle и Agent Benchmark Matrix; отсутствие опционального сигнала даёт typed Unknown/degraded state.

## Критерии готовности

- [ ] Contract versioned, bounded, immutable-by-revision и canonical-hash bound.
- [ ] Storage transactional, recoverable, idempotent и не содержит secrets/raw prompts/raw payloads.
- [ ] Runtime не обходит policy, approval, cancellation, resource limits или provenance.
- [ ] IPC authenticated, replay-safe, redacted; UI projection-only и доступен.
- [ ] Tests покрывают success, invalid/stale/conflict/restart/fault/security и отсутствие внешней зависимости.
- [ ] После реализации contract/state/evidence переносятся в canonical docs, затем комплект удаляется.

## Non-goals

Второй runtime/gateway/storage, unrestricted shell/network execution, silent fallback, автоматический approval, скрытое изменение пользовательских данных, обязательная cloud telemetry и функциональная готовность вместо плана.

## Связанный issue

- [#150 Multi-Reviewer Ensemble & Adjudication](https://github.com/rkfsociety/EvoHime/issues/150)
