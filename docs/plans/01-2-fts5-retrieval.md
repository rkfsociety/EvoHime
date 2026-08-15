# Этап 01.2: Retrieval v1 на SQLite FTS5

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

## Зависимости

Блокирующая зависимость: этап 01.1 — bounded incremental indexing. Он должен
создавать стабильные document/chunk записи, byte range, hash, canonical path,
workspace/project metadata и ordinal.

Опциональная зависимость: evidence checker из 01.1.2. Без него этап всё равно
выдаёт проверенные retrieval-результаты, но не выполняет дополнительную
проверку доказательств; 01.3 не должен считать evidence checker частью
обязательного пути запроса.

Этап 01.2 разблокирует 01.3 (planner вызывает retrieval) и 04.1 (роль
`researcher`). Этап 01.2 не зависит от embeddings или внешнего search service.

## Что этап отдаёт наружу

Bounded lexical retrieval по SQLite FTS5 с фильтрами области видимости,
детерминированным ranking/tie-break, валидацией источника и объяснением score.
Результат не является разрешением на действие: текущая пользовательская
инструкция, sandbox и policy Core имеют приоритет над найденным текстом.

## Контракт хранения

Индекс строится поверх canonical таблиц 01.1:

- `retrieval_documents(document_id INTEGER PRIMARY KEY, workspace_id, project_id,
  canonical_path, language, mime, content_hash, byte_length, file_mtime,
  deleted INTEGER NOT NULL DEFAULT 0, redaction_status TEXT NOT NULL DEFAULT
  'none', indexed_at)`;
- `retrieval_chunks(chunk_id INTEGER PRIMARY KEY, document_id, ordinal,
  byte_start, byte_end, line_start, line_end, chunk_hash, content, symbol,
  symbol_normalized)`;
- `retrieval_fts` — FTS5 с колонками `content`, `symbol_normalized` и
  `canonical_path`, с `content='retrieval_chunks'` и согласованными triggers
  либо одной транзакционной процедурой обновления.

На `workspace_id`, `project_id`, `(workspace_id, project_id)`, `language`,
`mime`, `canonical_path`, `content_hash` и `deleted` создаются обычные SQLite
индексы. FTS5 не используется для metadata-фильтров: сначала выбираются
допустимые document ids по metadata, затем эти ids ограничивают FTS-запрос.
План обязан проверить через `EXPLAIN QUERY PLAN`, что фильтр не превращается
в полный scan.

## Нормализация идентификаторов

Нормализация детерминирована и выполняется до записи в `symbol_normalized` и
до построения MATCH-выражения: Unicode NFC, lowercase по Unicode, замена
последовательностей whitespace на один пробел, удаление завершающей пары `()`.
Разделители внутри идентификатора не удаляются.

Языковые правила v1:

- Java/C#: сохраняются `.`, `::`, `_` и `$` (включая inner classes);
- Python: сохраняются `.`, `::` и `_`;
- JavaScript/TypeScript: сохраняются `.`, `::`, `_` и исходный camelCase
  дополнительно индексируется как есть;
- неизвестный язык: применяется только общая нормализация.

Примеры: `MyClass::method()` → `myclass::method`,
`UserAuthManager.validateToken` → `userauthmanager.validatetoken`.
Trigram tokenizer — обязательный вариант для запросов по `symbol` и
`canonical_path` в v1; его отключение допускается только для corpus, где
конфигурация явно сообщает о деградации поиска составных идентификаторов.

## Запрос и границы результата

Конфигурация запроса содержит три независимых bounded limit с проверкой
диапазона и безопасными значениями по умолчанию:

- `max_retrieval_chunks` — верхняя граница кандидатов FTS;
- `max_evidence_chunks` — граница для evidence checker, не меньше первой;
- `max_context_chunks` — граница prompt builder, не больше второй.

Они применяются последовательно: metadata scope → FTS MATCH → ranking и
tie-break → `max_retrieval_chunks` → validation → optional evidence checker →
`max_evidence_chunks` → prompt builder → `max_context_chunks`. Пустой MATCH
возвращает пустой результат без ошибки; отрицательные, нулевые и чрезмерные
limits отклоняются до выполнения SQL. Целый файл никогда не добавляется в
контекст только из-за одного совпавшего chunk; каждый chunk также ограничен
`max_tokens_per_chunk`.

