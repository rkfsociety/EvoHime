# План 104.2 — Remote Conversation Channels: безопасное управление Евой через Telegram, Slack и другие мессенджеры: runtime-интеграция и recovery

Статус: этап 2 для [плана 104.0](./104-0-remote-conversation-channels.md); после [плана 104.1](./104-1-remote-conversation-channels.md).

## Цель

Провести «Remote Conversation Channels: безопасное управление Евой через Telegram, Slack и другие мессенджеры» через Core runtime: validation -> authorization -> bounded operation -> typed result/event -> recovery.

## Зависимости

### Блокирующие

- План 104.1 — contract, validators, storage policy и errors.
- Existing workflow/child/provider/tool/memory boundaries, budgets, cancellation, audit и unknown-outcome semantics.

### Опциональные

- План 94.0 — зависимость из обзора.
- Event Trigger Runtime v1 — реализованный контракт из `../architecture.md`.
- План 40.0 — зависимость из обзора.
- План 98.0 — зависимость из обзора.

## Реализация

0. Загрузить stage-1 artifacts и закрепить immutable contract/policy snapshot active run.
1. Добавить Core handler/state machine; повторить authorization непосредственно перед effect и связать result/event с correlation + idempotency.
2. Подключить только registry/workflow/child/provider/tool surfaces, предусмотренные обзором. Optional backend даёт typed unavailable/degraded.
3. Формализовать timeout, lease, retry, backpressure, partial failure, cancellation и unknown outcome; после restart только replay/reconciliation, без blind retry.
4. Сделать fault-injection для crash до/после dispatch, stale version/lease, duplicate delivery, policy change и corruption.
5. Зафиксировать metadata-only projection и redacted evidence для этапов 3–4.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/remote_conversation_channels.rs` + handler в `crates/evohime-core/src/lib.rs`; сервис `RemoteConversationChannelsService` должен выполнять `validate → policy → bounded operation → typed result/event`.
- На старте run загрузить exact contract/policy snapshot и проверить correlation, idempotency, budget, cancellation и capability grant непосредственно перед effect.
- Для каждого внешнего/необратимого вызова записать before/after-dispatch evidence; unknown outcome переводить в reconciliation, без blind retry.
- Тесты: `crates/evohime-core/tests/remote_conversation_channels_recovery.rs` — timeout/cancel, duplicate, stale version/lease, crash до/после dispatch, restart и optional-unavailable.

### Acceptance-to-runtime matrix

- `C01` — Есть versioned ChannelProvider/ChannelConnection contracts. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C04` — Inbound queue/rate/attachment limits bounded. → журналировать переходы и восстановление через replay/reconciliation.
- `C08` — High-risk remote approvals deny-by-default. → повторить authorization непосредственно перед dispatch/effect.
- `C09` — Provider credentials и outbound data проходят обычные sensitive-data boundaries. → разрешить Core snapshot, проверить capability/locality и закрепить его на run.
- `C10` — Connections переживают restart и могут быть немедленно revoked. → журналировать переходы и восстановление через replay/reconciliation.

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
