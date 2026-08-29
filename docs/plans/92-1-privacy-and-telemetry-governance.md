# План 92.1 — Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries: Core-контракт, schema и storage

Статус: этап 1 для [плана 92.0](./92-0-privacy-and-telemetry-governance.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/72). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries» и сделать его реализуемым: первичный выход — «Есть explicit per-category consent state».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/privacy_and_telemetry_governance.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 92.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 40.0 — зависимость из обзора.
- План 37.0 — зависимость из обзора.
- План 51.0 — зависимость из обзора.
- План 69.0 — зависимость из обзора.
- План 81.0 — зависимость из обзора.

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

- `crates/evohime-core/src/privacy_and_telemetry_governance.rs`: ввести `PrivacyAndTelemetryGovernanceDefinition`, `PrivacyAndTelemetryGovernancePolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/privacy_and_telemetry_governance_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/privacy_and_telemetry_governance_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть explicit per-category consent state. → зафиксировать typed invariant, error code и deterministic fixture.
- `C02` — Есть централизованный typed telemetry dictionary/API. → зафиксировать typed invariant, error code и deterministic fixture.
- `C03` — Properties allowlisted, unknown fields fail closed. → зафиксировать typed invariant, error code и deterministic fixture.
- `C04` — Sensitive content deny-by-default и проходит pre-send guard. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C07` — Пользователь может revoke/reset/clear telemetry state. → зафиксировать typed invariant, error code и deterministic fixture.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть explicit per-category consent state.
- [ ] Есть централизованный typed telemetry dictionary/API.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
