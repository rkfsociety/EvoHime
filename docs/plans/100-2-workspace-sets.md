# План 100.2 — Workspace Sets: multi-root и cross-repository задачи с независимыми grants, VCS и checkpoints: runtime-интеграция и recovery

Статус: этап 2 для [плана 100.0](./100-0-workspace-sets.md); после [плана 100.1](./100-1-workspace-sets.md).

## Цель

Провести «Workspace Sets: multi-root и cross-repository задачи с независимыми grants, VCS и checkpoints» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 100.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 60.0 — зависимость из обзора.
- План 61.0 — зависимость из обзора.
- План 58.0 — зависимость из обзора.
- План 73.0 — зависимость из обзора.
- План 89.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/workspace_sets.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `WorkspaceSetsService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/workspace_sets_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть durable/versioned WorkspaceSet с уникальными root aliases. → журналировать переходы и восстановление через replay/reconciliation.
- `C04` — Cross-root search/edit/tasks работают без implicit parent-directory authority. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C05` — Rules, Skills, Diagnostics и Checkpoints корректно scoped per root. → провести через typed outcome, timeout, cancellation и idempotency.
- `C06` — Multi-root checkpoints/worktree bindings имеют coordinated, но честно non-atomic semantics. → провести через typed outcome, timeout, cancellation и idempotency.
- `C07` — Partial outcomes/recovery фиксируются per root. → журналировать переходы и восстановление через replay/reconciliation.

### Recovery contract

- Durable transitions восстанавливаются replay/reconciliation; transient work после restart получает typed `unknown`/`unavailable`, а не повтор side effect.
- Fault injection должна доказать отсутствие duplicate effect, потерю approval, обход policy или расширение capability set.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
