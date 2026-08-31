# План 52.3 — Conversation Workbench: единая поверхность Files, Diff, Tasks, Terminal, Browser и Usage: IPC, client projection и UI

Статус: этап 3 для [плана 52.0](./52-0-conversation-workbench.md); после [плана 52.2](./52-2-conversation-workbench.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 52.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- Agentic Browser Session и Revision-Safe Workspace Files capabilities из overview.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `ConversationWorkbenchRequest`, `ConversationWorkbenchResponse`, `ConversationWorkbenchEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: собрать Workbench из существующих `OperationsPanel`, `TracePanel`,
  `ContextUsage`, task/conversation projections и capability-specific views;
  новый panel допустим как layout container, но не дублирует их state.

### Acceptance-to-projection matrix

- `C01` — Есть единый Conversation Workbench рядом с chat. → дать bounded projection и явные Core-checked actions.
- `C02` — Files/Diff/Tasks/Terminal/Browser/Usage представлены отдельными
  capability-aware tabs. → render descriptor registry, lazy-mount heavy tabs
  and keep unavailable tabs non-callable with an explicit reason.
- `C04` — Tabs scoped к текущей conversation/workspace/backend snapshot. → дать bounded projection и явные Core-checked actions.
- `C05` — Есть typed cross-links из conversation events в workbench resources. → дать bounded projection и явные Core-checked actions.
- `C06` — Presentation state безопасно сохраняется per conversation. → дать bounded projection и явные Core-checked actions.
- `C07` — Live updates используют общий event/projection механизм. → показывать состояние только из Core event/evidence, без локального вывода renderer.
- Switching conversations clears/rekeys live subscriptions before painting the
  next projection, preventing stale terminal/files/browser state flash.

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
