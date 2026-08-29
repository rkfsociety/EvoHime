# План 83.1 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows: Core-контракт, schema и storage

Статус: этап 1 для [плана 83.0](./83-0-reasoning-operator-library.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/63). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows» и сделать его реализуемым: первичный выход — «Есть versioned ReasoningOperatorDefinition/Registry».

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Кандидатные поверхности из обзора: `crates/evohime-core/src/reasoning_operator_library.rs`, а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`, Electron main/preload bridge, bounded renderer projection и focused tests. Имена файлов проверяются по live checkout на этапе реализации и не являются заранее утверждённым API. Пути, schema revision и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 83.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.

### Опциональные

- План 25.0 — зависимость из обзора.
- План 26.0 — зависимость из обзора.
- План 63.0 — зависимость из обзора.
- План 79.0 — зависимость из обзора.

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

## Критерии выхода

- [ ] Есть versioned ReasoningOperatorDefinition/Registry.
- [ ] Built-in Generate/Review/Revise/Rank/Ensemble доступны как typed primitives.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
