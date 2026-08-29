# План 114.2 — Code-Anchored Intent Markers: задачи и вопросы Еве прямо из комментариев в исходниках: runtime-интеграция и recovery

Статус: этап 2 для [плана 114.0](./114-0-code-anchored-intent-markers.md); после [плана 114.1](./114-1-code-anchored-intent-markers.md).

## Цель

Провести «Code-Anchored Intent Markers: задачи и вопросы Еве прямо из комментариев в исходниках» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 114.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 70.0 — зависимость из обзора.
- План 73.0 — зависимость из обзора.
- План 75.0 — зависимость из обзора.
- План 57.0 — зависимость из обзора.
- План 82.0 — зависимость из обзора.
- План 97.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/code_anchored_intent_markers.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `CodeAnchoredIntentMarkersService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/code_anchored_intent_markers_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Поддержаны Question и EditRequest markers. → проверить exact revision/hash перед mutation и сохранить observed evidence.
- `C06` — Existing/agent-generated content не auto-trigger-ится. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C07` — Есть debounce/dedup/stale/loop protection. → нормализовать, дедуплицировать и ограничить вход до enqueue.
- `C08` — Marker запускает обычный EvoHime task, не отдельный небезопасный runtime. → провести через typed outcome, timeout, cancellation и idempotency.

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
