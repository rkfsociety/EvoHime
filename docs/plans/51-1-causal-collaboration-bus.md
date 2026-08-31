# План 51.1 — Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox: Core-контракт, schema и storage

Статус: этап 1 для [плана 51.0](./51-0-causal-collaboration-bus.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/31). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox» и сделать его реализуемым: первичный выход — «Есть typed CollaborationMessage envelope».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/causal_collaboration_bus.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 51.0 — scope, requirements, non-goals и dependency map.
- Retained Child mailbox/store, Agent Role Profiles и Team SOP Protocols
  roster/policy snapshots.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Artifact Handoff Registry для semantic deliverables; до него bounded inline
  payload и существующие ArtifactStore refs.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome.
   Зафиксировать полный envelope, machine-significant kind schemas,
   addressing (`DirectRoleInstance`, `RoleSlot`, `ProtocolGroup`, `Parent`,
   `TeamCoordinator`), subscription filters и delivery states
   `Accepted/Queued/Delivered/Consumed/Expired/Rejected`.
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

- `causal_collaboration_bus.rs` вводит routing/subscription/causality policy
  поверх `retained_child.rs`, `agent_role_profiles.rs` и
  `team_sop_protocols.rs`; sender/roster всегда Core-derived.
- Storage расширяет `retained_child_store.rs` либо использует общий message
  substrate с доказанной atomic sequence/dedup semantics; отдельный
  несовместимый mailbox store запрещён.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/causal_collaboration_bus_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C04` — Есть causation/correlation/sequence metadata. → зафиксировать typed invariant, error code и deterministic fixture.
- `C05` — Inbox bounded и имеет backpressure semantics. → задать bounded limits и typed overflow/limit errors.
- `C07` — Layer переиспользует/расширяет child mailbox, а не дублирует его без причины. → зафиксировать typed invariant, error code и deterministic fixture.
- Free-form prose допустим только как bounded human-readable field/`Notice`;
  routing и workflow transition не зависят от его парсинга.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть typed CollaborationMessage envelope.
- [ ] Sender identity и routing Core-owned.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
