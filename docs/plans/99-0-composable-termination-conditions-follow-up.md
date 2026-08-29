# План 99.0 — Composable Termination Conditions: typed stop algebra для agent/team runs

Статус: предложено по [issue #79](https://github.com/rkfsociety/EvoHime/issues/79). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный versioned слой **Composable Termination Conditions**: набор типизированных условий завершения agent/team run, которые можно комбинировать через `AND/OR` и вычислять по authoritative runtime events/evidence.

Это не замена Continuation Policy. Continuation Policy отвечает, продолжать ли автономную работу. Termination Conditions отвечают, **при каком формальном событии конкретный agent/team session должен считаться завершённым или остановленным**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/composable-termination-conditions-follow-up.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 63.0 — Composable Termination Conditions: first-class stop policies for agent and team runs.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 83.0 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows.
- План 95.0 — Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- termination conditions read-only;
- condition не выдаёт capabilities;
- model не может менять active condition snapshot;
- external stop sender identity Core-derived;
- artifact/evidence refs validated;
- raw free-text parsing не используется для security-critical stop;
- expression bounded по depth/size.

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

- [ ] Есть versioned TerminationCondition и expression AST.
- [ ] Поддерживаются typed stop/event/evidence conditions.
- [ ] Есть AND/OR composition.
- [ ] Stateful conditions durable/recoverable.
- [ ] Team coordinator проверяет termination до routing.
- [ ] Termination отделена от retry/continuation semantics.
- [ ] UI показывает reason/counters.

## Ограничения и non-goals

- arbitrary user scripts как predicates;
- parsing hidden chain-of-thought;
- замена Goal success criteria;
- автоматический retry внутри condition;
- standing approval через termination event.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#79 Composable Termination Conditions: typed stop algebra для agent/team runs](https://github.com/rkfsociety/EvoHime/issues/79)
