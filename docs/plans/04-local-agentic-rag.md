# План: Локальный Agentic RAG по документации workspace

Статус: draft для реализации после ревью.

## Цель

Дать Еве локальный индекс документации и кода, который умеет не только найти
совпадение, но и проверить качество результата, уточнить запрос и вернуть
ответ с provenance. Внешние cloud search services не требуются.

## Границы и инварианты безопасности

Индекс принадлежит Core/SQLite и строится только внутри разрешённого workspace.
Индекс не является источником права на выполнение команд: права, sandbox,
approval и ограничения инструментов проверяются отдельно перед каждым вызовом.

Сканер обязан до чтения и индексации:

- разрешить только пути под корнем workspace;
- нормализовать абсолютный путь и проверить его через canonical/real path;
- отклонять `..`, абсолютные пути вне корня, symlink/reparse escapes и
  недоступные объекты;
- повторно проверять canonical path после открытия файла, чтобы гонка замены
  symlink/reparse не вывела чтение за пределы workspace;
- применять встроенный denylist секретных путей и пользовательский `.ragignore`
  (gitignore-подобные patterns, включая `.env*`, `*.key`, `secrets/`);
- не индексировать содержимое исключённых путей, даже если оно было найдено
  через другой путь.

Даже прошедший фильтр результат несёт `redaction_status` и `is_secret_path`.
Перед выдачей evidence эти поля проверяются повторно. Данные не отправляются
provider или embeddings backend без отдельного явного режима.

## Целевой pipeline

```text
workspace files
  -> bounded scanner + canonical path policy
  -> parser/chunker + parent context + hashes
  -> SQLite documents/chunks/FTS5 (phase 1)
  -> deterministic query planner
  -> bounded lexical retrieval
  -> evidence checker
  -> context budget selection
  -> answer with citations/confidence
```

Phase 2 добавляет локальные embeddings и hybrid retrieval, не меняя границы
безопасности и offline-инварианты.

## Модель данных

Минимальная схема SQLite:

- `workspace_documents`: `document_id`, canonical relative `path`, language,
  MIME/type, `content_hash`, size, `last_modified`, `indexed_at`,
  `deleted_at`, `redaction_status`, `is_secret_path`;
- `document_chunks`: `chunk_id`, `document_id`, ordinal, byte start/end,
  optional line start/end snapshot, `chunk_hash`, `parent_context`, token/byte
  count и chunker version;
- FTS5 table с содержимым chunk и отдельными полями для path, language,
  symbol/identifier и parent context;
- `index_runs`: run id, started/finished/cancelled status, scanner/chunker/
  tokenizer versions, counts, error summary и dirty flag;
- `context_ledger`: query/run id, selected chunk ids, scores, chunk hashes,
  snippet hashes и selection reason.

Удалённые при полном сканировании документы помечаются `deleted_at`, их chunks
исключаются из retrieval и FTS5. Физическое удаление допускается только после
успешного завершения run и если history не требуется. Отменённый или аварийный
run не публикует неполный индекс как актуальный.

## Этапы

### 1. Bounded incremental indexing

Поддержать README, Markdown, Rust, TypeScript, JSON, TOML, YAML и текстовые
документы. Бинарные файлы по умолчанию исключаются; допустимые парсеры для PDF,
DOCX и других форматов остаются отдельным расширением, а не частью v1.

Жёсткие, конфигурируемые лимиты с безопасными defaults:

- максимальный размер текстового файла;
- максимальная длина строки для отсечения minified/dump-файлов;
- ранняя проверка первых байтов на `NUL` и binary-looking content;
- максимальное число chunks на документ и общий budget одного index run.

Стратегия chunking фиксируется версией:

- Markdown — заголовки `H1..Hn`, цепочка родительских заголовков сохраняется как
  breadcrumb; при слишком большом разделе применяется рекурсивный fallback по
  логическим блокам и max token/byte size;
- Rust/TypeScript — tree-sitter/AST для целых функций, методов, impl/class,
  struct и связанных деклараций; если parser недоступен, применяется
  детерминированный структурный fallback с именем файла и родительским symbol;
