# План 57.2 — Plan Artifact: versioned planning contract и явный переход Plan → Execute: runtime-интеграция и recovery

Статус: этап 2 для [плана 57.0](./57-0-plan-artifact.md); после [плана 57.1](./57-1-plan-artifact.md).

## Цель

Провести «Plan Artifact: versioned planning contract и явный переход Plan → Execute» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 57.1 — contract, validators, storage policy и errors.
- Legacy planning/review paths adapted to the single PlanArtifact authority.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- UI/diagnostics integration из overview.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/plan_artifact.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `PlanArtifactService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/plan_artifact_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Planning и Execution являются явными режимами/переходами. → Core
  state machine and explicit `ExecutePlan` command.
- `C03` — Planning default не выполняет обычные mutating effects. → read-only
  planning capability snapshot and mutation-denial fixtures.
- `C06` — ExecutePlan фиксирует exact revision/hash и создаёт execution
  snapshot. → atomic binding before first effect and stale-hash rejection.
- `C07` — TaskCheckpoint отслеживает фактическое выполнение отдельно от plan.
  → runtime progress/deviations/evidence reference immutable plan revision.
- `C08` — Material deviations имеют явный re-plan path. → minor/material
  classifier, pause and append-only new revision flow.
- `C09` — Plan acceptance не заменяет security approvals. → re-run ordinary
  policy/approval immediately before every effect.

### Recovery contract

- Durable transitions восстанавливаются replay/reconciliation; transient work после restart получает typed `unknown`/`unavailable`, а не повтор side effect.
- Fault injection должна доказать отсутствие duplicate effect, потерю approval, обход policy или расширение capability set.

## Критерии выхода

- [ ] Planning/Execution explicit and planning default read-only.
- [ ] ExecutePlan pins exact revision/hash before the first side effect.
- [ ] TaskCheckpoint progress remains separate from immutable plan state.
- [ ] Material deviation pauses into re-plan; acceptance grants no approval.
- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
