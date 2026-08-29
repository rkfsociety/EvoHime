# План 27.1 — Retained Child Contexts и mailbox: Core-контракт, schema и storage

Статус: самостоятельный этап 1 для [плана 27.0](./27-0-retained-child-contexts.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/8). Этап не считается реализованным по одному тексту.

## Цель и граница

Реализовать authoritative Rust contract, validators, canonical hash, error
codes и durable storage для registry/mailbox. Runtime dispatch, IPC и UI не
входят. Core остаётся единственным владельцем state и authority.

## Зависимости

### Блокирующие

- утверждённый scope/contract из [27.0](./27-0-retained-child-contexts.md);
- `child_contracts.rs`, `child_runtime.rs`, `child_workflow.rs`, `child_store.rs`,
  `ArtifactStore` policy и live `LocalDatabase` schema/migration ladder;
- authenticated actor/provenance, idempotency, SQLite backup-before-migrate и
  существующие error/event conventions.

### Опциональные

- Goal/Continuation linkage; при отсутствии typed linkage запись остаётся
  parent-scoped, а не получает implicit success.

## Реализация по шагам

0. Зафиксировать evidence текущего checkout: `SCHEMA_VERSION = 36`, фактические
   migration installers, свободные proto tags и отсутствие retained/mailbox
   contract. Проверить, что plan 23 и 26 используются только через canonical
   docs, а не как несуществующие файлы. Подтвердить proposed next revision и
   exact module owners до изменения schema.
1. Добавить `crates/evohime-core/src/retained_child.rs` с `RetainedChildV1`,
   `ChildFollowUpRequestV1`, `MailboxEntryV1`, enums lifecycle/delivery/mode и
   transition table. Ограничить строки/refs/payload defaults из 27.0; сделать
   `(parent_id, child_id)` и `registry_version` частью optimistic concurrency.
2. Реализовать canonical normalized JSON/hash, serde representation и typed
   errors (`unsupported_version`, `invalid_scope`, `stale_revision`,
   `invalidated_context`, `limit_exceeded`, `duplicate`, `unknown_delivery`).
   Unknown authority fields, malformed actor identity и oversized input fail
   closed.
3. Реализовать `crates/evohime-local-storage/src/retained_child_store.rs`
   либо документировать обоснованное объединение с `child_store.rs`. Additive
   transactional migration должна создать registry, follow-up/mailbox и
   dedup/sequence indexes без изменения старых child rows; backup выполняется
   до blocking migration. Уникальность parent scope и atomic sequence обязаны
   быть enforced SQLite, не только Rust.
4. Зафиксировать sensitivity/provenance/context matrix: inline только bounded
   non-secret metadata; artifact ref проходит существующую allowlist/read
   policy; retain/delete invalidates derived refs; no raw transcript storage.
5. Добавить focused fixtures на valid/invalid contract, canonical hash,
   duplicate/idempotency, parent isolation, stale version, limits, redaction,
   migration rollback/corruption и fresh-schema table/index assertions.

## Артефакты и критерии выхода

- versioned Core types, transition table, validators и stable typed errors;
- additive storage schema с exact revision, migration/backup/rollback evidence;
- atomic parent sequence и durable idempotency keys;
- provenance/sensitivity matrix и negative fixtures;
- evidence record с commit, exact paths, schema revision и commands.

Критерии этого этапа: contract round-trip/hash стабилен; чужой parent, stale
version, unknown field и overflow отвергаются; migration сохраняет старые child
rows и rollback доказан; mailbox ещё не dispatch-ится; все focused storage/Core
tests проходят. End-to-end delivery criteria остаются у этапов 27.2–27.4.

## Не входит

Runtime scheduling/dispatch, provider effect, IPC/UI, auto activation и
необратимые внешние side effects.
