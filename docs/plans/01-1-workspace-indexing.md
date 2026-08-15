# Этап 01.1: Bounded incremental indexing

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

## Зависимости

Блокирующие: существующие filesystem sandbox и SQLite. Context Budget Manager здесь не
требуется — индекс не касается контекста, поэтому этот этап можно вести
параллельно с планом 01.

Разблокирует: 01.2 (retrieval поверх построенного индекса).

## Что этап отдаёт наружу

Таблицы `workspace_documents`, `document_chunks`, FTS5 и `index_runs`.

## Содержание

Поддержать README, Markdown, Rust, TypeScript, JSON, TOML, YAML и текстовые
документы. Бинарные файлы по умолчанию исключаются; допустимые парсеры для PDF,
DOCX и других форматов остаются отдельным расширением.

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

## Проверки

- deterministic fixtures для unchanged file, changed chunk и удалённого файла;
- sandbox tests на symlink/reparse, `..`, secret paths, binary-looking content
  и workspace escape;
- пустые файлы, UTF-16/некорректная кодировка, файлы без переводов строк,
  minified/очень длинные строки, большие файлы и adversarial chunking;
- Rust/TypeScript AST chunks и fallback chunks с сохранённым parent context;
- cancellation/restart: неполный run не становится published index.

## Критерии готовности

- scanner не читает и не возвращает путь за пределами canonical workspace;
- удалённые документы и chunks не участвуют в retrieval;
- incremental run использует file/chunk hashes и не ломает актуальные citations;
- `RebuildIndex` отменяем, restart-safe и не публикует partial index.
