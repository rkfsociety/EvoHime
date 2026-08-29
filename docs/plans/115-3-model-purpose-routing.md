# План 115.3 — Model Purpose Routing: отдельные model profiles для primary, editor, selector, summarizer и auxiliary calls: IPC, client projection и UI

Статус: этап 3 для [плана 115.0](./115-0-model-purpose-routing.md); после [плана 115.2](./115-2-model-purpose-routing.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 115.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 39.0 — зависимость из обзора.
- План 42.0 — зависимость из обзора.
- План 67.0 — зависимость из обзора.
- План 36.0 — зависимость из обзора.
- План 46.0 — зависимость из обзора.
- План 59.0 — зависимость из обзора.
- План 71.0 — зависимость из обзора.
- План 83.0 — зависимость из обзора.
- План 105.0 — зависимость из обзора.

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
