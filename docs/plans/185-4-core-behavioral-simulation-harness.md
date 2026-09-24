# План 185.4 — determinism/security/recovery evidence и closure

Статус: active implementation contract. Этап следует после overview 185.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Подготовить clarification/wrong-action/approval-denial/retry/injection/recovery scenarios; обновить eval schema и canonical docs.

## Зависимости

### Блокирующие

Блокирующие: 185.1–185.3 и plan 184 evidence.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Проверить restart at committed boundary, mid-turn, stale revision/environment, parallel CI isolation.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Focused unit/integration/recovery/security/Windows and CI artifact checks.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Scripted local-first acceptance выполнен; model-driven actor не требуется для MVP.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
