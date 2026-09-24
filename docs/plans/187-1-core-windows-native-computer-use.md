# План 187.1 — target/observation/action/effect contracts

Статус: active implementation contract. Этап следует после overview 187.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Определить exact target identity, bounded fresh observation, TTL, one-use nonce, DPI/geometry/action mapping и typed action/delivery/verification result.

## Зависимости

### Блокирующие

Блокирующие: Windows window/session identity, PolicyGate/approval/receipt and EventJournal.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Observation invalidated on resize/DPI/window/helper generation change; persist only hashes/refs/reason codes.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Contract tests: PID reuse, multi-window, stale observation, token reuse, DPI/geometry, pixel bounds, legal states.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] UI/screen/model content stays untrusted; observation не permission.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
