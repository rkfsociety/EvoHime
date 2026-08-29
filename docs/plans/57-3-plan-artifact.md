# План 57.3 — Plan Artifact: versioned planning contract и явный переход Plan → Execute: IPC, client projection и UI

Статус: этап 3 для [плана 57.0](./57-0-plan-artifact.md); после [плана 57.2](./57-2-plan-artifact.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 57.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 23.0 — зависимость из обзора.
- План 40.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Критерии выхода

- [ ] IPC/UI показывают bounded redacted plan/criteria/evidence projection и явные actions.
- [ ] Execute/accept/replan actions повторно проверяются Core и не меняют accepted revision in-place.
- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
