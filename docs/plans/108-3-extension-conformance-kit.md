# План 108.3 — Extension Conformance Kit: contract tests и transactional registration для providers/adapters/extensions: IPC, client projection и UI

Статус: этап 3 для [плана 108.0](./108-0-extension-conformance-kit.md); после [плана 108.2](./108-2-extension-conformance-kit.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 108.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 33.0 — зависимость из обзора.
- План 37.0 — зависимость из обзора.
- План 45.0 — зависимость из обзора.
- План 69.0 — зависимость из обзора.
- План 74.0 — зависимость из обзора.
- План 76.0 — зависимость из обзора.
- План 106.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
