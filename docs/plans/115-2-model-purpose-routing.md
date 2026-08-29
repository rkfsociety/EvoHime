# План 115.2 — Model Purpose Routing: отдельные model profiles для primary, editor, selector, summarizer и auxiliary calls: runtime-интеграция и recovery

Статус: этап 2 для [плана 115.0](./115-0-model-purpose-routing.md); после [плана 115.1](./115-1-model-purpose-routing.md).

## Цель

Провести «Model Purpose Routing: отдельные model profiles для primary, editor, selector, summarizer и auxiliary calls» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 115.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 39.0 — зависимость из обзора.
- План 42.0 — зависимость из обзора.
- План 67.0 — зависимость из обзора.
- План 36.0 — зависимость из обзора.
- План 46.0 — зависимость из обзора.
- План 59.0 — зависимость из обзора.
- План 71.0 — зависимость из обзора.
- План 83.0 — зависимость из обзора.
- План 105.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/model_purpose_routing.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `ModelPurposeRoutingService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/model_purpose_routing_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Есть versioned ModelPurposeRoutingPolicy. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C03` — Internal subsystems запрашивают модель через purpose вместо собственных raw model-name settings. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C05` — Routing проверяет model capabilities, trust/locality и budget. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C06` — Retry/fallback остаётся отдельной Model Resilience Policy. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C07` — Exact purpose/profile фиксируются в model-call provenance и usage. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.

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
