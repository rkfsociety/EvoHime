# Этап 01.5: Citations и context integration

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

## Зависимости

Блокирующие: этап 01.3 (evidence и checker). Context Budget Manager реализован и принимает
selected evidence blocks и владеет ledger, куда пишется причина выбора. Это
единственный этап плана, опирающийся на контракт Context Budget Manager.

Разблокирует: документные цитаты в контексте и приём фактов реализованным
Memory Extraction.

## Что этап отдаёт наружу

Selected evidence blocks с compact citations и записью причины выбора в
context ledger.

## Содержание

Context Budget Manager получает только selected evidence blocks. Отбор:

1. проверить актуальность hash и sandbox policy;
2. отсортировать по deterministic retrieval/checker score;
3. жадно добавлять chunks до token budget и отдельного chunk-count limit;
4. для каждого chunk добавить минимальный parent context: path, language,
   breadcrumb/symbol и нужные соседние строки;
5. записать причину выбора в context ledger.

В model context передаются compact citations, а в финальном ответе —
`path:line-start-line-end`, `chunk_hash` и статус `stale`, если источник
изменился. Перед финальной выдачей Evidence Checker повторно валидирует hash
файла; при рассинхронизации citation обновляется после re-read либо явно
помечается stale.

В ledger сохраняются ids, ranks/scores, hashes, snippet hash и selected
metadata, но не полный текст. Извлечённые факты направляются в реализованный
Memory Extraction (см. [`../architecture.md`](../architecture.md)) только с
provenance и validation, через его существующий policy gate: кандидат получает
`pending_confirmation`, а автоматический commit в долговременную память
запрещён без явного подтверждения. Отдельного пути записи в память этот этап не
создаёт.

## Проверки

- тест: файл изменился после retrieval, но до ответа — citation обновляется или
  получает `stale`;
- тесты на prompt/context budget и отсутствие полного файла в context;
- ledger содержит ids, ranks, hashes и snippet hash, но не полный текст;
- evaluation metric: доля документальных утверждений с корректным provenance,
  плюс coverage/conflict/latency.

## Критерии готовности

- каждый retrieved fact имеет source path, актуальный line range, chunk hash и
  provenance status;
- evidence ограничен и по token budget, и по количеству chunks;
- факт, отправленный в память, попадает в `pending_confirmation`, а не
  становится активной памятью автоматически.
