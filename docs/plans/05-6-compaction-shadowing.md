# План 05.6 — ContextProjection и append-only shadowing

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — сделать compaction в Context Budget Manager provenance-preserving: model-visible поверхность описывается явной `ContextProjection`, а вытесненное и сжатое evidence не исчезает.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — поле `context_projection` и `MAX_CONTEXT_PROJECTION_BYTES`;
- [05.2](05-2-durable-storage.md) — хранение projection;
- [05.4](05-4-evidence-provenance.md) — `source_refs`, без которых shadowing бессмыслен;
- существующий Context Budget Manager.

### Опциональные

- [05.8](05-8-redaction-and-retention.md) — retention provenance. Без неё append-only shadowing работает корректно, но ничего не удаляет никогда: вытесненные `A`, `B`, `C` копятся, и локальная SQLite растёт неограниченно. Пока 05.8 не сделана, для shadowing действует временный потолок по объёму на задачу; при его достижении старейшие originals сжимаются до metadata + hash с явным статусом, а не молча.

## ContextProjection

Добавить Core-owned понятие `ContextProjection` — фактическую model-visible поверхность после Context Budget Manager.

Минимально:

```text
ContextProjection {
    projection_id
    input_revision
    model
    max_context
    reserved_output_tokens
    entries[]
}
```

Entry:

```text
{
    projection_entry_id
    source_refs[]
    operation
    content
    token_estimate
}
```

`operation`:

```text
include
summary
replace
prune
```

Projection фиксирует:

- включённые evidence;
- исключённые элементы, если это необходимо для объяснимости policy;
- summary/replacement;
- pruning;
- final ordering;
- token estimate.

### Отношение к `context_ledger`

`ContextProjection` — не новая сущность рядом с ledger, а его расширение до model-visible содержимого. `projection_entry_id` ложится на `selected_items[].id`, `operation = summary` и `source_refs[]` — на существующие `compression[].summary_id`/`source_ids`, `operation = prune` — на `dropped_items[].drop_reason`. Второй независимый список выбранных item заводить запрещено.

Двух независимых хешей одного и того же контекста быть не должно:
`context_projection_hash` вычисляется из `context_ledger_hash` и добавленного
content-покрытия по правилам [05.1](05-1-canonical-request-contract.md).
Второй независимый список выбранных item запрещён.

## Append-only shadowing

Compaction не должна уничтожать provenance.

Вместо:

```text
A B C -> delete -> S
```

использовать:

```text
A B C S(source=[A,B,C])
```

Model-visible projection видит `S`, audit/reconstruction surface сохраняет `A`, `B`, `C` и `S`.

Это относится к summary и pruning в Context Budget Manager. Старое evidence не должно исчезать только потому, что больше не помещается в текущий request.

## Тесты

### Unit

- `ContextProjection` совпадает с записью `context_ledger` по составу `selected_items`/`compression`/`dropped_items`;
- `context_projection_hash` детерминирован и связан с `context_ledger_hash`
  способом, объявленным в 05.1;
- превышение `MAX_CONTEXT_PROJECTION_BYTES` даёт типизированную ошибку.

### Integration

1. **Compaction:** originals сохраняются, summary содержит source refs.
2. **Повторный prune:** элемент, вытесненный дважды, не дублируется в audit surface.

## Критерии готовности

1. Каждый summary разрешается в свои originals.
2. Pruning не удаляет evidence из audit surface.
3. Один model call не описывается двумя независимыми списками контекста.
