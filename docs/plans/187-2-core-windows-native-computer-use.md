# План 187.2 — Windows UIA adapter, dispatch и recovery

Статус: active implementation contract. Этап следует после overview 187.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Windows coordinator: exact enumeration, UIA-first, background-first actions, capability-bounded dispatch, pre-effect recheck, readback and separate postcondition verification. Helper only through Supervisor if needed.

## Зависимости

### Блокирующие

Блокирующие: 187.1, supervisor/job lifecycle, supported Windows APIs and effect ledger.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Before-dispatch retry only with same valid observation; after possible dispatch = UnknownOutcome/no retry; helper generation invalidates tokens.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Windows fixture apps: UIA invoke/value, multi-window, unsupported/occluded, session unavailable, cancel/crash, foreground preservation.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Background refusal never auto-escalates; foreground requires explicit delivery mode/policy/approval.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
