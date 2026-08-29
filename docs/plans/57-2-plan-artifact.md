# План 57.2 — Plan Artifact: versioned planning contract и явный переход Plan → Execute: runtime-интеграция и recovery

Статус: этап 2 для [плана 57.0](./57-0-plan-artifact.md); после [плана 57.1](./57-1-plan-artifact.md).

## Цель

Провести «Plan Artifact: versioned planning contract и явный переход Plan → Execute» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 57.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 23.0 — зависимость из обзора.
- План 40.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Критерии выхода

- [ ] Переход `Plan -> Execute` выполняется только Core-командой до первого side effect.
- [ ] Accepted plan revision/hash закреплён на run; material deviation даёт revalidation или replan.
- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run pinned к exact contract/policy snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
