# Worker ML handlers: similarity + entities

> Дата: 2026-07-17  
> Статус: done

## Цель

Расширить stdlib ML-хендлеры Python worker: `text.similarity` и `text.entities` с зеркальной валидацией payload на Rust.

## Контракт

### `text.similarity`

- **Payload:** `{ "text_a": string, "text_b": string }`
- **Result:** `{ "score": float, "shared_tokens": int, "tokens_a": int, "tokens_b": int }`
- **Алгоритм:** bag-of-words cosine по токенам длиной > 2 (как keywords/summarize).

### `text.entities`

- **Payload:** `{ "text": string }`
- **Result:** `{ "urls": string[], "emails": string[], "paths": string[], "tickets": string[], "counts": { ... } }`
- **Алгоритм:** regex heuristics; дедуп casefold с сохранением порядка.

Общий лимит: `len(text*) <= 1_000_000`.

## Границы

- Без внешних ML-зависимостей и сети
- Worker process-local; durability у Rust
