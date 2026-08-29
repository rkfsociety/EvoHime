# План 73.0 — Dependency-Aware Task Graph: selective replanning и downstream invalidation

Статус: предложено по [issue #53](https://github.com/rkfsociety/EvoHime/issues/53). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный **Dependency-Aware Task Graph** для agent execution: план разбивается на typed tasks с явными зависимостями, assignee/role, status и evidence, а изменение или провал upstream-задачи инвалидирует только реально зависящий downstream scope.

Это дополняет Plan Artifact и TaskCheckpoint.

- **Plan Artifact** описывает согласованную стратегию.
- **Task Graph** является исполняемой декомпозицией текущего plan/run.
- **TaskCheckpoint** хранит фактическое состояние выполнения и continuity.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/dependency-aware-task-graph.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 25.0 — Persistent Goals: durable цели для долгих задач.
- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol.
- План 77.0 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation.
- План 89.0 — Checkpoint Forking & Replay: branch-and-compare запусков из сохранённого состояния.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- task graph не расширяет grants;
- assignee identity Core-resolved;
- completed status требует valid evidence policy;
- model patch не может менять completed history in-place;
- cycles/unknown refs rejected;
- replan cannot remove mandatory security/approval requirements;
- parallel scheduling respects workspace/write policies;
- user feedback не интерпретируется как executable identity.

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

- [ ] Есть versioned ExecutionTask/TaskDependency contracts.
- [ ] Core валидирует DAG и вычисляет Ready set.
- [ ] Поддерживается bounded parallel scheduling.
- [ ] Upstream revision/failure делает selective downstream invalidation.
- [ ] Partial replanning сохраняет unaffected completed work.
- [ ] Completion привязано к evidence/freshness.
- [ ] Task Graph связан с Plan Artifact и TaskCheckpoint, не дублируя их.
- [ ] Team SOP может использовать тот же task substrate.

## Ограничения и non-goals

- второй независимый workflow engine;
- автоматическое изменение accepted Plan при любом replan;
- last-writer-wins merge параллельных изменений;
- перенос completed status только по похожему тексту;
- бесконечный recursive retry;
- свободный arbitrary task code execution.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#53 Dependency-Aware Task Graph: selective replanning и downstream invalidation](https://github.com/rkfsociety/EvoHime/issues/53)
