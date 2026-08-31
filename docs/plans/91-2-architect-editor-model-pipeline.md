# План 91.2 — Architect-Editor Model Pipeline: раздельные reasoning и code-editing фазы: runtime-интеграция и recovery

Статус: этап 2 для [плана 91.0](./91-0-architect-editor-model-pipeline.md); после [плана 91.1](./91-1-architect-editor-model-pipeline.md).

## Цель

Провести «Architect-Editor Model Pipeline: раздельные reasoning и code-editing фазы» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 91.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 36.0 — зависимость из обзора.
- Tool Simulation Runtime v1 из `../architecture.md`.
- Model Resilience Policy v1 из `docs/architecture.md`.
- План 71.0 — зависимость из обзора.
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

- Entrypoint: `crates/evohime-core/src/architect_editor_model_pipeline.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `ArchitectEditorModelPipelineService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/architect_editor_model_pipeline_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Architect output оформлен как typed EditIntent. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C03` — Editor phase может использовать отдельный ModelProfile/edit protocol. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C04` — Same-model и separate-model modes поддерживаются. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C05` — Workspace drift проверяется между phases. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C06` — Failure routing/retries bounded и typed. → провести через typed outcome, timeout, cancellation и idempotency.
- `C07` — Все actual writes проходят существующий Core mutation/security boundary. → проверить exact revision/hash перед mutation и сохранить observed evidence.

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
