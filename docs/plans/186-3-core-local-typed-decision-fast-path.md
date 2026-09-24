# План 186.3 — safe consumer integration и bounded projections

Статус: active implementation contract. Этап следует после overview 186.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Интегрировать хотя бы один safe consumer: route/task hint из разрешённого purpose profile или candidate narrowing/guardrail tightening; IPC — bounded status/reason.

## Зависимости

### Блокирующие

Блокирующие: 186.2, Model Purpose Routing, PolicyGate and consumer fail behavior.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Low confidence → explicit NeedsReview/Unavailable or declared consumer behavior; no silent correctness fallback.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Consumer tests: cannot add route/tool/grant; low confidence cannot expand; deterministic guards dominate.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Renderer не запускает inference; confidence and permission decisions remain Core-owned.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
