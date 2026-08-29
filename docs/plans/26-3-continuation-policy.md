# План 26.3 — Continuation Policy и quality gates: IPC, client projection и UI

Статус: самостоятельный этап 3 для [плана 26.0](./26-0-continuation-policy.md); начинается после [плана 26.2](./26-2-continuation-policy.md).

## Цель

Дать desktop/CLI-клиенту минимальную typed поверхность для чтения состояния и явных действий пользователя, не перенося state или authority из Core.

## Зависимости

### Блокирующие

- План 26.2 — работающий Core vertical slice, recovery и список стабильных команд/events.
- Authenticated versioned named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация по шагам

0. Зарезервировать additive message/event names и tags после проверки proto; compatibility clients не меняют смысл старых команд.
1. Добавить request/result/event messages с bounded fields, correlation, idempotency, optimistic version и typed errors. Запретить raw prompt, credentials, hidden reasoning и необезличенный sensitive payload.
2. Реализовать Electron main/preload adapter; отдельный CLI/headless client
   добавляется только если он подтверждён live checkout и acceptance scope,
   иначе не входит в этот этап. Adapter только сериализует и маршрутизирует
   Core commands.
3. Добавить bounded renderer projection: status, progress, blockers, refs, policy/recovery warnings и user actions. Renderer не вычисляет state machine и не запускает effects.
4. Проверить reconnect, replay gap, duplicate events, stale action, denied action и отсутствующий optional backend.
5. Привязать UI/CLI traces к Core event/provenance IDs без вывода запрещённых payload.

## Предметная декомпозиция

- Proto: additive request/result/event DTO для `GetContinuationRun`,
  `PauseContinuation`, `StopContinuation`, `ResumeContinuation` и bounded
  event projection; tags резервируются после чтения текущего proto, без
  изменения старых oneof semantics.
- Electron: typed mapping в `pipe-client.ts`, command forwarding в
  `shell-bridge.ts`, projection state в shared API и panel-level tests.
- UI: Goal/Operations/Workflow surfaces показывают decision, gate status,
  budgets, blocker, recovery warning и stale-action error; raw prompt,
  provider output, credentials и hidden reasoning не проецируются.
- Tests: authenticated command tests, duplicate/stale actions, replay gap,
  redaction и reconnect с фактическим Core projection.

## Артефакты выхода

- additive protobuf contract и generated adapter types;
- main/preload/client bridge без обхода Core;
- bounded UI/CLI projection и typed error states;
- IPC, adapter и renderer/client tests на reconnect и tamper cases.

## Критерии выхода

- [ ] Новая surface additive и совместима с authenticated IPC.
- [ ] Все mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI не получает raw secrets, hidden reasoning или полномочия.
- [ ] Reconnect/replay и stale actions дают предсказуемый результат.
- [ ] UI показывает фактический Core state, а не локально вычисленную копию.

## Не входит

Новый transport, direct database access, model/provider credentials в client state или самостоятельная бизнес-логика renderer.
