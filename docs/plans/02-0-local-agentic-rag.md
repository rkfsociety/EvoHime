# План 02: Локальный Agentic RAG по документации workspace

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

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
  -> SQLite documents/chunks/FTS5 (этап 02.1)
  -> deterministic query planner
  -> bounded lexical retrieval
  -> evidence checker
  -> context budget selection
  -> answer with citations/confidence
```

Этап 02.4 добавляет локальные embeddings и hybrid retrieval, не меняя границы
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

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 02.1 | [Bounded incremental indexing](02-1-workspace-indexing.md) | `workspace_documents`, `document_chunks`, FTS5 и `index_runs` | 02.2 |
| 02.2 | [Retrieval v1: SQLite FTS5](02-2-fts5-retrieval.md) | bounded lexical retrieval с deterministic tie-break | 02.3, 05.1 |
| 02.3 | [Query planner и agentic loop](02-3-query-planner-and-checker.md) | planner, evidence checker и bounded loop | 02.5, 05.1 |
| 02.4 | [Embeddings как опциональный слой](02-4-optional-embeddings.md) | hybrid retrieval поверх 02.2 | — |
| 02.5 | [Citations и context integration](02-5-citations-and-context.md) | selected evidence blocks с citations | Context Budget Manager, Memory Extraction |

Порядок: 02.1 → 02.2 → 02.3 → 02.5. Этап 02.4 опционален и выполняется
последним; без него retrieval работает на FTS5.

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

## Зависимости плана

Блокирующие: этап 01.1 — он принимает selected evidence blocks и владеет
context ledger, в который этап 02.5 пишет причину выбора; существующие
filesystem sandbox, Memory v1 и SQLite FTS5. Остальные этапы плана 01 этому
плану не нужны, а этапы 02.1–02.3 можно делать параллельно с 01.1: индекс,
retrieval и planner не касаются контекста.

Реализованный Memory Extraction (см. [`../architecture.md`](../architecture.md))
принимает извлечённые факты через свой policy gate: факт приходит с provenance
и попадает в `pending_confirmation`, автоматический commit в долговременную
память запрещён. Этот план обязан пользоваться тем же контрактом, а не заводить
собственный путь записи в память. Обратная сторона той же связи: этот план
поставляет validation для document и tool evidence, из-за которой такие
кандидаты сейчас остаются pending.

## Ограничения scope

AST parsing для Rust/TypeScript должен быть добавлен только с проверенной
доступностью нужных parser crates; при недоступности действует
детерминированный fallback.

Cross-encoder reranking, Tantivy/Meilisearch, PDF/DOCX/HTML parsers, plugin API
и полноценная схема миграции внешних индексов не входят в первую реализацию.
Изменения schema/chunker/tokenizer требуют version bump и controlled full
rebuild; отдельный migration path может быть добавлен после измерения стоимости
rebuild.

Evaluation catalog должен включать retrieval correctness, security leakage,
incremental freshness, cancellation, latency и citation checks.
