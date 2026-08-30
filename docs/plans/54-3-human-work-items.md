# План 54.3 — Human Work Items: пользователь как полноценный участник workflow/team, а не только approval: IPC, client projection и UI

Статус: этап 3 для [плана 54.0](./54-0-human-work-items.md); после [плана 54.2](./54-2-human-work-items.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 54.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 45.0 — зависимость из обзора.
- Event Trigger Runtime v1 — реализованный контракт из `../architecture.md`.
- План 35.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `HumanWorkItemsRequest`, `HumanWorkItemsResponse`, `HumanWorkItemsEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: создать `desktop/evohime-electron/src/renderer/src/HumanWorkItemsPanel.tsx` только как projection/action surface; тесты — `desktop/evohime-electron/tests/human_work_items.test.tsx` и protocol/typecheck gates.

### Acceptance-to-projection matrix

- `C01` — Есть durable HumanWorkItem contract/state machine. → дать bounded projection и явные Core-checked actions.
- `C02` — Human work отделён от security approvals. → дать bounded projection и явные Core-checked actions.
- `C05` — Human может занимать compatible Team Role slot. → дать bounded projection и явные Core-checked actions.
- `C06` — Work items переживают restart и имеют Inbox UI. → дать bounded projection и явные Core-checked actions.
- `C07` — Human response не расширяет runtime capabilities. → дать bounded projection и явные Core-checked actions.

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
