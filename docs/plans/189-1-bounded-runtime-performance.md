# 189-1 — Baseline и Core UTF-8 decode

## Зависимости

Blocking: [189-0](189-0-bounded-runtime-performance.md). Optional: нет.

## Изменения и контракт

1. Проверить текущие реализации пяти участков из overview и существующие
   тесты; зафиксировать повторяемые fixtures и методику baseline.
2. В `crates/evohime-core/src/workspace_rag.rs::decode_text` сначала проверять
   заимствованный буфер через `std::str::from_utf8`, затем создавать итоговую
   owned строку. Для invalid UTF-8 сохранить lossy replacement и decode status;
   не менять UTF-16 BOM paths, сохранение исходных bytes и chunk semantics.
3. Использовать существующие unit tests RAG; добавить только недостающие
   случаи valid/invalid UTF-8, empty, UTF-16LE/BE и эквивалентность результата.

SQLite schema, migration, backup, IPC и pool ownership не меняются.
Для valid UTF-8 итоговая owned строка всё равно нужна; уменьшение аллокаций
на этом пути не обещается. Основной кандидат — исключение временной копии
на invalid UTF-8; оставить правку только при ясной пользе и эквивалентности.

## Verification и готовность

- Запустить targeted RAG decode tests и `cargo check --locked -p evohime-core`.
- Сравнить decode на одинаковых buffers с указанием размеров и кодировок;
  не запускать полную индексацию репозитория ради микроправки.
- Зафиксировать baseline UI/logger/workflow scenarios для следующих этапов.
- Если decode-оптимизация не оправдана, явно сохранить решение и причину,
  не блокируя остальные подтверждённые исправления.

## Recovery, rollback и release

Поведение restart/recovery неизменно. Rollback — обычный revert decode-правки.
При изменении публикуемых Core sources повысить только patch marker Core
по действующей module map; для одних тестов/замеров marker не менять.
Результаты и ограничения проверок записать в canonical release evidence.
