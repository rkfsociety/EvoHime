# План 185.3 — scenario evaluator, trace/benchmark и CLI/CI

Статус: active implementation contract. Этап следует после overview 185.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Оценивать typed scenario outcome, прикреплять plan 184 trace report и подавать per-attempt metrics в Benchmark Matrix; добавить scenario packs в evohime-eval.

## Зависимости

### Блокирующие

Блокирующие: 185.2, trace evaluator, existing benchmark/eval CLI.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Idempotency по run/scenario/combination/attempt; missing actor output не regenerates silently.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Один seed/snapshot даёт тот же report hash; incompatible trace/provider → unavailable/incomparable.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] CI показывает bounded reason codes; no new aggregate/baseline store.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
