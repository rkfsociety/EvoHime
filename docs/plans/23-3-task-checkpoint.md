# План 23.3 — TaskCheckpoint для compaction и recovery: IPC, client projection и UI

Статус: самостоятельный этап 3 для [плана 23.0](./23-0-task-checkpoint.md); начинается после [плана 23.2](./23-2-task-checkpoint.md).

## Цель

Дать desktop/CLI-клиенту минимальную typed поверхность для чтения состояния и явных действий пользователя, не перенося state или authority из Core.

## Зависимости

### Блокирующие

- План 23.2 — работающий Core vertical slice, recovery и список стабильных команд/events.
- Authenticated versioned named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация по шагам

0. Зарезервировать additive message/event names и tags после проверки proto; compatibility clients не меняют смысл старых команд.
1. Добавить request/result/event messages с bounded fields, correlation, idempotency, optimistic version и typed errors. Запретить raw prompt, credentials, hidden reasoning и необезличенный sensitive payload.
2. Реализовать Electron main/preload adapter и, если предусмотрено обзором, headless client; adapter только сериализует и маршрутизирует Core commands.
3. Добавить bounded renderer projection: status, progress, blockers, refs, policy/recovery warnings и user actions. Renderer не вычисляет state machine и не запускает effects.
4. Проверить reconnect, replay gap, duplicate events, stale action, denied action и отсутствующий optional backend.
5. Привязать UI/CLI traces к Core event/provenance IDs без вывода запрещённых payload.

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
