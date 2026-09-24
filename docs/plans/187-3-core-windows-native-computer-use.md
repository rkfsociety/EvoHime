# План 187.3 — ToolRegistry/policy/approval/IPC projections

Статус: active implementation contract. Этап следует после overview 187.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Route through ToolRegistry/capability, PolicyGate, exact-scope approval, ReceiptRuntime and EventJournal; expose only bounded status/reason/approval over authenticated IPC.

## Зависимости

### Блокирующие

Блокирующие: 187.2, manifests, approval/receipt and authenticated IPC; plans 184/185 provide trace/scenario fixtures.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Approval binds target, observation, action, policy and adapter hashes; payload drift invalidates it. Cancellation records possible effect.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Checks: no effect before approval, stale approval refusal, denial/no effect, dedup, no raw screenshot/text in IPC/trace.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Renderer presents projection only; grounding candidate cannot mint target/pixel authority.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
