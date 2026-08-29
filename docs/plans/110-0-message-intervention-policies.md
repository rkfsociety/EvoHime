# План 110.0 — Message Intervention Policies: typed interceptors для agent/team message routing

Статус: предложено по [issue #90](https://github.com/rkfsociety/EvoHime/issues/90). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный **Message Intervention Policy** слой для agent/team communications: Core-owned перехватчики, которые могут наблюдать, валидировать, блокировать, редактировать безопасную projection или перенаправлять сообщения **до** доставки участнику.

Это не замена Agent Middleware Pipeline и не замена Collaboration Bus.

- Middleware работает вокруг agent/model/tool execution.
- Collaboration Bus доставляет typed messages.
- Intervention Policy управляет тем, **можно ли конкретное сообщение доставить в текущем контексте и в каком безопасном виде**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/message-intervention-policies.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./110-1-message-intervention-policies.md)
- [Этап 2 — runtime-интеграция и recovery](./110-2-message-intervention-policies.md)
- [Этап 3 — IPC, client projection и UI](./110-3-message-intervention-policies.md)
- [Этап 4 — verification, release-evidence и закрытие](./110-4-message-intervention-policies.md)

## Зависимости

### Блокирующие

- План 51.0 — Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox.
- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 65.0 — Team Coordination Policies: pluggable routing for multi-agent collaboration.
- План 72.0 — Core Topic/Subscription Event Bus: typed pub/sub routing for agent runtime.
- План 92.0 — Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
MessageInterventionPolicy {
  id,
  version,
  hooks[],
  priority,
  scope,
  failure_mode,
  content_hash
}
```

Hook input:

```text
MessageInterventionContext {
  team_session_id,
  sender,
  recipients[],
  message_kind,
  contract_ref?,
  payload_metadata,
  sensitivity,
  phase,
  causation_id?,
  routing_snapshot_hash
}
```

Raw payload раскрывается interceptor-у только если его policy/security class это разрешает.

### Безопасность

- sender identity immutable/Core-derived;
- intervention не расширяет grants;
- security policies fail-closed;
- machine-significant verdict fields нельзя произвольно переписывать;
- sensitive payload redacted до recipient context;
- route/phase/termination проверяются до delivery;
- custom imported workflow/skill не регистрирует executable intervention code;
- policy snapshot pinned к TeamSession/run.

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

- [ ] Есть versioned MessageInterventionPolicy pipeline.
- [ ] Intervention выполняется до recipient context injection.
- [ ] Есть Route/Sensitivity/Phase/Duplicate guards.
- [ ] Projection patches ограничены typed operations.
- [ ] Security hooks fail-closed и имеют fixed ordering.
- [ ] Есть human escalation path.
- [ ] Intervention events audit-able без raw sensitive payload.

## Ограничения и non-goals

- произвольные user-written interception scripts;
- изменение team roster через interceptor;
- выдача approvals/grants;
- подмена sender identity;
- использование intervention вместо ArtifactStore access checks;
- скрытое переписывание semantic результата агента.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#90 Message Intervention Policies: typed interceptors для agent/team message routing](https://github.com/rkfsociety/EvoHime/issues/90)
