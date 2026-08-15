# Этап 01.4: Embeddings как опциональный слой

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

Этап целиком опционален: без него retrieval работает на FTS5, и остальные
этапы плана от него не зависят.

## Зависимости

Блокирующие: этап 01.2 (lexical retrieval, поверх которого строится hybrid) и
принятый evaluation catalog по нему.

Разблокирует: никого.

## Что этап отдаёт наружу

Hybrid retrieval поверх 01.2 с автоматическим откатом к FTS5.

## Содержание

Embeddings добавляются только после acceptance FTS5 по заранее записанному
evaluation catalog: precision/recall fixtures, latency, bounded context и
корректные citations. Они не обязательны для работы и при ошибке загрузки
модели автоматически откатываются к FTS5 с диагностическим статусом.

Для каждого vector index хранить `embedding_model_id`, model version,
`vector_dimension`, distance metric, chunker version и build status.
Несовместимые векторы не смешивать: новый индекс строится в фоне и публикуется
atomic switch после успешного завершения; при отмене остаётся старый рабочий
index.

Для объединения lexical/vector ranking использовать RRF с фиксированным
`k` и bounded объяснением по рангам, а не прямую сумму сырых BM25 и cosine
скоров. В результате показывать вклад lexical/vector. Embeddings можно включать
только для выбранных языков или paths; FTS5 остаётся fallback.

## Проверки

- fallback embeddings → FTS5 при ошибке загрузки модели;
- несовместимые векторы не смешиваются, отменённая сборка оставляет старый
  рабочий индекс;
- ranking объясняется вкладом lexical и vector, а не сырой суммой скоров.

## Критерии готовности

- failure embeddings автоматически деградирует в FTS5;
- atomic switch не публикует незавершённый vector index;
- offline режим сохраняется.
