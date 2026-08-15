# Этап 01.1: Bounded incremental indexing

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

## Зависимости

Блокирующие: существующие filesystem sandbox и SQLite. Context Budget Manager здесь не
требуется — индекс не касается контекста, поэтому этот этап можно вести
параллельно с планом 01.

Разблокирует: 01.2 (retrieval поверх построенного индекса).

## Что этап отдаёт наружу

Таблицы `workspace_documents`, `document_chunks`, FTS5 и `index_runs`.
Индекс хранится в Core/SQLite. Для каждого workspace используется отдельное
опубликованное поколение (`generation`); retrieval читает только последнее
поколение со статусом `published`.

## Контракт SQLite

Минимальная схема должна содержать следующие поля и ограничения:

- `workspace_documents`: `document_id`, `workspace_key`, canonical relative
  `path`, `generation`, `file_hash`, `size_bytes`, `encoding`, `decode_status`,
  `last_modified`, `indexed_at`, `status`, `redaction_status` и
  `is_secret_path`; уникальность `(workspace_key, generation, path)`;
- `document_chunks`: `chunk_id`, `document_id`, `generation`, ordinal,
  `chunk_hash`, `byte_start`, `byte_end`, snapshot line offsets,
  `parent_context`, `chunk_text`, token/byte counts и `strategy_version`;
  уникальность `(document_id, generation, ordinal)` и foreign key на документ;
- `index_runs`: `run_id`, `workspace_key`, `generation`, `started_at`,
  `finished_at`, `status`, scanner/chunker/tokenizer versions, counts,
  cancellation/error summary и `published_at`; статус `published` уникален
  для workspace, предыдущий опубликованный run переводится в `superseded`;
- FTS5 хранит chunk text, path, language/type, symbol/identifier и parent
  context и однозначно связывает строку с `chunk_id` и `generation`.

`workspace_key` должен быть производным от разрешённого workspace, а не от
пользовательского текста. Абсолютные пути в схеме не сохраняются. Foreign key,
индексы по active generation/path/document id и политика удаления должны быть
заданы миграцией. Если реализация выберет другой физический DDL, он обязан
сохранить этот логический контракт.

## Публикация и восстановление run

Run строит новое поколение поэтапно. Запись отдельных bounded batches может
выполняться в отдельных транзакциях, но ни один batch не считается видимым для
retrieval до публикации поколения.

Публикация выполняется короткой SQLite-транзакцией:

1. проверить, что run не отменён и все обязательные scan/chunk операции
   завершились;
2. синхронизировать document chunks и FTS5 для нового поколения;
3. перевести прежний `published` run в `superseded`, новый run — в
   `published`, записать `published_at` и тем самым атомарно сменить
   указатель активного поколения workspace.

При отмене, ошибке, потере lease или падении Core указатель не меняется.
Незавершённый `running` run после restart переводится в `cancelled` или
`failed`, а его поколение не участвует в retrieval. Старое published-поколение
остаётся доступным до успешной публикации нового. Физический GC старых
поколений выполняется только после успешной публикации и не является частью
критического пути retrieval.

Одновременно для workspace допускается только один публикующий run. Повторный
`RebuildIndex` должен получить bounded lease либо быть поставлен в очередь;
публикация устаревшего run после более нового запрещена.

## Содержание

Поддержать README, Markdown, Rust, TypeScript, JSON, TOML, YAML и текстовые
документы. Бинарные файлы по умолчанию исключаются; допустимые парсеры для PDF,
DOCX и других форматов остаются отдельным расширением.

Жёсткие, конфигурируемые лимиты с безопасными defaults:

- максимальный размер текстового файла;
- максимальная длина строки для отсечения minified/dump-файлов;
- ранняя проверка первых байтов на `NUL` и binary-looking content;
- максимальное число chunks на документ и общий budget одного index run.

