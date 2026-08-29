# План 33.3 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures: IPC, client projection и UI

Статус: этап 3 для [плана 33.0](./33-0-integration-provider-sdk.md); после [плана 33.2](./33-2-integration-provider-sdk.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 33.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer projection для Settings → Integrations и существующих
   Builder/Composer surfaces: status, scopes, blockers, refs, warnings и явные
   actions; отдельный CLI не добавлять, если в checkout нет существующего
   authenticated client contract. Renderer не вычисляет state machine и не
   запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать renderer trace к Core event/provenance IDs без запрещённых
   payload; CLI не добавлять без отдельного решения о существующем transport.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `IntegrationProviderSdkRequest`, `IntegrationProviderSdkResponse`, `IntegrationProviderSdkEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: определить существующие Settings/Builder/Composer entrypoints по live
  checkout; новый panel допустим только если нет подходящей composition point.
  Projection/action tests должны покрыть integration metadata, credential
  lifecycle status, dependency warning, action risk/scopes, missing binding,
  fixture status и отсутствие secret. Тестовый файл и component path
  подтверждаются на evidence freeze, а не считаются заранее существующим API.

### Acceptance-to-projection matrix

- `C01`/`C03` — provider/action identity, schemas, scopes and risk → bounded
  metadata projection, never manifest authority.
- `C02`/`C05` — credential lifecycle and dependency report → explicit user
  actions and warnings; secret values never cross IPC.
- `C04` — stable workflow binding → show provider/action/version and unresolved
  state from Core.
- `C06` — trigger capability → show declaration only; no trigger activation.
- `C07` — fixture status → show bounded pass/fail metadata, not mock payload.
- `C08` — secret boundary → protocol and redaction tests reject secret-shaped
  fields and payloads.

### Client safety and replay

- Mutation requests несут correlation/idempotency/optimistic version; Core повторно проверяет authorization и возвращает typed stale/denied/unavailable outcomes.
- Events bounded и redacted; reconnect/replay gap/duplicate отображаются явно, а renderer не вычисляет state machine и не запускает effect.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer получает только bounded projection (CLI не входит без existing
  authenticated surface).
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
