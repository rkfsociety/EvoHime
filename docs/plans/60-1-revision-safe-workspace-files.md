# План 60.1 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection: Core-контракт, schema и storage

Статус: этап 1 для [плана 60.0](./60-0-revision-safe-workspace-files.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/40). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection» и сделать его реализуемым: первичный выход — «Есть typed namespaces uploads/workspace/outputs/scratch».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/revision_safe_workspace_files.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 60.0 — scope, requirements, non-goals и dependency map.
- Existing ArtifactStore, tool registry, sandbox/path canonicalization, event
  journal and Sensitive Data Guardrails contracts.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Incremental Change Protocol provenance adapter из overview.

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

- `revision_safe_workspace_files.rs` defines shared ref/read/mutation/batch
  service; existing filesystem read/write/patch/advanced tools become adapters
  to it. No registered mutating file tool may retain a legacy write path.
- Persistence is explicit per namespace: upload/output bytes and metadata use
  ArtifactStore; workspace refs are recalculated/invalidated at boundaries;
  scratch is run-scoped ephemeral; mutation intent/result and partial/unknown
  outcomes use the durable event/recovery journal. Add a table only if existing
  owners cannot represent durable namespace metadata.
- File identity uses canonical backend path internally but projects only
  logical namespace/path + hash/revision. Reparse-point identity is rechecked
  immediately before mutation.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/revision_safe_workspace_files_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C02` — File refs несут content hash/revision. → зафиксировать fingerprint, preconditions и provenance-поля.
- `C08` — Path traversal/symlink/reparse escape закрыты. → зафиксировать typed invariant, error code и deterministic fixture.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть typed namespaces uploads/workspace/outputs/scratch.
- [ ] File refs несут content hash/revision.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
