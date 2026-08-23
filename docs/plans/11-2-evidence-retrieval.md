# 11-2 — Evidence и deterministic retrieval

## Цель

Сделать memory/RAG retrieval bounded, объяснимым и воспроизводимым поверх
существующего `workspace_rag.rs`, без второго ranker.

## Что уже есть в checkout

- `QueryPlan`, `QueryFilters`, `RetrievalLimits`, `QueryStrategy` и
  `HybridConfig`;
- `ScoreExplanation` (`algorithm`, `column_weights`, `term_frequencies`,
  `document_length`, `matched_filters`, `excluded_by`) — фактический score
  breakdown, и `RankingExplanation` (`algorithm`, `lexical_rank`,
  `vector_rank`, `rrf_rank`, `sources`) — позиции при RRF-слиянии;
- `CitationStatus` (`valid`/`updated`/`stale`), `Citation` с `chunk_hash` и
  `reason`, `SearchDiagnostics`, `RetrievalProgress` и `ContextBuildResult`;
- SQLite/FTS5 generation и bounded indexing с progress;
- vector index lifecycle (`building`/`ready`/`published`/`deprecated`/
  `failed`/`cancelled`) и режимы `hybrid`/`fts5`/`fallback_fts5` с причинами
  `vector_index_unavailable` и `vector_index_incompatible`.

Этап связывает эти структуры с memory records и закрывает freshness- и
фильтрационные разрывы.

## Зависимости

### Блокирующие

- 11-1: typed record с `evidence_refs` и `execution_event_refs`;
- текущие `workspace_rag.rs` и его generation/citation типы.

### Опциональные

- hybrid-ветка. При `HybridConfig.enabled == false` или несовместимом индексе
  `QueryStrategy` деградирует к FTS5-only, breakdown содержит только lexical
  факторы, а результат помечается `fallback_fts5` с причиной, не ошибкой;
- telemetry плана 12 для агрегатов retrieval quality.

## Контракт

1. Связать evidence record с document chunk, tool receipt, observation и
   execution event ID; отсутствующий или неизвестный ID — typed ошибка, а не
   молча пустая ссылка. Валидация provenance от Memory Extraction уже есть в
   `workspace_rag.rs` и переиспользуется, а не дублируется.
2. Ranking — multi-factor с фиксированным tie-break: стабильный порядок по
   score, затем по freshness, затем по ID. Фактора freshness в текущем
   слиянии нет — `RankingExplanation` хранит только `lexical_rank`,
   `vector_rank` и `rrf_rank`, поэтому этап аддитивно добавляет в неё
   freshness-компонент и итоговый tie-break ключ. Score breakdown остаётся в
   `ScoreExplanation` и обязателен для каждого выданного item.
3. Фильтры scope и privacy применяются до ranking, а не после усечения по
   лимиту: запись вне scope не должна влиять на позиции остальных. Текущий
   `QueryFilters` содержит только `path` и `language`, поэтому scope/privacy
   — аддитивные поля фильтра, а не переиспользование существующих; причина
   отсева пишется в `ScoreExplanation.excluded_by`.
4. Результат содержит provenance, `CitationStatus`, причины selected/dropped
   и uncertainty. Пустой результат отличается от отфильтрованного.
5. Перед подтверждением citation, memory candidate или финального ответа
   проверяется generation/snapshot freshness. Устаревший источник даёт
   существующий `stale` (или `updated`, если содержимое найдено по новому
   hash), а не подтверждённый факт. Отдельный статус `unknown` не вводится:
   невозможность проверки выражается `stale` плюс `reason`.
6. Memory records сегодня не имеют собственных векторов:
   `workspace_chunk_vectors` ссылается только на `document_chunks`. Memory
   retrieval поверх общего ranker начинается с lexical-ветки; вектора для
   memory — отдельное аддитивное решение, а не предпосылка этапа.
   Для этого этап добавляет явный adapter `MemoryRecord → RetrievalCandidate`:
   memory-строки не притворяются `document_chunks` и не подмешиваются в SQL
   без scope-предиката. Adapter обязан передать `record_id`, kind, scope,
   privacy, provenance и текстовый source для общего score/ranking pipeline;
   citation для memory указывает на record/evidence ID, а document citation
   сохраняет текущие chunk/hash-поля.

## Изменения по слоям

- Rust core: memory retrieval поверх общего ranker, freshness-фактор и
  tie-break, scope/privacy в `QueryFilters`;
- storage: индексы под scope/privacy фильтры; целостность векторов уже
  обеспечена `FOREIGN KEY ... ON DELETE CASCADE` от
  `workspace_vector_indexes` и `document_chunks`, поэтому проверка — на
  отсутствие обхода каскада, а не на построение нового детектора;
- IPC: bounded breakdown и citation status в projection.

## Проверки

- deterministic tie-break при одинаковых scores, включая равные freshness;
- memory adapter не выдаёт `document_chunks`-цитату для memory record и не
  допускает cross-workspace смешения кандидатов;
- stale/updated citation и generation mismatch;
- cross-workspace и scope isolation, фильтрация до ranking (позиции
  остальных не меняются при добавлении записи вне scope);
- `fallback_fts5` с обеими причинами (`vector_index_unavailable`,
  `vector_index_incompatible`);
- удаление chunk и deprecated index не оставляют строк в
  `workspace_chunk_vectors`;
- replay из записанных входов даёт идентичный порядок и breakdown;
- `cargo test --locked -p evohime-core` (targeted RAG/memory tests).

## Готово, когда

Любой retrieved item объясним, имеет provenance и не может подтвердить факт
после устаревания source или generation.
