# Specialized ML handlers: summarize + chunk

> Дата: 2026-07-16  
> Статус: approved

## Цель

Добавить прикладные stdlib-хендлеры `text.summarize` и `text.chunk` в Python worker с зеркальной валидацией payload на Rust.

## Контракт

### `text.summarize`

- **Payload:** `{ "text": string, "max_sentences"?: int }`
  - `max_sentences` default `3`, range `1..20`
- **Result:** `{ "summary": string, "sentences_used": int, "source_sentences": string[] }`
- **Алгоритм:** extractive — частота слов (как keywords), score предложений, top-N в исходном порядке, склейка через пробел.

### `text.chunk`

- **Payload:** `{ "text": string, "chunk_size"?: int, "overlap"?: int }`
  - `chunk_size` default `500`, range `64..8000`
  - `overlap` default `50`, `>= 0` и `< chunk_size`
- **Result:** `{ "chunks": [{ "index", "text", "start", "end" }], "count": int }`
- **Алгоритм:** скользящее окно по символам с overlap; последний кусок может быть короче.

Общий лимит: `len(text) <= 1_000_000`. Пустой текст — валидный результат (пустой summary / пустые chunks).

## Границы

- Без внешних ML-зависимостей и сети
- Worker process-local; durability у Rust
- Обновить `SUPPORTED_TASKS` + `validate_task_payload` в Python и Rust

## Тесты

- Python: summarize ranking/order, chunk overlap/bounds, bad payload 400
- Rust: schema validation for both tasks
