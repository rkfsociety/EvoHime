# План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries

Статус: предложено по [issue #49](https://github.com/rkfsociety/EvoHime/issues/49). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Core-owned Runtime Intervention Pipeline**: упорядоченный набор типизированных middleware/interceptors, которые могут наблюдать, валидировать, модифицировать, блокировать или приостанавливать runtime operations на заранее определённых границах.

Главная идея: cross-cutting policy не должна размазываться по каждому agent/tool/provider. Но middleware при этом не становится новым источником authority: все security-sensitive решения остаются в Core.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/runtime-intervention-pipeline.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 51.0 — Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox.
- План 72.0 — Core Topic/Subscription Event Bus: typed pub/sub routing for agent runtime.
- План 110.0 — Message Intervention Policies: typed interceptors для agent/team message routing.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Минимальный набор:

```text
before_model_request
after_model_response
before_agent_message_delivery
after_agent_message_delivery
before_tool_dispatch
after_tool_result
before_handoff
before_workflow_state_commit
before_external_publish
```

Hook point должен иметь строгий input/output contract, а не `Any -> Any`.

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

- [ ] Все hook points имеют typed contracts
- [ ] Handler ordering детерминирован
- [ ] Есть explicit decision enum
- [ ] Security handlers fail closed
- [ ] Модификации полностью аудируемы без утечки секретов
- [ ] Approval handler не может self-approve
- [ ] Есть recursion/reentrancy protection
- [ ] UI объясняет intervention пользователю

## Ограничения и non-goals

- Не делать middleware альтернативой Core permissions.
- Не разрешать renderer регистрировать authoritative security handler.
- Не давать произвольному plugin мутировать любой runtime object.
- Не сохранять hidden chain-of-thought или сырые секреты в audit.
- Не превращать каждый обычный log hook в blocking middleware.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#49 Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries](https://github.com/rkfsociety/EvoHime/issues/49)
