# План 186.4 — quality/security/resource evidence и closure

Статус: active implementation contract. Этап следует после overview 186.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Frozen suites по family/language/cardinality: ECE/Brier/accuracy, risk false negatives, p50/p95, batch, memory, cold/warm; update architecture/state/release evidence.

## Зависимости

### Блокирующие

Блокирующие: 186.1–186.3 and Windows/CI evidence.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Test restart, cancel, stale calibration, changed artifact and resource eviction; no old request masquerading as same attempt.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Focused contracts/adapter/consumer/calibration/resource tests and secret-free IPC/log review.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Acceptance подтверждён; unavailable/needs_review являются штатными результатами.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
