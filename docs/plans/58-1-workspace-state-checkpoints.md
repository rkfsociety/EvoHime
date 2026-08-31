# План 58.1 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history: Core-контракт, schema и storage

Статус: этап 1 для [плана 58.0](./58-0-workspace-state-checkpoints.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/38). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Workspace State Checkpoints: безопасный rollback файлов отдельно от task history» и сделать его реализуемым: первичный выход — «Workspace checkpoint отделён от TaskCheckpoint».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/workspace_state_checkpoints.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 58.0 — scope, requirements, non-goals и dependency map.
- Existing TaskCheckpoint, ArtifactStore and workspace sandbox/path contracts.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Plan Artifact and Revision-Safe Workspace Files integrations из overview.

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

- `workspace_state_checkpoints.rs` defines metadata/create/compare/preflight/
  restore contracts separate from TaskCheckpoint; `RestoreBoth` composes two
  explicit operations and never conflates their state.
- Snapshot bytes are content-addressed through ArtifactStore (or a documented
  extension of its quota/sensitivity contract); new store contains bounded
  manifest/ref metadata only and lives outside the workspace/user `.git`.
- Snapshot inclusion/exclusion policy must skip VCS metadata, dependencies,
  build caches and reparse escapes deterministically.
- Manifest is immutable and captures git head/dirty baseline, tracked state,
  before/after hashes, deletion/symlink metadata, sensitivity and source event;
  backend choice (`ArtifactStore`/shadow/hybrid) does not change public API.
- Retention is quota/age/LRU based only for unpinned checkpoints; pinned or
  user-named checkpoints require explicit policy/action and corruption is
  detected before compare/restore.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/workspace_state_checkpoints_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Workspace checkpoint отделён от TaskCheckpoint. → зафиксировать typed invariant, error code и deterministic fixture.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Workspace State Checkpoints: безопасный rollback файлов отдельно от task history».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Workspace checkpoint отделён от TaskCheckpoint.
- [ ] Snapshot backend не загрязняет пользовательский Git history.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
