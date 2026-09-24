# План 184.1 — trace contract, normalization и assertions

Статус: active implementation contract. Этап следует после overview 184.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Структурировать ExecutionTrace, event/run/attempt identity, terminal state, evaluator contract, assertions/results и canonical hash. Exact/subsequence, forbidden tools, canonical args, approval/effect, evidence, retry equivalence и bounds; raw content исключить до hash.

## Зависимости

### Блокирующие

Блокирующие: authoritative event/effect ids, redaction boundary, benchmark and fixture compatibility.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Partial/unknown events → Invalid/Interrupted; unknown schema/event не success. Hash только redacted canonical trace.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Unit: normalization/hash, args, order, approval binding, bounds, unknown fields/status, incompatible versions.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Schema, stable reason codes and compatibility with old tool_trace_digest; no second trace store.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