## Ranking и объяснение score

Основной score — SQLite FTS5 `bm25(retrieval_fts)`, с зафиксированными весами
колонок v1 и без плавающих пользовательских параметров. Сортировка:

1. score по убыванию;
2. `canonical_path` по ordinal byte-лексикографически;
3. `document_id` по возрастанию;
4. `ordinal` по возрастанию;
5. `byte_start` по возрастанию.

Счета считаются равными при `abs(a-b) <= 1e-9`. Все сравнения и преобразования
чисел должны давать одинаковый порядок на повторных запусках.

Каждый результат содержит машинно-читаемое `score_explanation`:

```json
{
  "algorithm": "bm25",
  "column_weights": {"content": 1.0, "symbol_normalized": 2.0, "canonical_path": 0.5},
  "term_frequencies": {"search": 3},
  "document_length": 240,
  "matched_filters": ["workspace_id=...", "language=rust"],
  "excluded_by": []
}
```

## Валидация, stale и redaction

Перед возвратом каждого chunk Core повторно проверяет текущий workspace state:

1. canonical path остаётся внутри разрешённого workspace, файл существует и
   не помечен deleted;
2. текущие `content_hash` и byte length совпадают с индексом;
3. `byte_start..byte_end` находится в текущих границах файла;
4. только после этого вычисляются актуальные line range и возвращается content.

При несовпадении hash/length или границ возвращается metadata с `stale=true`,
`lines=null`, без устаревшего content. `redaction_status` — enum `none`,
`partial`, `full`, устанавливаемый redaction service до retrieval. Для `full`
content не возвращается и chunk исключается из model context; для `partial`
возвращается только уже редактированное содержимое. `stale` и redaction —
разные состояния: stale означает рассогласование источника, redaction —
политику раскрытия. Секреты, токены, cookies, private keys и пароли не должны
индексироваться или попадать в ответ; toxic/security-sensitive MATCH не
обходит этот gate и получает безопасный пустой/редактированный результат.

## Обновление индекса и наблюдаемость

Изменение файла определяется hash + size/mtime. Incremental update в одной
транзакции удаляет старые chunks/FTS rows и вставляет новый набор; deleted
файлы исключаются из кандидатов. Stale обнаруживается на retrieval validation,
а не считается успешным обновлением. Изменение схемы или tokenizer требует
полного reindex с версией схемы и атомарной заменой состояния.

Логируются только redacted metadata: длительность запроса, число кандидатов,
число результатов после каждого gate, cache/index version и score buckets.
Текст запроса, секреты и содержимое документов не логируются. Для corpus до
1M chunks цель v1 — p99 одиночного retrieval < 500 ms, не менее 10
одновременных read-запросов и incremental update нового файла < 1 s; тесты
фиксируют размер corpus и ОС, а превышение цели даёт диагностический failure,
но не меняет безопасность или bounded limits.

## Проверки

- precision/recall на небольшом corpus с русским и английским текстом;
- metadata scope до FTS, workspace isolation и отсутствие path traversal;
- нормализация Java/C#/Python/JS идентификаторов и trigram-поиск;
- одинаковый порядок при одинаковых score, включая одинаковые ordinal;
- empty result, все три limits и ограничение tokens per chunk;
- hash/byte-range validation, modified/deleted file, `stale` и все redaction
  statuses;
- токсичный запрос, секретоподобный документ и путь вне sandbox;
- offline работа без сети и provider;
- incremental update, полный reindex после schema/tokenizer change и
  восстановление после прерванной транзакции;
- performance harness для SLA и `EXPLAIN QUERY PLAN` для metadata filters.

## Критерии готовности

- FTS5 retrieval работает без embeddings и внешней сети;
- схема, индексы metadata, tokenizer, normalization и update policy описаны и
  покрыты тестами;
- каждый результат содержит source path, byte range, актуальные lines либо
  `null`, chunk hash, provenance, score explanation, `stale` и
  `redaction_status`;
- ranking полностью детерминирован, все три limits применяются в указанном
  порядке, целый файл не попадает в context из-за одного chunk;
- security gates не раскрывают секреты и пути вне sandbox;
- evidence checker остаётся опциональным и не блокирует базовый retrieval;
- достигнуты либо измерены SLA v1, а `git diff --check` и offline test проходят.
