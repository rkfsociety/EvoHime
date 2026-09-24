# План 188.1 — campaign/node/run/metric contracts and lineage

Статус: active implementation contract. Этап следует после overview 188.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Version campaign/hypothesis/node/run/metric, single-parent acyclic lineage, lifecycle, idempotency/fencing and hard budgets; extend current runtime storage owner additively.

## Зависимости

### Блокирующие

Блокирующие: storage migration/backup, worktree identity, benchmark/evidence refs, policy/capability snapshots. Plans 184/185 evidence optional.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Recovery и rollback

Frozen baseline immutable; duplicate hash conflict; single parent; unknown outcome not candidate; sibling approval isolated.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Unit: hash/lifecycle/tree/branch/depth/node/budget/idempotency/metric compatibility/stale policy.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Store bounded refs/metadata only; no source bytes, full logs, prompts, datasets, secrets or weights.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
