# План 186.1 — request/result, adapter, capability и calibration contracts

Статус: active implementation contract. Этап следует после overview 186.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Определить bounded typed request/result, choice/score/probability, question batching, state refs, answer distributions, model/adapter identity and dispositions. Расширить manager-owned capabilities и quality calibration.

## Зависимости

### Блокирующие

Блокирующие: local model manager, policy/privacy, benchmark and calibration owners.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Model/artifact/adapter/family/language/cardinality/calibration mismatch → incompatible; result hash binds frozen inputs.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Contracts: bounds, distribution normalization, ECE/proper metric, freshness, unknown fields.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Quality calibration отдельно от hardware calibration; confidence без evidence не Eligible.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
