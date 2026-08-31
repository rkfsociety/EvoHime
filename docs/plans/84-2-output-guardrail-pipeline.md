# План 84.2 — Output Guardrail Pipeline: semantic validators, transforms и bounded correction loops: runtime-интеграция и recovery

Статус: этап 2 для [плана 84.0](./84-0-output-guardrail-pipeline.md); после [плана 84.1](./84-1-output-guardrail-pipeline.md).

## Цель

Провести «Output Guardrail Pipeline: semantic validators, transforms и bounded correction loops» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 84.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Structured Response Contract v1 из канонической архитектуры.
- План 40.0 — зависимость из обзора.
- План 57.0 — зависимость из обзора.
- План 69.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/output_guardrail_pipeline.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `OutputGuardrailPipelineService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/output_guardrail_pipeline_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Поддержаны ordered deterministic и model-based checks. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C04` — Retry/correction loop bounded и durable. → журналировать переходы и восстановление через replay/reconciliation.
- `C06` — Acceptance привязана к exact output/artifact revision. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C07` — Security approvals и real-effect evidence остаются отдельными слоями. → повторить authorization непосредственно перед dispatch/effect.

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
