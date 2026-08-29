# План 29.3 — Continual Refinement с evidence и approval: IPC, client projection и UI

Статус: этап 3 для [плана 29.0](./29-0-continual-refinement.md); после [плана 29.2](./29-2-continual-refinement.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 29.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

### Protocol and projection contract

- После проверки свободных tags добавить additive versioned commands/events:
  list/get candidate metadata, evaluate/re-evaluate, approve, reject, activate,
  rollback и list history. Все mutation requests несут correlation,
  idempotency key, expected revision и при необходимости approval token.
- Projection включает schema version, candidate id/revision, kind/target,
  owner scope, lifecycle, bounded counts, confidence, evaluation status,
  conflict/error code, before/after hashes и provenance ids. Statement,
  transcript, secret/sensitive body и hidden reasoning не передаются.
- Electron main/preload только валидирует bounded payload и маршрутизирует
  Core-команды; renderer `OperationsPanel` отображает очередь/history/diff и
  явные actions, но не считает evidence threshold, не решает policy и не
  вызывает target registry напрямую.
- Reconnect/replay gap, duplicate event, stale action, denied approval,
  deleted source и unavailable PromptRule target должны отображаться typed
  состояниями, а не исчезать из очереди.
- Focused tests должны покрыть generated protocol, adapter allowlist,
  redaction/bounds, replay/resync, idempotency и UI projection.

### Acceptance-to-projection matrix

- `R29-C02`/`R29-C03` → version/revision, counts, conflict and typed eval
  projection.
- `R29-C05`/`R29-C06` → approve/reject/activate/rollback actions with stale and
  idempotency handling.
- `R29-C07`/`R29-C08` → authenticated additive protocol, redaction and replay
  tests.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.
- [ ] Queue/history/diff bounded, redacted и показывает unavailable/conflict/
  stale outcomes без локального решения policy.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
