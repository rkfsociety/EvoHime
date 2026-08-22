# 11-1 — Typed memory lifecycle

## Цель

Довести существующий persisted memory record до полного versioned контракта и
явно разделить scratch state, session context и durable memory.

## Что уже есть в checkout

- `MemoryKind` с `default_ttl_ms`, `is_session_only` и
  `always_requires_approval`; `MemoryScopeLevel` с `precedence`;
- `SourceTrust` (`can_ground_strict_save`, `requires_validation`),
  `PrivacyLevel` (normal/sensitive/secret, `redacts_body_by_default`),
  `ConfirmationState` (`is_retrievable`, `is_terminal`), `ValidationStatus`
  (`allows_retrieval`) и `RawEvidenceLocator` с `to_provenance_json`;
- `memory_store::MemoryRecord` с `MemoryExtractionFields`: `kind`,
  `canonical_subject`, `confirmation_state`, `model_confidence` и
  `verification_confidence` (оба валидируются в `0.0..=1.0`),
  `privacy_class`, `source_trust`, `supersedes`/`superseded_by`/
  `supersession_reason`, `extractor_version`, `policy_version`,
  `validation_status`, `validated_at`, `provenance_source_id`;
- `MemoryStoreSql` с `validate`, `transition_state`,
  `revise_pending_statement`, `supersede`, `supersession_chain`,
  `expire_due`, `forget_with_tombstone`, aliases, session notes и
  `purge_expired_session_notes`;
- `scratchpad_store.rs` (`task_scratchpad`) для scratch state с `confirm`,
  `forget`, `recover` и `offload_candidates`;
- `memory_api.rs` с `MemoryOperation`, `Approval`/`MemoryAuthorization` —
  но поверх in-memory `MemoryDomain`, не поверх SQLite store.

Этап добавляет недостающие поля и gates, не заменяя эти типы и не сливая
`memory_domain::MemoryRecord` с `memory_store::MemoryRecord`.

## Зависимости

### Блокирующие

- 11-0 и текущие `memory_domain.rs`, `memory_extraction.rs`,
  `memory_store.rs`, `scratchpad_store.rs`, SQLite schema v29;
- контракты 08-1/08-2 после их принятия: идентичность события — глобальный
  durable `sequence_id` из `events`;
- контракты 09-2/09-3 после их принятия для policy и approval gate.

### Опциональные

- redaction словари плана 12. До их появления используется существующая
  `redact_sensitive` в memory store и `PrivacyLevel.redacts_body_by_default`;
- UI-подтверждение записи. До него `ConfirmationState` меняется только через
  Core command path, а renderer видит запись как `pending_confirmation`.

## Контракт

1. Ввести `record_version` — единственное новое версионное поле записи.
   Существующие `extractor_version` и `policy_version` остаются как есть и
   описывают версию извлечения и политики, а не формата строки.

   Аддитивные поля к `MemoryExtractionFields`:

   - `evidence_refs` — список source/evidence ID (chunk, tool receipt,
     observation), нормализованный из `RawEvidenceLocator`;
   - `execution_event_refs` — список `sequence_id` событий execution ledger
     в терминах 08-1.

   Новое поле `confidence` не вводится: шкала уверенности уже задана парой
   `model_confidence`/`verification_confidence`, и `validate` отвергает
   значения вне `0.0..=1.0`, поэтому sentinel-значение `unknown` в этих полях
   недопустимо. Отсутствие проверки выражается существующим
   `validation_status`, а не третьим числовым полем.

   `superseded_by`/`supersedes` уже есть в `MemoryExtractionFields` и
   остаются источником истины для цепочки ревизий; в доменную запись они не
   переносятся.

   Старый reader, не знающий новых полей, продолжает читать запись; миграция
   заполняет их deterministic значениями (`record_version` = версия схемы
   записи на момент миграции, пустые списки ссылок) без потери данных.

