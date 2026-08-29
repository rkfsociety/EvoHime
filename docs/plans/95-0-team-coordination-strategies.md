# План 95.0 — Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing

Статус: предложено по [issue #75](https://github.com/rkfsociety/EvoHime/issues/75). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime versioned **Team Coordination Strategy**: Core-owned слой, который определяет, **кто из участников team session получает следующий ход/задачу**, не смешивая это с ролью агента, message bus или самим workflow runtime.

Нужны несколько формализованных стратегий координации, которые можно выбирать для разных типов команд:

```text
RoundRobin
RuleSelector
ModelSelector
HandoffSwarm
GraphDirected
```

Стратегия не создаёт новых агентов, не расширяет grants и не заменяет Team SOP. Она только выбирает следующего допустимого участника из уже зарегистрированного roster.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/team-coordination-strategies.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 65.0 — Team Coordination Policies: pluggable routing for multi-agent collaboration.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 72.0 — Core Topic/Subscription Event Bus: typed pub/sub routing for agent runtime.
- План 79.0 — Team Coordinator: capability-aware delegation, dynamic task routing и managerial validation.
- План 83.0 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
TeamSession
  -> eligible participants
  -> CoordinationStrategy
  -> selected participant
  -> Core validation
  -> dispatch via existing team/child runtime
```

Strategy получает только участников, которых разрешает текущий TeamProtocol/session snapshot.

### Безопасность

- candidate set Core-owned;
- selection не расширяет grants;
- model selector не имеет tools;
- unknown participant id rejected;
- handoff route валидируется protocol snapshot;
- artifact access проверяется отдельно;
- human participant выбирается только если соответствующий role slot допускает human mode;
- selector failure не запускает случайного агента.

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

- [ ] Есть versioned TeamCoordinationStrategy.
- [ ] Поддерживаются RoundRobin и минимум одна dynamic strategy.
- [ ] Eligible set вычисляется Core до selection.
- [ ] ModelSelector возвращает только typed participant identity.
- [ ] Есть handoff/graph routing с protocol validation.
- [ ] Есть repeat/no-progress safeguards и fallback.
- [ ] Strategy snapshot durable и pinned к TeamSession.

## Ограничения и non-goals

- свободный unbounded group chat;
- создание новых ролей во время selection;
- глобальная маршрутизация между проектами;
- выдача capabilities через handoff;
- arbitrary executable selector scripts;
- использование model rationale как security decision.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#75 Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing](https://github.com/rkfsociety/EvoHime/issues/75)
