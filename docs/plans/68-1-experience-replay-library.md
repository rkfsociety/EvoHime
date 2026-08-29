# План 68.1 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection: Core-контракт, schema и storage

Статус: этап 1 для [плана 68.0](./68-0-experience-replay-library.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/48). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Experience Replay Library: episodic trajectories, success/failure retrieval и context injection» и сделать его реализуемым: первичный выход — «Есть versioned ExperienceRecord/Trajectory contracts».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/experience_replay_library.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 68.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 46.0 — зависимость из обзора.
- План 48.0 — зависимость из обзора.
- План 66.0 — зависимость из обзора.

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

- `crates/evohime-core/src/experience_replay_library.rs`: ввести `ExperienceReplayLibraryDefinition`, `ExperienceReplayLibraryPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/experience_replay_library_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/experience_replay_library_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть versioned ExperienceRecord/Trajectory contracts. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C02` — Success и failure experiences имеют evidence-backed scoring. → зафиксировать typed invariant, error code и deterministic fixture.
- `C05` — Context injection bounded и progressive. → задать bounded limits и typed overflow/limit errors.
- `C06` — Experience отделена от Memory и Refinement. → зафиксировать typed invariant, error code и deterministic fixture.
- `C08` — Повторяющиеся experiences могут служить evidence для refinement/evals. → зафиксировать typed invariant, error code и deterministic fixture.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Experience Replay Library: episodic trajectories, success/failure retrieval и context injection».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned ExperienceRecord/Trajectory contracts.
- [ ] Success и failure experiences имеют evidence-backed scoring.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
