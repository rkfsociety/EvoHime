# План 112.2 — Batch Invocation Runtime: bounded map execution по наборам inputs с per-item isolation и resume: runtime-интеграция и recovery

Статус: этап 2 для [плана 112.0](./112-0-batch-invocation-runtime.md); после [плана 112.1](./112-1-batch-invocation-runtime.md).

## Цель

Провести «Batch Invocation Runtime: bounded map execution по наборам inputs с per-item isolation и resume» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 112.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 25.0 — зависимость из обзора.
- План 26.0 — зависимость из обзора.
- План 63.0 — зависимость из обзора.
- План 45.0 — зависимость из обзора.
- План 62.0 — зависимость из обзора.
- План 83.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/batch_invocation_runtime.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `BatchInvocationRuntimeService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/batch_invocation_runtime_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть durable BatchInvocation/BatchItem contracts. → журналировать переходы и восстановление через replay/reconciliation.
- `C03` — Каждый item имеет отдельный run/state/provenance. → журналировать переходы и восстановление через replay/reconciliation.
- `C04` — Concurrency и per-item/global budgets bounded. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Batch переживает Core restart и продолжает Pending work без дублей. → журналировать переходы и восстановление через replay/reconciliation.
- `C06` — Partial failures и approvals не теряют прогресс остальных items. → повторить authorization непосредственно перед dispatch/effect.
- `C07` — Retry учитывает idempotency/unknown-outcome semantics. → провести через typed outcome, timeout, cancellation и idempotency.

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
