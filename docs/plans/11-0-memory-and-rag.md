# План 11 — Typed memory и Core-first RAG

## Цель

Свести существующие memory domain, memory store и workspace RAG в один
проверяемый lifecycle: запись, retrieval, compaction и forget. План не создаёт
вторую базу знаний, второй memory SDK и не вводит автоматическое запоминание
всего transcript.

## Что уже есть в checkout

- `crates/evohime-core/src/memory_domain.rs`: bounded in-memory
  `MemoryDomain` — `MemoryScope` (project/task/workspace), `ProvenanceRef`,
  `PrivacyLabel` (public/internal/private/secret), `MemoryStatus`, TTL через
  `is_expired_at`, `archive`/`forget`. Persistence и embedding/vector API
  домен намеренно не даёт;
- `crates/evohime-core/src/memory_api.rs`: `MemoryApi` поверх домена с
  `MemoryOperation`, `Approval`/`MemoryAuthorization`, `inspect_provenance` и
  `export`;
- `crates/evohime-core/src/memory_extraction.rs`: `MemoryKind` с
  `default_ttl_ms`, `is_session_only`, `always_requires_approval`,
  `MemoryScopeLevel`, `SourceTrust` с `can_ground_strict_save`,
  `PrivacyLevel` (normal/sensitive/secret), `ConfirmationState`,
  `ValidationStatus`, policy gate `evaluate` и `RawEvidenceLocator`;
- `crates/evohime-local-storage/src/memory_store.rs`: SQLite persistence —
  собственный `MemoryRecord` (`MemoryScope` с дополнительным `Session`,
  `MemoryPrivacy` public/internal/private и `MemoryExtractionFields`),
  `validate`, `transition_state`, `supersede`/`supersession_chain`,
  `expire_due`, `forget_with_tombstone`, aliases и session notes с
  `purge_expired_session_notes`;
- `crates/evohime-local-storage/src/scratchpad_store.rs`: durable task-scoped
  scratchpad (`task_scratchpad`) с `upsert`/`confirm`/`forget`,
  `offload_candidates` и запретом silent override подтверждённой записи;
- `crates/evohime-core/src/workspace_rag.rs` (~4.2k строк): bounded indexing,
  SQLite/FTS5 (`tokenize='trigram'`), `QueryStrategy`/`HybridConfig`,
  `ScoreExplanation` и `RankingExplanation`, `CitationStatus`
  (valid/updated/stale), `SearchDiagnostics`, `ContextBuildResult` и
  `RagLedgerProjection`; полный lifecycle vector index
  (`workspace_vector_indexes`, `workspace_chunk_vectors`) с deterministic
  локальным эмбеддером `embed_local` и режимами `hybrid`/`fts5`/
  `fallback_fts5`;
- `crates/context-budget` (крейт `evohime-context-budget`): `ContextPlanner`,
  `ContextItem`, `ladder`/`OffloadSink`, `BoundedSummarizer` с deterministic
  fallback, `ContextLedgerEntry` с `SelectedItemRecord`,
  `DroppedItemRecord`, `CompressionRecord` и `LedgerOutcome`;
- `crates/evohime-core/src/context_budget.rs` — интеграция этого крейта в
  agent loop (`ContextRuntime`, `assemble`/`replan`, `record_actual_usage`,
  `ModelContextProjection`, `deterministic_summarizer`, `model_summarizer`,
  `PrecomputedSummaryModel`) и
  `crates/evohime-local-storage/src/context_ledger_store.rs`
  (`append`, `record_usage`, `register_receipt`, `find_by_hash`,
  `projection`, `prune`);
- SQLite schema v30 с transactional migration и backup/restore
  (`backup.rs`, safety backup плюс `rollback_from_safety`).

План 11 закрывает разрывы между этими частями, а не переписывает их.

## Решения, зафиксированные ревью

1. Источник истины для persisted typed record — `memory_store::MemoryRecord`.
   `memory_domain::MemoryRecord` остаётся отдельным in-memory доменным типом
   с другим набором полей и другим privacy enum; план 11 не объединяет их и
   не переносит новые поля в домен. Новый record type не вводится:
   недостающие поля добавляются аддитивно к текущей SQLite schema.
