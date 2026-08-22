# План 11 — Typed memory и Core-first RAG

## Цель

Свести существующие memory domain, memory store и workspace RAG в один
проверяемый lifecycle: запись, retrieval, compaction и forget. План не создаёт
вторую базу знаний, второй memory SDK и не вводит автоматическое запоминание
всего transcript.

## Что уже есть в checkout

- `crates/evohime-core/src/memory_domain.rs`: `MemoryScope` (project/task/
  workspace), `ProvenanceRef`, `PrivacyLabel`, `MemoryStatus`, TTL через
  `is_expired_at`, `archive`/`forget`. Домен намеренно не владеет persistence
  и не даёт embedding/vector API;
- `crates/evohime-core/src/memory_extraction.rs`: `MemoryKind` с
  `default_ttl_ms`, `is_session_only`, `always_requires_approval`,
  `MemoryScopeLevel`, `SourceTrust` с `can_ground_strict_save`,
  `PrivacyLevel`, `ConfirmationState` и обязательный `evidence_locator`;
- `crates/evohime-local-storage/src/memory_store.rs`: SQLite persistence,
  валидация записи, `transition_state`, `supersede` и `supersession_chain`,
  `expire_due`, `forget_with_tombstone`, aliases и session notes с
  `purge_expired_session_notes`;
- `crates/evohime-core/src/workspace_rag.rs` (~4.2k строк): bounded indexing,
  SQLite/FTS5 generation, `QueryStrategy`/`HybridConfig`, `ScoreExplanation`
  и `RankingExplanation`, `CitationStatus`, `SearchDiagnostics`,
  `ContextBuildResult` и `RagLedgerProjection`;
- `crates/evohime-core/src/context_budget.rs` и
  `crates/evohime-local-storage/src/context_ledger_store.rs`: context plan,
  dropped/compression projection, ledger append/usage/prune;
- SQLite schema v29 с transactional migration, backup и rollback.

План 11 закрывает разрывы между этими частями, а не переписывает их.

## Решения, зафиксированные ревью

1. Источник истины для typed record — существующие `memory_domain.rs` и
   `memory_store.rs`. Новый record type не вводится: недостающие поля
   добавляются аддитивно к текущей SQLite schema.
2. Термин «consent» в коде отсутствует. В плане 11 он не вводится как новая
   отдельная сущность: разрешение на запись и на выдачу выражается
   существующими `PrivacyLevel`, `SourceTrust`, `ConfirmationState` и
   `always_requires_approval` плюс policy/approval плана 09. Отдельный
   consent-каталог запрещён.
3. Retrieval остаётся в `workspace_rag.rs`. Memory retrieval переиспользует
   его ranking/citation типы, а не создаёт второй ranker.
4. Embeddings и hybrid retrieval — опциональный слой. Отсутствие embeddings
   всегда даёт deterministic FTS5 fallback, а не ошибку.
5. Compaction пишет derived summary как versioned projection со ссылками на
   исходные event ID. Удалять исходные execution/evidence events compaction
   не может; для receipts prefix compaction остаётся существующий
   `compact_chain` в `evohime-receipts`.

## Границы

Входит: аддитивные поля typed record (confidence, evidence links, execution
event references), scope/privacy/approval gates до записи и до выдачи,
deterministic retrieval с score breakdown, optional embeddings, context
budget и compaction, expiry/deletion/forget и bounded projection в UI.

Не входит: автоматическое запоминание всего transcript, thought без evidence
как факт, внешняя knowledge base, второй ranker или memory SDK, UI как
источник истины и удаление исходных events через compaction.

## Зависимости

### Блокирующие

- планы 08–10 после их принятия: execution events, policy/approval,
  capability scope и authenticated projection;
- текущие `memory_domain.rs`, `memory_extraction.rs`, `memory_store.rs`,
  `workspace_rag.rs`, `context_budget.rs`, `context_ledger_store.rs` и
  SQLite schema v29.

### Опциональные

- local embeddings. Без них retrieval работает через deterministic FTS5, а
  hybrid score breakdown содержит только lexical компоненты;
- provider reflection. Без него compaction завершается deterministic
  `degraded`/`unknown`, сохраняет исходные items и не подтверждает новые
  факты;
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
