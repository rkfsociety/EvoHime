# План 188.4 — Git/runtime/security recovery evidence and closure

Статус: active implementation contract. Этап следует после overview 188.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Verify bounded lifecycle/stop rules; update architecture/current-state/release evidence after implementation; close only with tested complete contract.

## Зависимости

### Блокирующие

Блокирующие: 188.1–188.3 and Windows Git/worktree/invocation evidence; 184/185 optional.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Repeat crash matrix; stale/recreated worktree cannot impersonate a node; cleanup cannot delete committed lineage.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Focused unit, Git integration, runtime/recovery, parallel, artifact/security and Windows checks; inspect CI artifacts and links.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Issue #164 acceptance proven; reproducible lineage, no unbounded loops or auto-promotion.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
