# План 78.0 — Capability Workbenches: lifecycle-scoped tool groups with shared state and resources

Статус: предложено по [issue #58](https://github.com/rkfsociety/EvoHime/issues/58). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime абстракцию **Workbench**: lifecycle-scoped runtime component, который предоставляет динамический набор tools, управляющих общими ресурсами и состоянием.

Workbench занимает промежуточный уровень между отдельным tool и workflow/agent:

```text
Agent / Workflow
      ↓
   Workbench
      ↓
tools + shared session/resources/state
```

Это не Skill (#3): Skill описывает, **как работать**. Workbench предоставляет, **с чем и в какой живой runtime-сессии работать**.

Это не Integration Provider SDK (#13): provider описывает интеграцию/авторизацию/actions, а Workbench управляет конкретной runtime instance и lifecycle доступных инструментов.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/capability-workbenches.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 58.0 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history.
- План 61.0 — Task Worktree Isolation: отдельные Git worktrees для параллельных agent/child задач.
- План 89.0 — Checkpoint Forking & Replay: branch-and-compare запусков из сохранённого состояния.
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

- [ ] Workbench имеет versioned contract и lifecycle
- [ ] Tool list может быть dynamic
- [ ] Есть snapshot/restore
- [ ] Есть explicit ownership/scope
- [ ] Concurrency semantics заданы каждым workbench
- [ ] Capability проверяется при discovery и повторно при dispatch
- [ ] Credentials не входят в persisted state
- [ ] Есть resource lease/recovery model
- [ ] Cancellation/unknown outcome согласованы с durable runtime
- [ ] UI показывает lifecycle и shared resources

## Ограничения и non-goals

- Не заменять отдельные stateless tools workbench'ами без причины.
- Не превращать Workbench в ещё один workflow engine.
- Не считать shared process/session автоматическим shared authority.
- Не хранить OS handles и secrets как portable state.
- Не заставлять все integrations использовать Workbench, если provider actions stateless.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#58 Capability Workbenches: lifecycle-scoped tool groups with shared state and resources](https://github.com/rkfsociety/EvoHime/issues/58)
