# План 97.2 — Model Edit Protocol Registry: строгие patch/search-replace стратегии и repair feedback: runtime-интеграция и recovery

Статус: этап 2 для [плана 97.0](./97-0-model-edit-protocol-registry.md); после [плана 97.1](./97-1-model-edit-protocol-registry.md).

## Цель

Провести «Model Edit Protocol Registry: строгие patch/search-replace стратегии и repair feedback» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 97.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 39.0 — зависимость из обзора.
- План 60.0 — зависимость из обзора.
- План 70.0 — зависимость из обзора.
- План 84.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/model_edit_protocol_registry.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `ModelEditProtocolRegistryService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/model_edit_protocol_registry_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть versioned EditProtocol registry. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C02` — Минимум SEARCH/REPLACE + patch + structured/whole-file protocols оформлены явно. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C03` — Любой edit проходит parse + dry-run/preflight до mutation. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C05` — Ambiguous/fuzzy edits не применяются молча. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C06` — Failure feedback позволяет bounded repair только неуспешных edits. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C07` — Protocol selection привязан к ModelProfile/strategy, а не model-name branches. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.

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
