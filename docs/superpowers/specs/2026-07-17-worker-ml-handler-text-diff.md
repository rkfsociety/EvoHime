# Worker ML handler: text.diff

> Дата: 2026-07-17  
> Статус: done

## Цель

Добавить stdlib-хендлер `text.diff` рядом с `text.similarity`: построчный unified diff и счётчики изменений, с зеркальной валидацией payload на Rust.

## Контракт

### `text.diff`

- **Payload:** `{ "text_a": string, "text_b": string, "context"?: int, "max_diff_lines"?: int }`
  - `context` — контекст unified diff (`0..20`, default `3`)
  - `max_diff_lines` — потолок длины `unified_diff` (`1..2000`, default `500`)
- **Result:**
  - `ratio` — `difflib.SequenceMatcher.ratio()` по строкам
  - `lines_a` / `lines_b` / `lines_equal` / `lines_added` / `lines_removed`
  - `unified_diff` — массив строк unified diff
  - `diff_truncated` — `true`, если вывод обрезан по `max_diff_lines`

Общий лимит: `len(text_*) <= 1_000_000`.

## Границы

- Только stdlib (`difflib`), без внешних зависимостей
- Не замена `git.diff` tool — это сравнение двух строк в worker job
