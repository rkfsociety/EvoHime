# План 185.2 — turn runtime, persistence и recovery

Статус: active implementation contract. Этап следует после overview 185.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Оркестрировать bounded turns через existing Core runtime/Tool Simulation Runtime; committed turn checkpoints в существующем benchmark/evidence store.

## Зависимости

### Блокирующие

Блокирующие: 185.1, Tool Simulation Runtime, workflow/child and idempotent persistence.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Resume только с committed turn при matching hashes; ambiguous mid-turn не повторяется; no network or real adapter.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Integration: clarification, approval, failure/retry, injection, unavailable fixture, cancellation, no-real-fallback, duplicate commit.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Tool Simulation Runtime остаётся effect interception owner; его ephemeral cache не считается durable.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