- JSON/TOML/YAML — структурные chunks по объектам/ключам с ограничением размера;
- прочий текст — детерминированное рекурсивное разбиение с min/max size.

Каждый chunk получает byte offsets, `chunk_hash`, parent context и snapshot
метаданных файла. Абсолютные line offsets не являются единственным источником
истины: при выдаче citation файл перечитывается и byte range переводится в
актуальные строки. Если file hash изменился, evidence помечается `stale` и не
выдаётся как свежая цитата без reindex/on-read validation.

Incremental indexing использует file hash для быстрого пропуска unchanged
файлов и chunk hash для повторного использования неизменившихся chunks. После
сканирования выполняется garbage collection удалённых путей. Изменение файла
до границ последующих chunks может потребовать перестроить их offsets; citation
не должна полагаться на сохранённые номера строк.

### 2. Retrieval v1: SQLite FTS5

- Начать с SQLite FTS5 и встроенного `bm25()`; внешний search service не нужен.
- Использовать стабильный tie-break: score, canonical path, document id,
  chunk ordinal; случайность и неинициализированные seed запрещены.
- До полнотекстового поиска применять metadata-first фильтры по workspace,
  project, language, MIME и path.
- Для кода индексировать отдельное поле symbol/identifier и поддержать
  составные идентификаторы через детерминированную нормализацию. FTS5 trigram
  допускается как измеряемая оптимизация для code/path search, но не является
  обязательной зависимостью v1.
- Возвращать bounded result set: максимальное число chunks для retrieval,
  evidence checker и model context задаётся отдельно.
- Каждый результат содержит document/chunk ids, path, byte range, актуальные
  lines после validation, score, score explanation, language, hashes,
  redaction status и stale status.
- Не помещать целый файл в prompt только потому, что найден один chunk.

### 3. Deterministic query planner и agentic loop

Planner сначала выполняет локальный pre-check без LLM:

- очевидный path — path search;
- `?`, вопросительные слова или явные команды поиска — lexical search;
- один identifier — exact symbol search;
- иначе — lexical search с ограниченными terms.

Planner возвращает только валидированный JSON по фиксированной схеме:

```json
{
  "need_search": true,
  "strategy": "exact_symbol|lexical|path|metadata",
  "query": "...",
  "filters": {"path": null, "language": null},
  "reason": "...",
  "confidence": 0.0
}
```

Неизвестные поля, недопустимая strategy, пустой query и confidence вне `[0,1]`
отклоняются. LLM rewrite не используется для каждого запроса и не может
изменять scope workspace или security filters.

Agentic loop имеет одновременно hard limit итераций (default 2), wall-clock
timeout и token budget. Стратегии переписывания идут в порядке: exact
symbol/identifier, lexical expansion, path/type filter, затем optional
semantic strategy в phase 2. Вся цепочка rewrite и причины остановки пишутся в
diagnostic log без секретного содержимого.

После каждого retrieval checker вычисляет минимум:

- term/identifier coverage для lexical query;
- наличие независимого evidence для утверждения;
- score threshold и diversity по документам;
- конфликт источников с явным `conflict=true`;
- hash freshness и sandbox validity.

Пороговые значения и формулы versioned/configurable, а не скрытая оценка LLM.
Если конфликт не разрешён deterministic metadata policy (сначала актуальность,
затем явно настроенный path priority), оба источника передаются модели с
пометкой конфликта; ответ не должен выбирать один молча.

При низком coverage planner делает ограниченное rewrite. После исчерпания
итераций, времени или evidence budget система сообщает «данных недостаточно» и
может запросить разрешение на более широкий поиск. UI получает bounded
streaming status: текущая стратегия, число найденных chunks, coverage,
rewrite и причина завершения; частота обновлений ограничена.

### 4. Embeddings как опциональный слой (phase 2)

Embeddings добавляются только после acceptance FTS5 по заранее записанному
evaluation catalog: precision/recall fixtures, latency, bounded context и
корректные citations. Они не обязательны для работы и при ошибке загрузки модели
автоматически откатываются к FTS5 с диагностическим статусом.

