# План 59.1 — Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев: Core-контракт, schema и storage

Статус: этап 1 для [плана 59.0](./59-0-incremental-change-protocol.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/39). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев» и сделать его реализуемым: первичный выход — «Есть explicit RequirementDelta + RepositoryBaseline».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/incremental_change_protocol.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 59.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 57.0 — зависимость из обзора.
- План 58.0 — зависимость из обзора.
- План 38.0 — зависимость из обзора.

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

- `crates/evohime-core/src/incremental_change_protocol.rs`: ввести `IncrementalChangeProtocolDefinition`, `IncrementalChangeProtocolPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/incremental_change_protocol_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/incremental_change_protocol_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C02` — Есть ImpactAnalysis и versioned ChangePlan. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C03` — Реализация привязана к plan items и before/after fingerprints. → зафиксировать typed invariant, error code и deterministic fixture.
- `C05` — Scope drift видим и может gate-иться policy. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C06` — Review/tests используют exact implementation revision/fingerprint. → зафиксировать fingerprint, preconditions и provenance-поля.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть explicit RequirementDelta + RepositoryBaseline.
- [ ] Есть ImpactAnalysis и versioned ChangePlan.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
