# План 104.3 — Remote Conversation Channels: безопасное управление Евой через Telegram, Slack и другие мессенджеры: IPC, client projection и UI

Статус: этап 3 для [плана 104.0](./104-0-remote-conversation-channels.md); после [плана 104.2](./104-2-remote-conversation-channels.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 104.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 94.0 — зависимость из обзора.
- Event Trigger Runtime v1 — реализованный контракт из `../architecture.md`.
- План 40.0 — зависимость из обзора.
- План 98.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `RemoteConversationChannelsRequest`, `RemoteConversationChannelsResponse`, `RemoteConversationChannelsEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: создать `desktop/evohime-electron/src/renderer/src/RemoteConversationChannelsPanel.tsx` только как projection/action surface; тесты — `desktop/evohime-electron/tests/remote_conversation_channels.test.tsx` и protocol/typecheck gates.

### Acceptance-to-projection matrix

- `C05` — Conversation routing и dedup Core-owned. → дать bounded projection и явные Core-checked actions.
- `C06` — Есть streaming/final reply abstraction. → дать bounded projection и явные Core-checked actions.
- `C07` — Human Work Item можно безопасно закрыть из поддерживаемого channel. → дать bounded projection и явные Core-checked actions.

### Client safety and replay

- Mutation requests несут correlation/idempotency/optimistic version; Core повторно проверяет authorization и возвращает typed stale/denied/unavailable outcomes.
- Events bounded и redacted; reconnect/replay gap/duplicate отображаются явно, а renderer не вычисляет state machine и не запускает effect.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
