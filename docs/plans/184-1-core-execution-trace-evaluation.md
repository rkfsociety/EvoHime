# План 184.1 — trace contract, normalization и assertions

Статус: active implementation contract. Schema, assertions и legacy digest подтверждены exact-SHA CI на базовом main; follow-up hardening ожидает CI, а git diff --check остаётся открытым gate.

## Scope и изменяемые контракты

Структурировать ExecutionTrace, event/run/attempt identity, terminal state, evaluator contract, assertions/results и canonical hash. Exact/subsequence, forbidden tools, canonical args, approval/effect, evidence, retry equivalence и bounds; raw content исключить до hash.

Совместимость: существующий `AttemptResult::tool_trace_digest` сохраняет
64-символьный lowercase hex формат. `ExecutionTrace::legacy_tool_trace_digest`
возвращает те же SHA-256 bytes, что `canonical_hash`, но без префикса
`sha256:`; отдельное поле, журнал или trace store не создаётся.

## Зависимости

### Блокирующие

Блокирующие: authoritative event/effect ids, redaction boundary, benchmark and fixture compatibility.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Partial/unknown events → Invalid/Interrupted; unknown schema/event не success. Hash только redacted canonical trace.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Unit: normalization/hash, legacy digest format and byte compatibility, args,
order, approval binding, retry target linkage, terminal outcome consistency,
bounds, unknown fields/status, stable reason-code serialization and incompatible
versions.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [x] Schema, stable reason codes and compatibility with old tool_trace_digest; no second trace store.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [x] Canonical docs обновляются только после подтверждённого поведения.