2. Термин «consent» в коде отсутствует. В плане 11 он не вводится как новая
   отдельная сущность: разрешение на запись и на выдачу выражается
   существующими `PrivacyLevel`, `SourceTrust`, `ConfirmationState` и
   `always_requires_approval` плюс policy/approval плана 09. Отдельный
   consent-каталог запрещён.
3. Retrieval остаётся в `workspace_rag.rs`. Memory retrieval переиспользует
   его ranking/citation типы, а не создаёт второй ranker.
4. Embeddings уже есть как deterministic локальный слой (`embed_local`,
   `VECTOR_DIMENSION`, published vector index). Опционален не эмбеддер, а
   hybrid-ветка: при `HybridConfig.enabled == false`, недоступном или
   несовместимом индексе retrieval даёт `fallback_fts5` с причиной
   (`vector_index_unavailable`/`vector_index_incompatible`), а не ошибку.
   Внешний embedding provider в план 11 не входит.
5. Compaction пишет derived summary как versioned projection со ссылками на
   исходные event ID. Удалять исходные execution/evidence events compaction
   не может; для receipts prefix compaction остаётся существующий
   `compact_chain` в `evohime-receipts`.

## Границы

Входит: аддитивные поля typed record (record version, evidence links,
execution event references), scope/privacy/approval gates до записи и до
выдачи, deterministic retrieval со score breakdown, hybrid/FTS5 деградация,
context budget и compaction, expiry/deletion/forget и bounded projection в UI.

Не входит: автоматическое запоминание всего transcript, thought без evidence
как факт, внешняя knowledge base, внешний embedding provider, второй ranker
или memory SDK, объединение domain- и store-записи в один тип, UI как
источник истины и удаление исходных events через compaction.

## Зависимости

### Блокирующие

- планы 08–10 после их принятия: execution events, policy/approval,
  capability scope и authenticated projection;
- текущие `memory_domain.rs`, `memory_api.rs`, `memory_extraction.rs`,
  `memory_store.rs`, `scratchpad_store.rs`, `workspace_rag.rs`,
  крейт `evohime-context-budget`, `context_budget.rs`,
  `context_ledger_store.rs` и SQLite schema v30;
- механика миграции в `crates/evohime-local-storage/src/lib.rs`:
  `Self::migrate` вызывается только при `version < LEGACY_SCHEMA_VERSION`
  (26), а v27–v30 ставятся идемпотентными `install_schema`. Любая новая
  ветка `if current < 31` для существующей базы v26–v30 не выполнится — это
  учитывает 11-1. Новые memory-поля относятся к следующему приложенческому
  переходу v30 → v31 и должны устанавливаться идемпотентным `install_schema`
  на каждом открытии базы, а не добавляться только в старую migration ladder.

### Опциональные

- hybrid-ветка retrieval. При её отключении или несовместимом индексе
  breakdown содержит только lexical факторы, а режим помечается
  `fallback_fts5`;
- provider reflection. Без него compaction завершается deterministic
  fallback (`CompressionRecord.fallback = true`), сохраняет исходные items и
  не подтверждает новые факты;
- telemetry плана 12. Без него retrieval/compaction метрики остаются
  локальными счётчиками и не блокируют этапы.

## Этапы

- [11-1 — typed memory lifecycle](11-1-memory-contract.md)
- [11-2 — evidence и deterministic retrieval](11-2-evidence-retrieval.md)
- [11-3 — context budget, compaction и projections](11-3-context-compaction.md)
- [11-4 — forget, recovery и acceptance](11-4-memory-acceptance.md)

Порядок: 11-1 → 11-2 → 11-3 → 11-4.

## Готово, когда

Каждая memory record имеет scope, privacy, provenance и lifecycle, retrieval
объясним и воспроизводим, forget удаляет все derived data вместе с
tombstone, compaction сохраняет ссылки на исходные events, а UI меняет
только projection через Core command path.
