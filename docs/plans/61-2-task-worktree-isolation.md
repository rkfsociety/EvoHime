# План 61.2 — Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач: runtime-интеграция и recovery

Статус: этап 2 для [плана 61.0](./61-0-task-worktree-isolation.md); после [плана 61.1](./61-1-task-worktree-isolation.md).

## Цель

Провести «Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 61.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 60.0 — зависимость из обзора.
- План 41.0 — зависимость из обзора.
- План 43.0 — зависимость из обзора.
- План 45.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/task_worktree_isolation.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `TaskWorktreeIsolationService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/task_worktree_isolation_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Core имеет durable TaskWorktree registry. → журналировать переходы и восстановление через replay/reconciliation.
- `C02` — Runs/children могут быть pinned к isolated worktree root. → провести через typed outcome, timeout, cancellation и idempotency.
- `C03` — Parallel agents не используют один mutable checkout по умолчанию при включённой isolation policy. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C06` — Worktree lifecycle/recovery/cleanup не теряет unintegrated changes. → журналировать переходы и восстановление через replay/reconciliation.
- `C07` — WorkspaceCheckpoints поддерживаются per worktree. → проверить exact revision/hash перед mutation и сохранить observed evidence.

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
