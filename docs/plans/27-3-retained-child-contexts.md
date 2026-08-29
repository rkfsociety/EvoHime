# План 27.3 — Retained Child Contexts и mailbox: IPC, client projection и UI

Статус: самостоятельный этап 3 для [плана 27.0](./27-0-retained-child-contexts.md); начинается после [плана 27.2](./27-2-retained-child-contexts.md).

## Зависимости

### Блокирующие

- работающий Core vertical slice/recovery и command-event inventory из
  [27.2](./27-2-retained-child-contexts.md);
- `crates/desktop-ipc/proto/evohime.desktop.proto`, Rust transport,
  `desktop/EvoHime.IpcTests`, Electron main/preload adapter и generated protocol
  check; C# compatibility oracle обновляется вместе с additive proto.

### Опциональные

- CLI/headless client не входит в MVP; его отсутствие не блокирует этап и не
  должно порождать отдельную обязательную surface.

## Реализация по шагам

0. Проверить live oneof/event tags (сейчас последняя занятая command tag — 156),
   выбрать следующий свободный диапазон без reuse, записать names/tags/schema
   revision в evidence и обновить Rust/C#/TypeScript generated surfaces.
1. Добавить additive typed messages `ListRetainedChildren`, `GetRetainedChild`,
   `RetainChild`, `SendChildFollowUp` и `DeleteRetainedChild`, а также bounded
   registry/mailbox projections. Поля correlation, idempotency,
   optimistic version и typed error обязательны; Core игнорирует client actor,
   sender и receiver claims.
2. Реализовать handler в `crates/evohime-core/src/ipc_bridge.rs`, Electron
   `desktop/evohime-electron/src/main`/preload adapter и C# compatibility
   mappings. Adapter только сериализует и маршрутизирует; storage/state machine
   остаются в Core.
3. Добавить OperationsPanel projection: lifecycle, role/name, revision,
   activity, pending count, TTL, stale/invalidated reason, Goal/workflow refs,
   delivery outcome и explicit actions. Renderer не строит state machine и не
   получает prompt, transcript, secret, hidden reasoning или absolute path.
4. Проверить reconnect, replay gap/resync, duplicate events, stale action,
   denied action, deleted/expired child, redaction и old-client behavior.
   Compatibility tests должны доказать, что старый client игнорирует additive
   surface, а новый не обходит Core.

## Артефакты и критерии выхода

- additive proto contract с exact tags/schema и generated Rust/C#/TS types;
- Core IPC handlers, main/preload bridge и metadata-only UI projection;
- Rust IPC, C# compatibility, adapter и renderer tests на reconnect/tamper;
- все mutations проходят Core authorization, idempotency и optimistic version;
- UI показывает фактический Core state и различает pending/delivered/unknown,
  без capability escalation.

## Не входит

Новый transport, direct database access, headless client как blocking scope,
credentials в client state и самостоятельная бизнес-логика renderer.
