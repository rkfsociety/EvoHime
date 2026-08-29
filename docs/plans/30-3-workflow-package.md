# План 30.3 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей: IPC, client projection и UI

Статус: этап 3 для [плана 30.0](./30-0-workflow-package.md); после [плана 30.2](./30-2-workflow-package.md).

## Цель

Добавить typed client surface для export, import preview, credential rebinding и
явного commit без переноса authority.

## Зависимости

### Блокирующие

- План 30.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages для export, preview, resolve,
   rebind и commit с correlation, idempotency, optimistic version и typed
   errors; исключить secrets, raw prompt/output, package bytes сверх лимита и
   hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive package commands/results/events (export, preview,
  rebind, commit) и command/event oneof после проверки свободных tags; сохранить
  major, replay/resync и bounded frame limits. Commit не должен принимать
  capability definition или credential value.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: создать `WorkflowPackagePanel.tsx` как projection/action surface:
  export preview/stripped fields/hash/dependencies и import report; кнопка
   commit disabled при обязательных unresolved dependencies или отсутствии
   rebinding. Credential picker возвращает renderer только opaque slot state;
   raw secret и даже provider credential value не покидают Core-owned store.
   Тесты покрывают отсутствие parse/preview effect и redaction.

### Acceptance-to-projection matrix

- `C08` — Import не расширяет Core capability registry. → показывать только
  Core report; renderer не регистрирует capability и не считает resolution.
- `C04` — Import выполняет validate/resolve/preview до записи. → разделить
  preview и commit в UI и явно показывать phase/blocked reason.

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
