# План 108.2 — Extension Conformance Kit: contract tests и transactional registration для providers/adapters/extensions: runtime-интеграция и recovery

Статус: этап 2 для [плана 108.0](./108-0-extension-conformance-kit.md); после [плана 108.1](./108-1-extension-conformance-kit.md).

## Цель

Провести «Extension Conformance Kit: contract tests и transactional registration для providers/adapters/extensions» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 108.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 33.0 — зависимость из обзора.
- План 37.0 — зависимость из обзора.
- План 45.0 — зависимость из обзора.
- План 69.0 — зависимость из обзора.
- План 74.0 — зависимость из обзора.
- План 76.0 — зависимость из обзора.
- План 106.0 — зависимость из обзора.

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
