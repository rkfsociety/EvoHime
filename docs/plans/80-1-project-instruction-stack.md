# План 80.1 — Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence: Core-контракт, schema и storage

Статус: этап 1 для [плана 80.0](./80-0-project-instruction-stack.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/60). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence» и сделать его реализуемым: первичный выход — «Есть Core-owned ProjectRule registry/discovery».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/project_instruction_stack.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 80.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 60.0 — зависимость из обзора.
- План 64.0 — зависимость из обзора.

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

- `crates/evohime-core/src/project_instruction_stack.rs`: ввести `ProjectInstructionStackDefinition`, `ProjectInstructionStackPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: `crates/evohime-local-storage/src/project_instruction_stack_store.rs` и существующий `LocalDatabase` migration path; migration additive, backup-before-migrate, rollback без частичной записи, а для ephemeral state добавить negative persistence test.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/project_instruction_stack_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть Core-owned ProjectRule registry/discovery. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C03` — Есть path-based conditional activation. → зафиксировать typed invariant, error code и deterministic fixture.
- `C04` — Active instruction stack вычисляется детерминированно и hash-ится. → зафиксировать fingerprint, preconditions и provenance-поля.
- `C07` — Rules не расширяют capabilities и отделены от Skills/security policy. → задать Core-owned authority/sensitivity policy и fail-closed validation.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть Core-owned ProjectRule registry/discovery.
- [ ] Поддержаны global, workspace, nested и `AGENTS.md` sources.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
