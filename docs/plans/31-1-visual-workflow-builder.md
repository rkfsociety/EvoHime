# План 31.1 — Visual Workflow Builder: typed canvas, validation и live runtime inspection: Core-контракт, schema и storage

Статус: этап 1 для [плана 31.0](./31-0-visual-workflow-builder.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/11). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать typed contract для canvas draft поверх существующего `workflow/v1` и сделать его реализуемым: первичный выход — Core-owned contract, на котором этап UI сможет построить canvas.

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/visual_workflow_builder.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 31.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 30.0 — зависимость из обзора.

## Реализация

0. Сверить overview с live code/docs/tests/git log; если контракт уже существует, собрать evidence для закрытия, не создавая второй authority.
1. Описать versioned fields, enums, draft transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility. Для mutation определить optimistic version и stale outcome; published workflow и running snapshot остаются immutable.
2. Реализовать Rust validators и canonical serde/JSON/Proto representation; unknown version, oversized input и authority-bearing unknown data дают typed error.
3. Добавить durable store и additive migration с backup-before-migrate для draft/recovery state; layout и draft переживают restart, а published/running graph не переписываются.
4. Добавить deterministic fixtures: valid/invalid, duplicate, stale, redaction, limit и migration failure; выдать evidence-пакет этапу 2.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Предметная декомпозиция

### Поверхности и контракт

- `crates/evohime-core/src/visual_workflow_builder.rs`: ввести `VisualWorkflowBuilderDefinition`, `VisualWorkflowBuilderPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: расширить существующий `LocalDatabase`/workflow storage либо добавить отдельный store только после evidence freeze; draft и layout могут переживать restart, published definition хранится как immutable version. Не дублировать runtime snapshot из `workflow_store`.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/visual_workflow_builder_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale, layout-vs-execution hash и draft persistence/migration решения.

### Acceptance-to-contract matrix

- `C02` — Pins и block metadata приходят из Core registry. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C03` — Core выполняет authoritative validation. → зафиксировать typed invariant, error code и deterministic fixture.
- `C04` — Сохранение создаёт immutable новую version. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C05` — Layout metadata отделена от execution hash. → хранить layout отдельно и доказать, что перемещение узлов не меняет execution hash.

### Definition freeze

- До stage 2 зафиксировать schema revision, execution/layout hash rules, sensitivity/provenance matrix, typed error codes и exact persistence decision для draft/recovery.
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть Core-owned typed contract для canvas над существующим workflow contract.
- [ ] Pins и block metadata приходят из Core registry.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Draft/recovery persistence decision и rollback доказаны тестом; running workflow не изменяется.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
