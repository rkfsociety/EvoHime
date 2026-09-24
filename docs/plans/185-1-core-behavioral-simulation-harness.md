# План 185.1 — scenario/actor/environment/run contracts

Статус: active implementation contract. Этап следует после overview 185.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Задать versioned Scenario, scripted/canned actor, typed observation predicates, synthetic environment refs, turn/resource policy, outcome contract и frozen run snapshot.

## Зависимости

### Блокирующие

Блокирующие: tool fixture/emulation, agent/policy snapshot и план 184 TraceEvaluationContractRef.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Run identity фиксирует scenario/actor/environment/model/policy/seed; synthetic approval ограничен run.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Unit: schema/hash, deterministic transitions, limits, capability widening, approval isolation, unknown transition.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Scripted mode reproducible; actor/text не authority.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
