# План 71.1 — Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows: Core-контракт, schema и storage

Статус: этап 1 для [плана 71.0](./71-0-workflow-optimization-lab.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/51). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows» и сделать его реализуемым: первичный выход — «Есть versioned OptimizationRun/Candidate contracts».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/workflow_optimization_lab.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 71.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 36.0 — зависимость из обзора.
- Tool Simulation Runtime v1 из `../architecture.md`.
- Composable Termination Conditions v1 — зависимость из канонических документов.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input и authority-bearing unknown data дают typed error.
3. Добавить durable store и additive migration с backup-before-migrate только если состояние переживает restart; ephemeral state закрепить отрицательным persistence test.
4. Добавить deterministic fixtures: valid/invalid, duplicate, stale, redaction, limit и migration failure; выдать evidence-пакет этапу 2.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Предметная декомпозиция

### Поверхности и контракт

- `crates/evohime-core/src/workflow_optimization_lab.rs`: ввести `WorkflowOptimizationLabDefinition`, `WorkflowOptimizationLabPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: состояние этапа остаётся ephemeral; новую durable таблицу и migration не добавлять. Добавить negative persistence test, а диагностический результат передавать через существующий event/release evidence.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/workflow_optimization_lab_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C03` — Candidates оцениваются через Benchmark Matrix. → зафиксировать typed invariant, error code и deterministic fixture.
- `C04` — Есть multi-metric objective/constraints. → зафиксировать typed invariant, error code и deterministic fixture.
- `C05` — Есть train/validation/holdout semantics. → зафиксировать typed invariant, error code и deterministic fixture.
- `C06` — Security regressions являются hard rejection. → задать Core-owned authority/sensitivity policy и fail-closed validation.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned OptimizationRun/Candidate contracts.
- [ ] Search space declarative и Core-validated.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
