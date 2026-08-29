# План 39.1 — Structured Response Contract: schema-first ответы модели с provider/tool fallback: Core-контракт, schema и storage

Статус: этап 1 для [плана 39.0](./39-0-structured-response-contract.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/19). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Structured Response Contract: schema-first ответы модели с provider/tool fallback» и сделать его реализуемым: первичный выход — «Есть versioned ResponseContract».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/structured_response_contract.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 39.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 24.0 — зависимость из обзора.
- План 37.0 — зависимость из обзора.
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

- `crates/evohime-core/src/structured_response_contract.rs`: ввести `StructuredResponseContractDefinition`, `StructuredResponseContractPolicy`, typed state/event/error types и public validation entrypoint; зарегистрировать модуль в `crates/evohime-core/src/lib.rs`.
- Storage: состояние этапа остаётся ephemeral; новую durable таблицу и migration не добавлять. Добавить negative persistence test, а диагностический результат передавать через существующий event/release evidence.
- Proto/adapter: определить только versioned DTO, которые нужны stage 3; secrets, raw prompts и executable identities в contract не входят.
- Тесты: unit fixtures рядом с модулем и `crates/evohime-core/tests/structured_response_contract_contract.rs` для valid/invalid, bounds, redaction, duplicate/stale и migration/ephemeral решения.

### Acceptance-to-contract matrix

- `C01` — Есть versioned ResponseContract. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C04` — Все outputs проходят Core-side schema validation. → ввести versioned Rust-типы, enum-состояния и canonical serialization.
- `C05` — Ошибки typed и различают parse/validation/multiple/unsupported. → зафиксировать typed invariant, error code и deterministic fixture.
- `C06` — Repair retries bounded. → задать bounded limits и typed overflow/limit errors.

### Definition freeze

- До stage 2 зафиксировать schema revision, canonical hash, sensitivity/provenance matrix, typed error codes и exact persistence decision для «Structured Response Contract: schema-first ответы модели с provider/tool fallback».
- Evidence stage 1: `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` и сохранённые fixtures/SQL migration evidence.

## Критерии выхода

- [ ] Есть versioned ResponseContract.
- [ ] Gateway поддерживает provider-native и synthetic-tool strategies.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
