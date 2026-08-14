# План: Локальный Agentic RAG по документации workspace

Статус: draft для ревью.

## Цель

Дать Еве локальный индекс документации и кода, который умеет не только найти
совпадение, но и проверить качество результата, уточнить запрос и вернуть
ответ с provenance. Внешние cloud search services не требуются.

## Границы

Индекс принадлежит Core/SQLite и строится только внутри разрешённого workspace.
Symlink/reparse escapes, секретные каталоги и файлы вне sandbox исключаются.
Индекс не является источником права на выполнение команд.

## Целевой pipeline

```text
workspace files
  -> bounded scanner
  -> normalized chunks + hashes
  -> SQLite FTS5/local embeddings (phase 2)
  -> query planner
  -> lexical/hybrid retrieval
  -> evidence checker
  -> answer with citations and confidence
```

## Этапы

### 1. Индексация

- Ввести `workspace_documents`, `document_chunks`, `index_runs` и content hash.
- Поддержать README, Markdown, Rust, TypeScript, JSON, TOML, YAML и текстовые
  документы; бинарные файлы только как metadata.
- Резать по заголовкам и логическим блокам, сохраняя path, line offsets,
  language и source hash.
- Игнорировать `.git`, `target`, `node_modules`, secrets/auth paths и файлы,
  которые sandbox уже блокирует.
- Индексация incremental: unchanged hash не перечитывать.

### 2. Retrieval v1

- Начать с SQLite FTS5/BM25 и deterministic tie-break.
- Поддержать scope workspace/project и фильтры file type/path.
- Возвращать bounded chunks с `document_id`, `chunk_id`, path, lines, score и
  redaction status.
- Не помещать целый файл в prompt только потому, что найден один chunk.

### 3. Agentic loop

- Query planner определяет, нужен ли поиск и какой тип источника нужен.
- После retrieval checker проверяет покрытие вопроса, конфликт источников и
  свежесть hash.
- При низком coverage переписывать запрос максимум N раз и менять стратегию:
  exact symbol → lexical terms → path/type filter.
- После исчерпания budget честно сообщать «данных недостаточно» и просить
  разрешение на более широкий поиск.

### 4. Embeddings как опциональный слой

- Добавлять локальные embeddings только после FTS5 acceptance.
- Хранить embedding model id и vector version; смена модели запускает
  background reindex, не смешивая несовместимые векторы.
- Hybrid score должен быть bounded и объяснимым; lexical hit остаётся
  fallback при отсутствии локальной embedding model.
- Не отправлять документы в provider без отдельного явного режима.

### 5. Citations и context integration

- В model context передавать только selected evidence blocks и компактные
  citations.
- В финальном ответе показывать path:line для локальных источников.
- В context ledger сохранять ids/scores/hashes, а не полный текст.
- Извлечённые факты направлять в Memory Extraction только с validation и
  provenance на document chunk.

## IPC и UI

- Read-only commands: `IndexWorkspace`, `SearchWorkspaceKnowledge`,
  `GetIndexStatus`, `RebuildIndex` с bounded progress.
- UI показывает indexed files, last index hash, excluded paths и source links.
- `RebuildIndex` не должен блокировать чат; cancellation и restart-safe state
  обязательны.

## Проверки

- deterministic indexing fixtures с изменённой строкой и unchanged file;
- sandbox tests на symlink/reparse, secret paths и workspace escape;
- retrieval precision fixtures по EvoHime docs/code;
- query rewrite limit и no-result behavior;
- citation line validity после файла изменился;
- prompt-size and context-budget integration;
- offline test: весь pipeline работает без сети и provider.

## Критерии готовности

- новая строка переиндексирует только затронутый chunk;
- каждый retrieved fact имеет source path/line/hash;
- агент не утверждает документальный факт без evidence или явно маркирует
  uncertainty;
- FTS5 работает без embeddings, embeddings не являются обязательными;
- индекс не расширяет filesystem permissions.

## Зависимости

Нужны Context Budget Manager, существующие filesystem sandbox и Memory v1.
Evaluation catalog должен включать retrieval correctness и citation checks.
