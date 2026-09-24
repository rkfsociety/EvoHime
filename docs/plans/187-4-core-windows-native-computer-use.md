# План 187.4 — GUI regressions, package acceptance и release evidence

Статус: active implementation contract. Этап следует после overview 187.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Complete Windows action fixture matrix, GUI behavioral scenarios and Benchmark evidence; update architecture/current-state/release docs after implementation.

## Зависимости

### Блокирующие

Блокирующие: 187.1–187.3, plans 184/185 evidence and Windows CI fixture availability.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Recheck crash, unknown outcome, stale recreated window, cancel, focus restoration and RDP/session transition; rollback disables admission.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Focused Windows UIA/E2E proves exact target, focus preservation, explicit foreground approval, refusals and package inclusion.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Issue #163 acceptance evidence complete; actuator acknowledgement alone is not task success.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