2. Зафиксировать уровни хранения и запретить их смешение:

   - `scratch` — `task_scratchpad`; durable на диске, но не память: не
     индексируется и не выдаётся как факт. `ScratchpadStore::confirm`
     переводит запись в `ScratchpadStatus::Confirmed` внутри scratchpad и
     сам по себе не создаёт memory record;
   - `session/context` — два разных носителя, которые нельзя путать:
     `memory_session_notes` с TTL и `purge_expired_session_notes`, и записи
     `memory_entries` со `MemoryScope::Session`. Оба не участвуют в
     long-term retrieval; переход в durable возможен только явной командой с
     evidence, порождающей новую запись, а не сменой `scope_kind` на месте;
   - `durable` — `MemoryScope::Project`/`Task`/`Workspace` в `memory_store`.

3. Проверять scope, privacy и approval дважды: перед записью и перед выдачей
   записи в context. Проверка на выдаче не может быть заменена кэшем
   результата проверки на записи. Privacy проверяется в терминах persisted
   полей: `privacy` (`MemoryPrivacy`) для видимости записи и `privacy_class`
   (`PrivacyLevel`) для redaction; `privacy_class == "secret"` остаётся
   непреодолимым — `validate` возвращает `SecretNotStorable` до persistence.

4. Model output или thought без validated evidence остаётся в
   `ConfirmationState`, для которого `is_retrievable() == false`, и не
   получает `SourceTrust`, дающий `can_ground_strict_save`.

5. Любая mutation памяти порождает execution event с provenance и
   идемпотентным ключом; renderer получает только bounded metadata
   projection без body, если `redacts_body_by_default()` истинно.

6. Gates 3–5 применяются на пути к SQLite. Существующий `MemoryApi` работает
   поверх in-memory `MemoryDomain`, поэтому этап либо переводит его на
   `MemoryStoreSql`, либо добавляет отдельный store-путь с теми же
   `MemoryOperation`/`MemoryAuthorization`; два расходящихся набора gates
   запрещены.

## Изменения по слоям

- SQLite: аддитивная миграция v29 → v30 набором
  `ALTER TABLE memory_entries ADD COLUMN ... DEFAULT ...` по образцу ветки
  `current < 15`. Ветка `if current < 30` в `Self::migrate` недостаточна:
  `migrate` вызывается только при `version < LEGACY_SCHEMA_VERSION` (26), а
  базы v26–v29 её не увидят. Этап поднимает `SCHEMA_VERSION` до 30 и
  одновременно чинит условие вызова миграции (или переносит новые колонки в
  идемпотентный `install_schema`-путь), иначе backfill молча не применится;
- rollback здесь — не down-migration: `read_schema_version` отвергает
  `version > SCHEMA_VERSION` как `UnsupportedSchema`. Откат выполняется
  восстановлением pre-migration копии (`.db.bak` или `backup.rs` с
  `rollback_from_safety`), и это должно быть проверено тестом, а не
  подразумеваться;
- Rust core: расширение `memory_store::MemoryRecord`, gates на пути записи,
  linkage к ledger;
- IPC/proto: аддитивные поля projection без bump major;
- Electron: typed projection и отсутствие прямого доступа к SQLite.

## Проверки

- schema/serialization round-trip, migration fixtures v29 → v30 для базы,
  созданной как v26, v28 и v29 (каждая должна получить новые колонки);
- восстановление из pre-migration backup после неудачной миграции;
- scope/privacy gate и cross-workspace isolation;
- TTL/lifecycle transitions, session-note expiry, `MemoryScope::Session` не
  попадает в long-term retrieval, supersession chain;
- запись без evidence не становится retrievable;
- `privacy_class == "secret"` отвергается до записи, redaction secrets/PII
  применяется до SQLite;
- provenance linkage к event/action/observation через `sequence_id`;
- `cargo test --locked -p evohime-core -p evohime-local-storage`.

## Готово, когда

Ни одна запись не появляется без scope, privacy и provenance, session note не
превращается в durable memory молча, а durable memory невозможно создать
одним model-generated assertion.
