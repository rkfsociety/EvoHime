# План 38.2 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas: runtime-интеграция и recovery

Статус: этап 2 для [плана 38.0](./38-0-adaptive-tool-catalog.md); после [плана 38.1](./38-1-adaptive-tool-catalog.md).

## Цель

Провести «Adaptive Tool Catalog: dynamic selection и deferred tool schemas» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 38.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 24.0 — зависимость из обзора.
- План 33.0 — зависимость из обзора.
- План 37.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/adaptive_tool_catalog.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `AdaptiveToolCatalogService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/adaptive_tool_catalog_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Full schemas загружаются только для выбранных tools. → провести через typed outcome, timeout, cancellation и idempotency.
- `C04` — Есть bounded max tool count и explicit fallback policy. → провести через typed outcome, timeout, cancellation и idempotency.
- `C05` — Поддержан хотя бы один deterministic и один semantic/model selector. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C06` — Provider-native deferred search является optional optimization. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C07` — Selection cache имеет безопасную invalidation policy. → провести через typed outcome, timeout, cancellation и idempotency.
- `C08` — Diagnostics показывают выбор и стоимость selector-а. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.

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
