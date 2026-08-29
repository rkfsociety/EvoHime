# План 28.3 — Persistent Analysis Kernel: IPC, client projection и UI

Статус: этап 3 для [плана 28.0](./28-0-persistent-analysis-kernel.md); после [плана 28.2](./28-2-persistent-analysis-kernel.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 28.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.
- Contract/schema/event inventory из 28.1–28.2 и live proto; numeric tags
  назначаются только после проверки текущего последнего command tag (156) и
  event oneof, с сохранением старых generated clients.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Проверить proto и зарезервировать additive names/tags после 28.2; записать
   exact command/event tags, schema version и old-client behavior в evidence.
1. Добавить bounded request/result/event messages с correlation, idempotency,
   optimistic version, projection budget и typed errors; исключить secrets, raw
   prompt, hidden reasoning, arbitrary paths и object bytes.
2. Реализовать Core handlers в `crates/evohime-core/src/ipc_bridge.rs`, затем
   Electron main/preload adapter; adapter только сериализует и маршрутизирует,
   а authorization/storage/state machine остаются в Core.
3. Добавить bounded renderer projection/diagnostics для status, progress,
   blockers, refs, warnings, limits, reset и actions. CLI не входит в MVP;
   отсутствие CLI даёт documented omission, не вторую обязательную surface.
4. Проверить reconnect, replay gap/resync, duplicate event, stale/denied action,
   redaction, old-client compatibility и unavailable optional backend.
5. Привязать UI trace к Core event/provenance IDs без запрещённых payload и
   добавить negative test, что renderer не может запросить raw object value.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.
- [ ] Exact tags/schema и generated Rust/C#/TypeScript surfaces зафиксированы;
  старые clients продолжают handshake/replay и не получают authority.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
