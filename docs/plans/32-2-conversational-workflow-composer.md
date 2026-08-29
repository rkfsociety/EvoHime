# План 32.2 — Conversational Workflow Composer: создание и правка workflow из естественного языка: runtime-интеграция и recovery

Статус: этап 2 для [плана 32.0](./32-0-conversational-workflow-composer.md); после [плана 32.1](./32-1-conversational-workflow-composer.md).

## Цель

Провести «Conversational Workflow Composer: создание и правка workflow из естественного языка» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 32.1 — contract, validators, storage policy и errors.
- План 31.0 — Builder authoring contract для передачи validated draft и сохранения immutable version.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.
- Existing Core model gateway/provider route, model provenance/receipt и bounded catalog surfaces — для безопасной генерации proposal; Composer не вызывает provider напрямую.

### Опциональные

- План 30.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy/catalog/model-route snapshot для active composition session.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Вызвать модель только через Core gateway с bounded catalog snapshot; после ответа выполнить parse -> proposal validation -> binding -> static validation -> risk preview. Модельный вызов не имеет tool/effect authority; provider unavailable, timeout, malformed output и catalog drift дают typed outcome.
3. Подключить только заявленные registry/workflow/child/provider/tool surfaces для binding/preview. Optional integration даёт typed missing/unavailable, а не выдуманную capability.
4. Формализовать timeout, cancellation, bounded repair loops, backpressure, partial failure, duplicate/idempotency и stale draft revision; после restart unsaved session теряется предсказуемо, accepted immutable graph читается через Builder storage, без blind retry или автоматического запуска.
5. Сделать fault-injection для crash до/после model dispatch, duplicate delivery, policy/catalog change, malformed response и save failure; зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/conversational_workflow_composer.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `ConversationalWorkflowComposerService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/conversational_workflow_composer_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Ева умеет создавать workflow draft из natural language. → провести через typed outcome, timeout, cancellation и idempotency.
- `C02` — Proposal отделён от authoritative workflow contract. → провести через typed outcome, timeout, cancellation и idempotency.
- `C04` — Есть iterative typed edits. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C06` — Есть risk/side-effect preview. → повторить authorization непосредственно перед dispatch/effect.
- `C07` — Draft можно открыть в builder и сохранить как immutable version. → передать validated draft в Builder authoring API с optimistic revision и explicit Save.

### Recovery contract

- Durable transitions восстанавливаются replay/reconciliation; transient work после restart получает typed `unknown`/`unavailable`, а не повтор side effect.
- Fault injection должна доказать отсутствие duplicate effect, потерю approval, обход policy или расширение capability set.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable имеют отдельные outcomes.
- [ ] Composer не выполняет external effect и не повторяет model dispatch автоматически.
- [ ] Active composition session pinned к exact contract/policy/catalog/model-route snapshot.
- [ ] Recovery/fault-injection tests воспроизводимы.
- [ ] Model invocation не имеет tool/effect authority и имеет bounded unavailable/invalid outcomes.
- [ ] Unsaved restart и accepted save/reload имеют разные доказанные outcomes.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