Для каждого vector index хранить `embedding_model_id`, model version,
`vector_dimension`, distance metric, chunker version и build status. Несовместимые
векторы не смешивать: новый индекс строится в фоне и публикуется atomic switch
после успешного завершения; при отмене остаётся старый рабочий index.

Для объединения lexical/vector ranking использовать RRF с фиксированным
`k` и bounded объяснением по рангам, а не прямую сумму сырых BM25 и cosine
скоров. В результате показывать вклад lexical/vector. Embeddings можно включать
только для выбранных языков или paths; FTS5 остаётся fallback.

### 5. Citations и context integration

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
metadata, но не полный текст. Извлечённые факты направляются в Memory
Extraction только с provenance и validation. Новые факты сначала получают
`proposed/pending` и confidence; автоматический commit в долговременную память
запрещён без явного подтверждения или отдельной политики.

## IPC и UI

Команды Core:

- `IndexWorkspace`;
- `SearchWorkspaceKnowledge`;
- `GetIndexStatus`;
- `RebuildIndex` с cancellation, bounded progress и restart-safe state.

Команды не дают UI прямого доступа к filesystem или SQLite. UI показывает
indexed files, excluded paths, размер индекса, время последнего полного и
incremental run, текущий статус, stale/dirty state и source links. Progress не
должен спамить UI чаще заданного interval.

## Проверки и evaluation catalog

- deterministic fixtures для unchanged file, changed chunk и удалённого файла;
- sandbox tests на symlink/reparse, `..`, secret paths, binary-looking content
  и workspace escape;
- пустые файлы, UTF-16/некорректная кодировка, файлы без переводов строк,
  minified/очень длинные строки, большие файлы и adversarial chunking;
- Rust/TypeScript AST chunks и fallback chunks с сохранённым parent context;
- retrieval precision, deterministic tie-break, metadata filters, bounded
  evidence count и query rewrite limit;
- тест: файл изменился после retrieval, но до ответа — citation обновляется или
  получает `stale`;
- тесты на prompt/context budget и отсутствие полного файла в context;
- токсичные запросы: агент не выдаёт секреты и пути вне sandbox, даже если
  загрязнённый документ случайно попал в индекс;
- cancellation/restart: неполный run не становится published index;
- offline test: весь pipeline работает без сети и provider;
- evaluation metric: доля документальных утверждений с корректным provenance,
  плюс coverage/conflict/latency и fallback embeddings → FTS5.

## Критерии готовности

- scanner не читает и не возвращает путь за пределами canonical workspace;
- удалённые документы и chunks не участвуют в retrieval;
- incremental run использует file/chunk hashes и не ломает актуальные citations;
- каждый retrieved fact имеет source path, актуальный line range, chunk hash и
  provenance status;
- planner имеет валидируемый контракт, bounded loop и deterministic stop;
- checker использует явные versioned metrics и сообщает unresolved conflicts;
- evidence ограничен и по token budget, и по количеству chunks;
- агент не утверждает документальный факт без evidence или явно маркирует
  uncertainty;
- FTS5 работает без embeddings, а failure embeddings автоматически деградирует
  в FTS5;
- `RebuildIndex` отменяем, restart-safe и не публикует partial index;
- offline режим сохраняется, а индекс не расширяет filesystem permissions.

## Зависимости и ограничения scope

Нужны Context Budget Manager, существующие filesystem sandbox, Memory v1 и
SQLite FTS5. AST parsing для Rust/TypeScript должен быть добавлен только с
проверенной доступностью нужных parser crates; при недоступности действует
детерминированный fallback.

Cross-encoder reranking, Tantivy/Meilisearch, PDF/DOCX/HTML parsers, plugin API
и полноценная схема миграции внешних индексов не входят в первую реализацию.
Изменения schema/chunker/tokenizer в v1 требуют version bump и controlled full
rebuild; отдельный migration path может быть добавлен после измерения стоимости
rebuild.

Evaluation catalog должен включать retrieval correctness, security leakage,
incremental freshness, cancellation, latency и citation checks.
