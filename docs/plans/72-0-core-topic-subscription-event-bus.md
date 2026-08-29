# План 72.0 — Core Topic/Subscription Event Bus: typed pub/sub routing for agent runtime

Статус: предложено по [issue #52](https://github.com/rkfsociety/EvoHime/issues/52). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime внутренний **typed topic/subscription event bus** для слабосвязанной коммуникации между agent runtime, workflows, retained children, integrations и системными сервисами.

Это внутренний Core-механизм. Он не должен превращать локальное приложение в обязательный distributed broker zoo с Kafka ради отправки трёх событий.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/core-topic-subscription-event-bus.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./72-1-core-topic-subscription-event-bus.md)
- [Этап 2 — runtime-интеграция и recovery](./72-2-core-topic-subscription-event-bus.md)
- [Этап 3 — IPC, client projection и UI](./72-3-core-topic-subscription-event-bus.md)
- [Этап 4 — verification, release-evidence и закрытие](./72-4-core-topic-subscription-event-bus.md)

## Зависимости

### Блокирующие

- План 51.0 — Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```rust
struct TopicId {
    namespace: String,
    name: String,
    partition_key: Option<String>,
}

struct EventEnvelope<T> {
    event_id: EventId,
    schema: EventSchemaId,
    schema_version: u32,
    topic: TopicId,

    producer: RuntimeIdentity,
    workflow_run_id: Option<WorkflowRunId>,
    goal_id: Option<GoalId>,

    correlation_id: CorrelationId,
    causation_id: Option<EventId>,

    created_at: Timestamp,
    payload: T,
}
```

Correlation и causation должны быть first-class, иначе через полгода runtime timeline станет коллекцией загадочных timestamp'ов, как обычно и происходит с «простым event bus».

### Безопасность

Topic publish/subscribe должны проходить capability checks.

Например:

```text
runtime.workflow.events.read
runtime.workflow.events.publish
runtime.child.events.read
integration.github.events.consume
```

Наличие доступа к одному namespace не означает доступ ко всему payload другого workflow/user context.

EventEnvelope не должен содержать raw credentials.

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

- [ ] Topic/EventEnvelope/Subscription contracts versioned
- [ ] Ephemeral и Durable delivery поддержаны
- [ ] Durable delivery имеет ACK/NACK/retry/dead-letter
- [ ] Correlation/causation являются first-class fields
- [ ] Есть backpressure и idempotency model
- [ ] Publish/subscribe защищены capabilities
- [ ] Restart не теряет correctness-critical events
- [ ] Local Core implementation не требует внешнего broker

## Ограничения и non-goals

- Не строить distributed microservice platform в первой версии.
- Не заменять workflow state machine event bus'ом.
- Не публиковать secrets в payload.
- Не давать renderer unrestricted subscription access.
- Не гарантировать бессмысленный global total order.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#52 Core Topic/Subscription Event Bus: typed pub/sub routing for agent runtime](https://github.com/rkfsociety/EvoHime/issues/52)
