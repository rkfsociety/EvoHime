# План 95.1 — Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing: Core-контракт, schema и storage

Статус: этап 1 для [плана 95.0](./95-0-team-coordination-strategies.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/75). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing» и сделать его реализуемым: первичный выход — «Есть versioned TeamCoordinationStrategy».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/team_coordination_strategies.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 95.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 65.0 — зависимость из обзора.
- План 72.0 — зависимость из обзора.
- План 79.0 — зависимость из обзора.
- План 83.0 — зависимость из обзора.

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

- `crates/evohime-core/src/team_coordination_strategies.rs`: ввести `TeamCoordinationStrategiesDefinition`, `TeamCoordinationStrategiesPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/team_coordination_strategies_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/team_coordination_strategies_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть versioned TeamCoordinationStrategy. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C02` — Поддерживаются RoundRobin и минимум одна dynamic strategy. → зафиксировать typed invariant, error code и deterministic fixture.
- `C03` — Eligible set вычисляется Core до selection. → зафиксировать typed invariant, error code и deterministic fixture.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned TeamCoordinationStrategy.
- [ ] Поддерживаются RoundRobin и минимум одна dynamic strategy.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
