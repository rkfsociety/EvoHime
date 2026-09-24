# План 184.4 — regressions, release evidence и closure

Статус: active implementation contract. Этап следует после overview 184.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Закрыть compatibility, errors, hash/evidence mapping; обновить architecture/current-state/eval schema/release evidence только после реализации.

## Зависимости

### Блокирующие

Блокирующие: 184.1–184.3 и focused test/CI evidence.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Проверить crash до/после freeze, report commit и aggregation; rollback оставляет digest-only path.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Unit/integration/recovery/Windows regression по #160, CI artifact inspection и ссылки.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Acceptance подтверждён evidence; baseline promotion остаётся explicit Core action.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
