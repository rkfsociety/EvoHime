# План 71.0 — Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows

Статус: предложено по [issue #51](https://github.com/rkfsociety/EvoHime/issues/51). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный **Workflow Optimization Lab**: offline/dev-only контур, который автоматически генерирует, мутирует, оценивает и сравнивает варианты agent/workflow strategy на benchmark suites, не изменяя production workflow напрямую.

Это не runtime auto-tuning и не Continual Refinement в live session. Optimizer работает как экспериментальная лаборатория поверх versioned workflow/agent profiles и Benchmark Matrix.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/workflow_optimization_lab.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./71-1-workflow-optimization-lab.md)
- [Этап 2 — runtime-интеграция и recovery](./71-2-workflow-optimization-lab.md)
- [Этап 3 — IPC, client projection и UI](./71-3-workflow-optimization-lab.md)
- [Этап 4 — verification, release-evidence и закрытие](./71-4-workflow-optimization-lab.md)

## Зависимости

### Блокирующие

- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- Tool Simulation Runtime v1 из `../architecture.md`.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Composable Termination Conditions v1 — реализованный Core-контракт из канонических документов.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Base Strategy
 -> Candidate Generator
 -> Safe Structural Mutations
 -> Benchmark Matrix
 -> Score
 -> Search/Selection
 -> Candidate Archive
 -> Human Review
 -> optional promotion
```

Optimizer **никогда не активирует candidate в production автоматически**.

### Безопасность

- optimizer запускается только explicit offline/dev action;
- candidates не получают больше capabilities base profile;
- security middleware/approvals immutable constraints;
- synthetic benchmark credentials only;
- no production side effects by default;
- candidate promotion explicit;
- benchmark evaluator frozen per run;
- optimizer model не имеет tools.

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

- [ ] Есть versioned OptimizationRun/Candidate contracts.
- [ ] Search space declarative и Core-validated.
- [ ] Candidates оцениваются через Benchmark Matrix.
- [ ] Есть multi-metric objective/constraints.
- [ ] Есть train/validation/holdout semantics.
- [ ] Security regressions являются hard rejection.
- [ ] Search bounded по rounds/cost/tokens/time.
- [ ] Promotion в production только explicit и versioned.

## Ограничения и non-goals

- live self-modifying production agent;
- генерация произвольного executable code для runtime;
- автоматическая публикация winner;
- изменение security policy ради лучшего benchmark score;
- публичный leaderboard;
- гарантия глобально оптимального workflow.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#51 Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows](https://github.com/rkfsociety/EvoHime/issues/51)
