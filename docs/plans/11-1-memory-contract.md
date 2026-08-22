# 11-1 — Typed memory lifecycle

## Цель

Довести существующий memory record до полного versioned контракта и явно
разделить scratch state, session context и durable memory.

## Что уже есть в checkout

- `MemoryKind` с `default_ttl_ms`, `is_session_only` и
  `always_requires_approval`; `MemoryScopeLevel` с `precedence`;
- `SourceTrust` (`can_ground_strict_save`, `requires_validation`),
  `PrivacyLevel` (`redacts_body_by_default`), `ConfirmationState`
  (`is_retrievable`) и обязательный `evidence_locator` в extraction;
- `MemoryStoreSql` с `validate`, `transition_state`, `supersede`,
  `expire_due`, `forget_with_tombstone`, session notes и
  `purge_expired_session_notes`;
- `scratchpad_store.rs` для scratch state и `MemoryDomain` как in-memory
  доменный слой без persistence.

Этап добавляет недостающие поля и gates, не заменяя эти типы.

## Зависимости

### Блокирующие

- 11-0 и текущие `memory_domain.rs`, `memory_extraction.rs`,
  `memory_store.rs`, SQLite schema v29;
- контракты 08-1/08-2 после их принятия для execution event ID;
- контракты 09-2/09-3 после их принятия для policy и approval gate.

### Опциональные

- redaction словари плана 12. До их появления используется существующая
  redaction memory store и `PrivacyLevel.redacts_body_by_default`;
- UI-подтверждение записи. До него `ConfirmationState` меняется только через
  Core command path, а renderer видит запись как `pending`.

## Контракт

1. Ввести `record_version` и аддитивные поля к текущей записи:

   - `confidence` — bounded числовой диапазон с фиксированной шкалой;
   - `evidence_refs` — список source/evidence ID (chunk, tool receipt,
     observation);
   - `execution_event_refs` — ID событий execution ledger;
   - `superseded_by`/`supersedes` уже есть и остаются источником истины для
     цепочки ревизий.

   Старый reader, не знающий новых полей, продолжает читать запись; миграция
   заполняет их deterministic значениями (`confidence = unknown`, пустые
   списки) без потери данных.

2. Зафиксировать три уровня хранения и запретить их смешение:

   - `scratch` — `scratchpad_store`, не индексируется и не выдаётся как
     факт;
   - `session/context` — session notes с TTL и `purge_expired_session_notes`;
     переход в durable возможен только явной командой с evidence;
   - `durable` — workspace/project/task memory в `memory_store`.

3. Проверять scope, `PrivacyLevel` и approval дважды: перед записью и перед
   выдачей записи в context. Проверка на выдаче не может быть заменена
   кэшем результата проверки на записи.

4. Model output или thought без validated evidence остаётся
   `ConfirmationState`, для которого `is_retrievable() == false`, и не
   получает `SourceTrust`, дающий `can_ground_strict_save`.

5. Любая mutation памяти порождает execution event с provenance и
   идемпотентным ключом; renderer получает только bounded metadata
   projection без body для `PrivacyLevel`, требующего redaction.

## Изменения по слоям

- SQLite: аддитивная миграция schema v29 → v30 с backfill и rollback;
- Rust core: расширение `MemoryRecord`, gates в `memory_api.rs`, linkage к
  ledger;
- IPC/proto: аддитивные поля projection без bump major;
- Electron: typed projection и отсутствие прямого доступа к SQLite.

## Проверки

- schema/serialization round-trip, migration fixtures v29 → v30 и rollback;
- scope/privacy gate и cross-workspace isolation;
- TTL/lifecycle transitions, session-note expiry, supersession chain;
- запись без evidence не становится retrievable;
- redaction secrets/PII до записи в SQLite;
- provenance linkage к event/action/observation;
- `cargo test --locked -p evohime-core -p evohime-local-storage`.

## Готово, когда

Ни одна запись не появляется без scope, privacy и provenance, session note не
превращается в durable memory молча, а durable memory невозможно создать
одним model-generated assertion.
