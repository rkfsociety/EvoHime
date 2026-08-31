# План 52.2 — Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage: runtime-интеграция и recovery

Статус: этап 2 для [плана 52.0](./52-0-conversation-workbench.md); после [плана 52.1](./52-1-conversation-workbench.md).

## Цель

Провести «Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 52.1 — contract, validators, storage policy и errors.
- Conversation Event Log replay cursor и TaskCheckpoint refs.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- Agentic Browser Session и Revision-Safe Workspace Files capabilities из overview.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только заявленные registry/workflow/child/provider/tool surfaces. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/conversation_workbench.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `ConversationWorkbenchService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Workbench сам не dispatch-ит effects: user actions маршрутизируются в
  существующий authoritative service каждой capability. Unavailable tab не
  заменяется shell-derived guess; cross-links валидируются по conversation,
  workspace и backend snapshot.
- Тесты: `crates/evohime-core/tests/conversation_workbench_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C02` — Files/Diff/Tasks/Terminal/Browser/Usage представлены отдельными
  capability-aware tabs. → resolve each descriptor against the pinned
  conversation/workspace/backend capability snapshot and return typed
  availability/reason without dispatching an effect.

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
