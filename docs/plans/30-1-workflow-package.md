# План 30.1 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей: Core-контракт, schema и storage

Статус: этап 1 для [плана 30.0](./30-0-workflow-package.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/10). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative package envelope над существующим `workflow/v1` и
сделать его реализуемым: первичный выход — «Есть versioned package format».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/workflow_package.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 30.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать envelope, dependency entries, credential slots, portable field
   metadata, provenance/fork lineage и import states (`parsed`, `validated`,
   `resolved`, `previewed`, `committed`, `rejected`). Зафиксировать bounds,
   compatibility, idempotency by content hash и stale outcome.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input и authority-bearing unknown data дают typed error.
3. Добавить только необходимое durable mapping/history через additive migration
   с backup-before-migrate; если package bytes остаются file-owned, закрепить
   это отрицательным persistence test. Не дублировать `workflow_runs`.
4. Добавить deterministic fixtures: round-trip, canonical export, stripped
   secrets/runtime ids, schema metadata redaction, dependency resolution,
   duplicate, mismatch, traversal/size limit и migration failure; выдать
   evidence-пакет этапу 2.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Предметная декомпозиция

### Поверхности и контракт

- `crates/evohime-core/src/workflow_package.rs`: ввести `WorkflowPackageDefinition`, `WorkflowPackagePolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Contract должен принимать только уже существующий `WorkflowGraph`; export
  строит portable projection, а import не десериализует произвольные поля в
  capability registry.
- Storage: `crates/evohime-local-storage/src/workflow_package_store.rs` и
  существующий `LocalDatabase` migration path; migration additive,
  backup-before-migrate, rollback без частичной записи. Package bytes могут
  оставаться file-owned, но импортированная workflow definition/version и
  content-hash mapping должны иметь однозначное durable ownership либо
  negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/workflow_package_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть versioned package format. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C02` — Export удаляет credentials/secrets/runtime-specific state. → ввести
  schema-driven sensitivity/portable metadata и fixture, доказыющий отсутствие
  token, secret, lease, run/checkpoint/session и machine path.
- `C03` — Есть dependency manifest. → зафиксировать typed dependency entry,
  resolution statuses и deterministic fixture.
- `C04` — Import выполняет validate/resolve/preview до записи. → зафиксировать
  фазовую state machine и fixture, где preview не меняет registry/database.
- `C05` — Credential slots требуют локального rebinding. → задать Core-owned authority/sensitivity policy и fail-closed validation.
- `C06` — Сохраняется безопасная provenance/fork lineage. → зафиксировать fingerprint, preconditions и provenance-поля.
- `C07` — Canonical hash позволяет duplicate/diff detection. → зафиксировать fingerprint, preconditions и provenance-поля.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Workflow Package: переносимый import/export без секретов и с rebinding зависимостей».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned package format.
- [ ] Export удаляет credentials/secrets/runtime-specific state.
- [ ] Import до explicit commit не имеет durable/external effect.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
