# План 188.3 — comparison, bounded projections and explicit promotion

Статус: active implementation contract. Этап следует после overview 188.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Parse only versioned structured metrics; compare to compatible baseline; classify Candidate/Reject/NeedsMoreEvidence/Incomparable; bounded tree and metric projections over authenticated IPC.

## Зависимости

### Блокирующие

Блокирующие: 188.2, metric/evaluator and existing benchmark/evidence owners. Optional plans 184/185/179.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Partial/unknown/unavailable cannot be candidate; replay idempotent on exact hashes; explicit existing Core promotion only.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Integration: incompatible schema/baseline, missing samples, hard constraints, unavailable evaluator, forged metric, exhausted budget, cancel, concurrency.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Improvement is not authority; no automatic merge/push/publication.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
