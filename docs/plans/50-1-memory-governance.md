# План 50.1 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy: Core-контракт, schema и storage

Статус: этап 1 для [плана 50.0](./50-0-memory-governance.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/30). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Memory Governance: typed memory, evidence gates, reinforcement и retention policy» и сделать его реализуемым: первичный выход — «MemoryRecord имеет typed kind/scope/durability/authority/confidence».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/memory_governance.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 50.0 — scope, requirements, non-goals и dependency map.
- Существующие `memory_domain`, `memory_api`, `memory_retrieval` и
  `memory_store` contracts; stage обязан выбрать одно authoritative
  `MemoryRecord` representation и migration path.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Continual Refinement и TaskCheckpoint consumers из overview.

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

- Расширить `memory_domain.rs`, `memory_api.rs`, `memory_extraction.rs`,
  `memory_retrieval.rs` и durable `memory_store.rs`; optional
  `memory_governance.rs` содержит gate/policy, но не второй record/store.
- Storage: additive fields/tables в существующем memory store с explicit
  migration для старых records, backup-before-migrate и rollback без
  частичной записи. Старые API не могут писать durable record в обход gate.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/memory_governance_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — MemoryRecord имеет typed kind/scope/durability/authority/confidence. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C03` — Есть dedup/merge и explicit contradiction semantics. → зафиксировать typed invariant, error code и deterministic fixture.
- `C04` — Reinforcement требует независимого evidence. → зафиксировать typed invariant, error code и deterministic fixture.
- `C05` — Есть freshness и versioned retention policy. → ввести versioned Rust-типы, enum-состояния и canonical serialization.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Memory Governance: typed memory, evidence gates, reinforcement и retention policy».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] MemoryRecord имеет typed kind/scope/durability/authority/confidence.
- [ ] Долговременные записи проходят MemoryWriteGate.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
