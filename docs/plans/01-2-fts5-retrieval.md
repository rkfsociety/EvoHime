# Этап 01.2: Retrieval v1 на SQLite FTS5

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

## Зависимости

Блокирующие: этап 01.1 — retrieval работает поверх построенного индекса.

Разблокирует: 01.3 (planner вызывает retrieval) и 04.1 (роль `researcher`).

## Что этап отдаёт наружу

Bounded lexical retrieval с deterministic tie-break и объяснением score.

## Содержание

- Начать с SQLite FTS5 и встроенного `bm25()`; внешний search service не нужен.
- Использовать стабильный tie-break: score, canonical path, document id,
  chunk ordinal; случайность и неинициализированные seed запрещены.
- До полнотекстового поиска применять metadata-first фильтры по workspace,
  project, language, MIME и path.
- Для кода индексировать отдельное поле symbol/identifier и поддержать
  составные идентификаторы через детерминированную нормализацию. FTS5 trigram
  допускается как измеряемая оптимизация для code/path search, но не является
  обязательной зависимостью.
- Возвращать bounded result set: максимальное число chunks для retrieval,
  evidence checker и model context задаётся отдельно.
- Каждый результат содержит document/chunk ids, path, byte range, актуальные
  lines после validation, score, score explanation, language, hashes,
  redaction status и stale status.
- Не помещать целый файл в prompt только потому, что найден один chunk.

## Проверки

- retrieval precision, deterministic tie-break, metadata filters и bounded
  evidence count;
- одинаковый запрос на неизменном индексе даёт одинаковый порядок результатов;
- токсичные запросы: агент не выдаёт секреты и пути вне sandbox, даже если
  загрязнённый документ случайно попал в индекс;
- offline test: retrieval работает без сети и provider.

## Критерии готовности

- FTS5 работает без embeddings;
- каждый результат имеет source path, актуальный line range, chunk hash и
  provenance status;
- целый файл не попадает в контекст из-за одного совпавшего chunk.
