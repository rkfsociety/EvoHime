# План 34.2 — Event Trigger Runtime: безопасный запуск workflow по внешним событиям: runtime-интеграция и recovery

Статус: этап 2 для [плана 34.0](./34-0-event-trigger-runtime.md); после [плана 34.1](./34-1-event-trigger-runtime.md).

## Цель

Провести «Event Trigger Runtime: безопасный запуск workflow по внешним событиям» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 34.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 33.0 и конкретные provider implementations для provider-originated
  webhook paths; без них такие paths возвращают typed `unavailable`, а local
  workspace/system events работают через bounded local ingress.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/event_trigger_runtime.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `EventTriggerRuntimeService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Реализовать отдельные ingress paths для provider webhook и local workspace event: authenticity/schema до enqueue, bounded normalization, workspace scope, debounce/coalescing и ignore patterns; subscription reconciliation не создаёт пропущенные события.
- Для accepted-but-not-dispatched событий использовать durable accepted marker и idempotent dispatch; для storm/self-trigger loop применять chain depth, fingerprint suppression, queue overflow и circuit transitions до запуска workflow.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/event_trigger_runtime_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть versioned TriggerDefinition. → провести через typed outcome, timeout, cancellation и idempotency.
- `C02` — Есть normalized EventEnvelope. → нормализовать, дедуплицировать и ограничить вход до enqueue.
- `C03` — Webhook authenticity и schema проверяются до enqueue. → журналировать переходы и восстановление через replay/reconciliation.
- `C04` — Есть dedup/replay protection. → нормализовать, дедуплицировать и ограничить вход до enqueue.
- `C05` — Workflow version pinned. → провести через typed outcome, timeout, cancellation и idempotency.
- `C06` — Input mapping ограничивает payload. → выполнить allowlisted mapping до workflow enqueue; rejected/missing/invalid fields не попадают в runtime.
- `C07` — Есть rate limits/circuit breaker. → применить per-trigger rate/concurrency/queue bounds, coalescing и typed overflow/circuit outcomes до dispatch.
- `C08` — State durable/recoverable. → журналировать переходы и восстановление через replay/reconciliation.
- `C09` — Existing workflow approvals/grants сохраняются. → повторить authorization непосредственно перед dispatch/effect.

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
