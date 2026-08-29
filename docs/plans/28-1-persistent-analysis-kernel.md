# План 28.1 — Persistent Analysis Kernel: Core-контракт, schema и storage

Статус: этап 1 для [плана 28.0](./28-0-persistent-analysis-kernel.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/9). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать authoritative contract «Persistent Analysis Kernel», schema
границу, validators и storage policy. Worker/runtime остаются выходом этапа
28.2; этот этап обязан подготовить контракт, который 28.2 сможет исполнять.

## Граница

- Core владеет типами, состояниями, policy, provenance и записью. Model/user input только предлагает данные и не доказывает effect, approval, test или completion.
- Владелец contract: `crates/evohime-core/src/analysis_kernel.rs`;
  storage: `crates/evohime-local-storage/src/analysis_kernel_store.rs`;
  общая migration ladder — `crates/evohime-local-storage/src/lib.rs`.
  IPC types принадлежат этапу 28.3 и здесь не создаются. Пути, schema revision
  и IPC tags подтверждаются на evidence freeze.

## Зависимости

### Блокирующие

- План 28.0 — scope, requirements, non-goals и dependency map.
- Core capability/policy/approval, SQLite migration, event journal и authenticated IPC.
- текущая live schema и retained-child handoff плана 27; child-ref acceptance
  опирается на его канонический contract и не считается выполненным без
  проверки selected immutable refs.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Сверить overview с live code/docs/tests/git log; подтвердить фактическую
   следующую свободную schema revision после v37 и отсутствие kernel authority. Если часть
   контракта уже существует, собрать evidence, не создавая вторую authority.
1. Описать versioned fields, enums, transitions, parent/session scope,
   actor/provenance, idempotency, limits, sensitivity, ref kinds и compatibility.
   Для каждой mutation определить optimistic version, stale outcome и
   owner-check; отдельно запретить process memory/raw transcript storage.
2. Реализовать Rust validators и canonical serde/JSON representation в
   `analysis_kernel.rs`; typed IPC representation остаётся 28.3. Unknown
   version, oversized input, unknown authority-bearing data, invalid ref и
   secret/sensitive inline payload дают typed error.
3. Реализовать `analysis_kernel_store.rs` и additive migration с
   backup-before-migrate: durable manifest/object metadata, idempotency,
   sequence/event rows и invalidation markers; blob bytes идут только через
   существующий ArtifactStore. Ephemeral runtime получает отрицательный
   persistence test.
4. Добавить deterministic fixtures: valid/invalid, duplicate, stale,
   parent-isolation, redaction, limit, hash, migration rollback/corruption и
   unknown-outcome metadata; выдать evidence-пакет этапу 28.2.

## Артефакты

- contract/types + validator + transition table;
- canonical serialization/hash, error codes и provenance matrix;
- storage schema/store или доказательство отсутствия persistence;
- focused contract/security/migration tests.

## Критерии выхода

- [ ] Contract/schema/storage policy полностью определены, валидаторы и
  canonical hash проходят fixtures.
- [ ] Durable manifest/object metadata и ephemeral-vs-durable boundary доказаны;
  worker/runtime не выдаются как результат этого этапа.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Storage/ephemeral decision, parent isolation и rollback доказаны тестом.

## Rollback

Несовместимая запись отклоняется без частичной записи; failed migration возвращает backup и прежнюю schema revision. Внешние эффекты не запускаются.

## Не входит

Runtime orchestration, UI, external provider/backend и необратимые side effects.
