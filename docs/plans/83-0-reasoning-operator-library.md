# План 83.0 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows

Статус: предложено по [issue #63](https://github.com/rkfsociety/EvoHime/issues/63). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Reasoning Operator Library**: versioned набор переиспользуемых model-computation primitives с typed inputs/outputs, которые можно использовать в agent loop, workflow, Team SOP, planner и offline Workflow Optimization Lab.

Reasoning Operator — это не Tool и не Skill.

```text
Tool
  выполняет внешний/local effect

Skill
  описывает процедуру/знание о том, как решать класс задач

Reasoning Operator
  выполняет ограниченную модельную операцию над typed input
  и возвращает typed result без самостоятельных side effects
```

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/reasoning_operator_library.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./83-1-reasoning-operator-library.md)
- [Этап 2 — runtime-интеграция и recovery](./83-2-reasoning-operator-library.md)
- [Этап 3 — IPC, client projection и UI](./83-3-reasoning-operator-library.md)
- [Этап 4 — verification, release-evidence и закрытие](./83-4-reasoning-operator-library.md)

## Зависимости

### Блокирующие

- План 25.0 — Persistent Goals: durable цели для долгих задач.
- План 26.0 — Continuation Policy: bounded autonomous loops и quality gates.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Composable Termination Conditions v1 — реализованный Core-контракт из канонических документов.
- План 79.0 — Team Coordinator: capability-aware delegation, dynamic task routing и managerial validation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- reasoning operator по умолчанию не имеет tools/network/filesystem authority;
- operator не расширяет grants текущего run;
- prompt template/model output не определяет executable capability identity;
- output проходит schema validation;
- candidate refs Core-assigned;
- loops/fan-out bounded;
- imported workflow может ссылаться только на уже зарегистрированный совместимый operator;
- security/approval policy не может быть заменена Review operator-ом;
- model Verify не считается real-world evidence без соответствующего evidence class.

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

- [ ] Есть versioned ReasoningOperatorDefinition/Registry.
- [ ] Built-in Generate/Review/Revise/Rank/Ensemble доступны как typed primitives.
- [ ] Machine outputs используют Structured Response Contract.
- [ ] Review/Revise и Ensemble имеют bounded iteration/parallelism budgets.
- [ ] Operator invocation имеет provenance/usage metrics.
- [ ] Operators не получают tool capabilities автоматически.
- [ ] Workflow/Role/Optimization Lab могут ссылаться на stable operator identities.
- [ ] Built-in operators имеют отдельные eval fixtures.

## Ограничения и non-goals

- новый agent runtime;
- произвольный executable operator code из проекта/интернета;
- превращение всех prompts в registry entities;
- model-based замена deterministic tests/validation;
- unbounded debate/ensemble loops;
- выдача operators собственных credentials/tools;
- автоматическая активация optimizer-generated operator implementations.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#63 Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows](https://github.com/rkfsociety/EvoHime/issues/63)
