# План 107.1 — Guided Calibration Sessions: iterative human feedback и versioned role guidance: Core-контракт, schema и storage

Статус: этап 1 для [плана 107.0](./107-0-guided-calibration-sessions.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/87). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Guided Calibration Sessions: iterative human feedback и versioned role guidance» и сделать его реализуемым: первичный выход — «Есть durable CalibrationSession/Iteration contracts».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/guided_calibration_sessions.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 107.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Реализованный Resumable Conversation Event Log v1 из `docs/architecture.md`.
- План 46.0 — зависимость из обзора.
- План 56.0 — зависимость из обзора.
- План 88.0 — зависимость из обзора.

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

- `crates/evohime-core/src/guided_calibration_sessions.rs`: ввести `GuidedCalibrationSessionsDefinition`, `GuidedCalibrationSessionsPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/guided_calibration_sessions_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/guided_calibration_sessions_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C06` — Candidates активируются только через Refinement pipeline. → зафиксировать typed invariant, error code и deterministic fixture.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Guided Calibration Sessions: iterative human feedback и versioned role guidance».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть durable CalibrationSession/Iteration contracts.
- [ ] Поддержан интерактивный baseline-feedback-revision loop.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
