# 11-2 — Evidence и deterministic retrieval

## Цель

Сделать memory/RAG retrieval bounded, объяснимым и воспроизводимым поверх
существующего `workspace_rag.rs`, без второго ranker.

## Что уже есть в checkout

- `QueryPlan`, `QueryFilters`, `RetrievalLimits`, `QueryStrategy` и
  `HybridConfig`;
- `ScoreExplanation` и `RankingExplanation` — готовая основа score
  breakdown;
- `CitationStatus`, `Citation`, `SearchDiagnostics`, `RetrievalProgress` и
  `ContextBuildResult`;
- SQLite/FTS5 generation и bounded indexing с progress.

Этап связывает эти структуры с memory records и закрывает freshness/
fallback-разрывы.

## Зависимости

### Блокирующие

- 11-1: typed record с `evidence_refs` и `execution_event_refs`;
- текущие `workspace_rag.rs` и его generation/citation типы.

### Опциональные

- local embeddings и vector cache. Без них `QueryStrategy` деградирует к
  FTS5-only, breakdown содержит только lexical факторы, а результат
  помечается deterministic-фолбэком, не ошибкой;
- telemetry плана 12 для агрегатов retrieval quality.

## Контракт

1. Связать evidence record с document chunk, tool receipt, observation и
   execution event ID; отсутствующий или неизвестный ID — typed ошибка, а не
   молча пустая ссылка.
2. Ranking — multi-factor с фиксированным tie-break (стабильный порядок по
   score, затем по freshness, затем по ID) и обязательным score breakdown в
   `RankingExplanation`.
3. Фильтры scope и privacy применяются до ranking, а не после усечения по
   лимиту: запись вне scope не должна влиять на позиции остальных.
4. Результат содержит provenance, `CitationStatus`, причины selected/dropped
   и uncertainty. Пустой результат отличается от отфильтрованного.
5. Перед подтверждением citation, memory candidate или финального ответа
   проверяется generation/snapshot freshness; stale источник даёт
   `stale`/`unknown`, а не подтверждённый факт.

## Изменения по слоям

- Rust core: memory retrieval поверх общего ranker, freshness-проверка;
- storage: индексы под scope/privacy фильтры и orphan-embedding detection;
- IPC: bounded breakdown и citation status в projection.

## Проверки

- deterministic tie-break при одинаковых scores;
- stale/missing citation и generation mismatch;
- cross-workspace и scope isolation, фильтрация до ranking;
- FTS5-only fallback и обнаружение orphan embeddings;
- replay из записанных входов даёт идентичный порядок и breakdown;
- `cargo test --locked -p evohime-core` (targeted RAG/memory tests).

## Готово, когда

Любой retrieved item объясним, имеет provenance и не может подтвердить факт
после устаревания source или generation.
