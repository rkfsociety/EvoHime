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

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
