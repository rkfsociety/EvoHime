# План 57.1 — Plan Artifact: versioned planning contract и явный переход Plan → Execute: Core-контракт, schema и storage

Статус: этап 1 для [плана 57.0](./57-0-plan-artifact.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/37). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Plan Artifact: versioned planning contract и явный переход Plan → Execute» и сделать его реализуемым: первичный выход — versioned `PlanArtifact`/`PlanStep` contract с типизированными acceptance criteria.

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/plan_artifact.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 57.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 23.0 — зависимость из обзора.
- План 40.0 — зависимость из обзора.

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

- `crates/evohime-core/src/plan_artifact.rs`: ввести `PlanArtifactDefinition`, `PlanArtifactPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/plan_artifact_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/plan_artifact_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C02` — Acceptance criteria типизированы, имеют `evidence_kind`, а их status → зафиксировать typed invariant, error code и deterministic fixture.
- `C03` — Переход `Plan -> Execute` выполняется только Core-командой до первого → зафиксировать typed invariant, error code и deterministic fixture.
- `C04` — Plan steps разрешают capabilities через Core registry и не несут raw → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C05` — Accepted plan revision/hash immutable; material deviation требует → зафиксировать fingerprint, preconditions и provenance-поля.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Plan Artifact: versioned planning contract и явный переход Plan → Execute».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned `PlanArtifact`/`PlanStep` contract с identity, revision,
  hash и provenance.
- [ ] Acceptance criteria типизированы, имеют `evidence_kind`, а их status
  выводится из Core-owned evidence.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
