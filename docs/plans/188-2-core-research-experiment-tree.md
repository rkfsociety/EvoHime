# План 188.2 — worktree snapshot, invocation dispatch, budgets/recovery

Статус: active implementation contract. Этап следует после overview 188.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Extend existing Task Worktree/Git owner through actual clean-state prep and exact commit record; content-addressed snapshot; dispatch through versioned Workflow/Invocation/Recipe owner; bound child parallelism.

## Зависимости

### Блокирующие

Блокирующие: 188.1, real Git/worktree effect owner (metadata contract alone is insufficient), ArtifactStore, invocation, durable child and receipts.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Crash matrix: before worktree/commit/archive/dispatch, active run, result→evaluation. Unknown effects reconcile, never blind retry or duplicate node.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Git integration: sibling isolation, dirty rejection, exact commit/archive hashes, changed HEAD, cleanup lineage and cancellation.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Run inputs frozen to commit/invocation/policy/capability; no arbitrary shell or mutable host source.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
