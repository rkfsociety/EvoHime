# 11-2 — Evidence и deterministic retrieval

## Цель

Сделать retrieval bounded, explainable и воспроизводимым на данных текущего
workspace/RAG поколения.

## Изменения

1. Увязать source/evidence records с document chunk, tool receipt,
   observation и execution event IDs.
2. Реализовать multi-factor ranking с фиксированным tie-break, score breakdown,
   freshness, scope и consent filters.
3. Сохранить hybrid retrieval и local embedding cache как optional слой;
   отсутствие embeddings всегда даёт deterministic FTS5 fallback.
4. Возвращать provenance, citation status, selected/dropped reasons и
   uncertainty вместе с retrieval result.
5. Проверять snapshot/generation freshness перед подтверждением citation,
   memory candidate или финального ответа.

## Проверки

- deterministic tie-break при одинаковых scores;
- stale/missing citation и generation mismatch;
- cross-workspace/scope isolation;
- FTS5-only fallback и orphan embedding detection;
- replay из записанных входов с одинаковым результатом.

## Готово, когда

Любой retrieved item объясним, имеет provenance и не может подтвердить факт
после устаревания source или generation.
