# План 49.0 — Resumable Conversation Event Log: cursor-based history, live sync и reconnect без дублей

Статус: предложено по [issue #29](https://github.com/rkfsociety/EvoHime/issues/29). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Resumable Conversation Event Log**: типизированный append-only журнал событий conversation/run, который служит общей основой для истории чата, live streaming, terminal/browser/file activity, usage, task/goal state и безопасного восстановления UI после reconnect/restart.

Ключевая задача — объединить исторические и живые события так, чтобы renderer мог:

- быстро показать уже известную историю;
- подписаться только на новые события;
- пережить потерю соединения;
- восстановить пропущенный диапазон;
- не получить дубли после reconnect;
- корректно reconciliate optimistic пользовательские сообщения с authoritative событиями Core.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/resumable_conversation_event_log.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./49-1-resumable-conversation-event-log.md)
- [Этап 2 — runtime-интеграция и recovery](./49-2-resumable-conversation-event-log.md)
- [Этап 3 — IPC, client projection и UI](./49-3-resumable-conversation-event-log.md)
- [Этап 4 — verification, release-evidence и закрытие](./49-4-resumable-conversation-event-log.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Conversation имеет Core-owned monotonic event sequence.
- [ ] History API использует cursor-based pagination.
- [ ] Live subscription умеет resume с `after_sequence`.
- [ ] Renderer обнаруживает gaps и duplicates.
- [ ] Outgoing messages имеют stable `client_message_id` и idempotent reconciliation.
- [ ] Streaming deltas отделены от authoritative finalized event.
- [ ] Chat/terminal/browser/tasks/usage строятся как projections одного log.
- [ ] Restart/reconnect не требует пересылать всю историю и не создаёт дублей.

## Ограничения и non-goals

- exactly-once network transport;
- хранение каждого token delta навсегда;
- глобальный event bus между всеми пользователями/машинами;
- использование timestamp как единственного cursor;
- хранение raw secrets ради полного replay;
- превращение renderer cache в authoritative storage;
- удаление существующего TaskCheckpoint/Workflow recovery в пользу conversation log.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#29 Resumable Conversation Event Log: cursor-based history, live sync и reconnect без дублей](https://github.com/rkfsociety/EvoHime/issues/29)