Источник конфигурации, значения defaults, минимумы/максимумы и поведение при
некорректной конфигурации должны быть зафиксированы в Core config contract и
проверяться до начала run. Отдельно задаются memory/time limits AST parser,
bounded retry на чтение и timeout citation. Workspace не может произвольно
ослабить hard limits.

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

File hash — SHA-256 от исходных байтов стабильного snapshot файла. `chunk_hash`
— SHA-256 от версионированного канонического payload, включающего decoded chunk
text, parent context, тип chunk и `strategy_version`, но не byte offsets и не
snapshot line offsets. Поэтому вставка текста до unchanged chunk требует
обновить offsets, но допускает повторное использование его content hash.

Исходные байты, decoded text и encoding являются разными уровнями данных:
UTF-8 и UTF-16 распознаются явно; invalid UTF-8 для обычного текста допускает
bounded lossy decode с `decode_status=lossy`, а структурные форматы и AST
парсятся только из корректно декодированного текста. Неоговорённая Unicode
нормализация не выполняется; любое изменение правил декодирования или payload
увеличивает соответствующую версию стратегии.

Incremental indexing использует file hash для быстрого пропуска unchanged
файлов и chunk hash для повторного использования неизменившихся chunks. После
сканирования выполняется garbage collection удалённых путей. Изменение файла
до границ последующих chunks может потребовать перестроить их offsets; citation
не должна полагаться на сохранённые номера строк.

Чтение файла выполняется по стабильному протоколу: canonical path проверяется
до открытия и повторно после открытия, snapshot читается bounded способом,
затем metadata/hash проверяются ещё раз. При изменении файла выполняется
ограниченное число повторов. Если стабильный snapshot получить нельзя,
документ получает статус `unstable`, причина попадает в `index_runs`, а
неподтверждённые chunks не публикуются. Удаление между scan и read исключает
документ из нового поколения и не ломает весь run.

FTS5 обновляется вместе с `document_chunks` в транзакциях публикуемых batches.
Удаление или замена chunk обязаны иметь соответствующую операцию в FTS5;
retrieval дополнительно фильтрует `generation` и active document status.
Проверка индекса должна выявлять orphan chunks, ghost FTS rows и расхождение
между FTS5 и основной таблицей.

## Проверки

- deterministic fixtures для unchanged file, changed chunk и удалённого файла;
- sandbox tests на symlink/reparse, `..`, secret paths, binary-looking content
  и workspace escape;
- пустые файлы, UTF-16/некорректная кодировка, файлы без переводов строк,
  minified/очень длинные строки, большие файлы и adversarial chunking;
- Rust/TypeScript AST chunks и fallback chunks с сохранённым parent context;
- cancellation/restart: неполный run не становится published index, старое
  published-поколение остаётся доступным;
- crash после записи bounded batch, concurrent `RebuildIndex` и запрет
  публикации устаревшего run;
- файл изменён во время чтения, файл удалён между scan и read и bounded retry
  для unstable snapshot;
- вставка текста в начало файла с переиспользованием unchanged chunk hash и
  обновлением offsets;
- FTS5 consistency: нет orphan/ghost rows, удалённые chunks не находятся;
- raw UTF-8/UTF-16, invalid encoding, lossy decode и отсутствие скрытой
  Unicode-нормализации;
- лимиты AST memory, chunks, размера файла, длины строки, run budget и
  citation timeout;
- strategy version mismatch оставляет старое поколение доступным до полного
  rebuild нового.

## Критерии готовности

- scanner не читает и не возвращает путь за пределами canonical workspace;
- удалённые документы и chunks не участвуют в retrieval;
- incremental run использует SHA-256 file/chunk hashes и не ломает актуальные
  citations при offset drift;
- FTS5 и `document_chunks` согласованы для опубликованного поколения;
- `RebuildIndex` отменяем, restart-safe, сериализован для workspace и не
  публикует partial или устаревший index;
- конфигурация лимитов валидируется до запуска, а unstable/stale состояния
  диагностируются без выдачи ложной свежей evidence.
