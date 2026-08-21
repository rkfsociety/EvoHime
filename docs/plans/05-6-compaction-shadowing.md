# План 05.6 — ContextProjection и append-only shadowing

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — сделать compaction в Context Budget Manager provenance-preserving:
model-visible поверхность описывается ровно тем `ContextProjection`, который
определён в 05.1, а вытесненное и сжатое evidence не исчезает из audit surface.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — контракт
  `context_projection`, правила `context_projection_hash` и
  `MAX_CONTEXT_PROJECTION_BYTES`;
- [05.2](05-2-durable-storage.md) — `model_requests`,
  `model_request_sources` и атомарная запись committed envelope;
- [05.4](05-4-evidence-provenance.md) — `source_refs` и разрешение
  immutable source state;
- существующий Context Budget Manager.

### Опциональные

- [05.8](05-8-redaction-and-retention.md) — окончательные правила удаления и
  retention. До 05.8 shadowing всё равно работает append-only, но для
  ограничения роста действует временный потолок
  `MAX_SHADOW_BYTES_PER_TASK`. При достижении потолка оригинал переводится в
  явное состояние `metadata_hash_only`; это не `REQUEST_SOURCE_MISSING` и не
  успешная реконструкция полного текста. После 05.8 те же строки подчиняются
  `redacted`/`retention_pruned` и правилам удаления хеша источника.

## ContextProjection

`ContextProjection` не является отдельной сущностью рядом с ledger. Это
каноническое расширение **той же** записи `context_ledger`, которое попадает в
`ModelRequestEnvelopeV1.context_projection` строго по контракту 05.1.

Нормативная форма:

```text
ContextProjection {
    ledger_id
    context_ledger_hash
    entries[]
    context_projection_hash
}
```

Обязательные свойства:

- `ledger_id` равен верхнеуровневому `ModelRequestEnvelopeV1.ledger_id` и
  ссылается на `context_ledger.id`;
- `context_ledger_hash` равен hash именно этой строки ledger;
- `entries[]` — детерминированное представление существующих
  `selected_items[]`, `compression[]` и `dropped_items[]`, а не новый список
  контекста; для `include`/`summary` сохраняется фактический model-visible
  порядок, для `prune` — порядок ledger и причина исключения;
- `context_projection_hash` вычисляется строго по формуле 05.1:

  ```text
  SHA-256("evohime-context-projection-v1\0" ||
          context_ledger_hash_bytes ||
          JCS(projection_content_coverage))
  ```

  Другая формула, второй независимый hash или hash «по content-покрытию» без
  этой domain-separated формулы запрещены.

`entries[]` не дублирует `content` или `token_estimate`. Фактические bytes
model-visible prompt/message/tool blocks хранятся и проверяются по правилам
05.2. Entry содержит только ссылки и coverage, необходимые для связи с
ledger и для hash:

```text
ContextProjectionEntry {
    projection_entry_id
    operation              -- include | summary | prune
    source_refs[]
    block_ref_id?          -- opaque ref на model-visible block
}
```

`projection_entry_id` не создаёт новую независимую идентичность:

- `include` использует `selected_items[].id`;
- `summary` использует `compression[].summary_id`, а `source_refs` разрешают
  `compression[].source_ids` до исходных evidence;
- `prune` использует `dropped_items[].id`, а причина берётся из
  `dropped_items[].drop_reason`.

`operation = replace` не вводится. Замена нескольких originals сводным
текстом — это `summary` с `source_refs` на originals; summary summary — ещё
одна запись `summary` с source ref на предыдущий summary и транзитивными
ссылками на его originals.

`include` означает, что item вошёл в projection. `summary` означает, что
model-visible является summary, а не его originals. `prune` означает, что item
не вошёл в model-visible prompt, но его drop reason и provenance остаются в
projection/audit surface. Для каждой операции проверяется точное отображение
на соответствующее поле ledger; entry без разрешимого источника блокирует
commit.

Параметры `model`, `max_context` и `reserved_output_tokens` остаются входами и
результатом Context Budget Manager, но не добавляются в `ContextProjection`:
они уже представлены в ledger/profile или в envelope и не должны создавать
вторую схему идентичности projection.

Shadowing изменяет audit storage, но сам по себе не меняет
`context_projection_hash`: в hash входит только фактическая model-visible
projection и её `projection_content_coverage`. Новый summary или изменение
его model-visible content меняет coverage и hash; сохранение того же
model-visible результата с добавлением shadowed original hash не меняет hash.

## Durable shadow records

Shadowed originals сохраняются Core-owned append-only записями. Одного
`model_request_sources` недостаточно: эта таблица хранит request-level refs и
не хранит captured original payload. 05.6 добавляет в storage contract:

```text
context_shadowed_originals
- shadow_id PK
- ledger_id NOT NULL FK context_ledger(id)
- request_id NOT NULL FK model_requests(request_id)
- original_kind             -- selected | compression | dropped
- original_id
- operation                 -- summary | prune
- parent_shadow_id NULL FK context_shadowed_originals(shadow_id)
- content_block_hash NULL FK context_shadow_blocks(content_hash)
- source_state              -- full | metadata_hash_only | redacted | retention_pruned
- original_content_hash NULL
- byte_len NOT NULL
- created_at
- UNIQUE (request_id, original_kind, original_id, operation)
```

```text
context_shadow_source_refs
- shadow_id
- request_id
- source_ref_ordinal
- source_ordinal
- PRIMARY KEY (shadow_id, source_ref_ordinal)
- FK (request_id, source_ordinal) -> model_request_sources(request_id, ordinal)
```

```text
context_shadow_blocks
- content_hash PK
- byte_len NOT NULL
- bytes NULL
```

`context_shadow_blocks` — отдельное content-addressed хранилище captured
originals, чтобы его refcount и retention не смешивались с
`model_request_blocks` prompt/message/tool payload. В `full` состоянии
`bytes` обязаны соответствовать `content_hash` и `byte_len`; в
`metadata_hash_only` bytes равны `NULL`, а `original_content_hash` и
метаданные shadow row сохраняются. Hash-only переход никогда не выполняется
молча.

Каждый shadow row привязан одновременно к `ledger_id`, request и точным
`model_request_sources` через `(request_id, source_ordinal)`. Запись
projection, source refs, shadow rows и block refs выполняется одной
транзакцией committed envelope. Forward reference, отсутствие source row,
несовпадение source hash или несовпадающий content hash откатывают всю
транзакцию.

## Временный потолок до 05.8

До появления retention из 05.8 действует нормативная константа:

```text
MAX_SHADOW_BYTES_PER_TASK = 8 * 1024 * 1024
```

Единица измерения — байты captured UTF-8/BLOB payload в уникальных строках
`context_shadow_blocks`, сгруппированных по `ledger.task_id`; metadata,
source refs и hash не входят в счётчик. Дедупликация по `content_hash`
учитывается один раз, но каждый ledger/task сохраняет свои provenance-ссылки.

Перед commit Core считает новый bounded total. Если потолок превышен,
детерминированно, от старых к новым, переводятся наименее свежие полные
shadow blocks в `metadata_hash_only` до прохождения лимита. При равенстве
используются `(created_at, shadow_id)`. Если один новый original сам больше
потолка, он сразу сохраняется как metadata + hash с тем же состоянием.

В результате:

- текущий summary/pruned model request может быть dispatchable, если его
  собственные model-visible blocks полны и все обязательные provenance refs
  разрешены;
- исходный full text, переведённый в `metadata_hash_only`, больше не считается
  реконструируемым; resolver возвращает typed
  `REQUEST_SHADOW_CONTENT_COMPACTED`, а не `REQUEST_SOURCE_MISSING`;
- append-only evidence row, source refs, hash (если политика источника это
  разрешает) и факт compaction сохраняются;
- пишется bounded audit event со статусом `metadata_hash_only`, причиной
  `temporary_shadow_cap`, освобождёнными байтами и числом затронутых rows;
- превышение лимита не приводит к неограниченному росту SQLite и не разрешает
  молча заменить старый original текущим содержимым workspace/memory.

Константа, единица измерения, порядок compaction, состояние и typed error
обязательны в реализации и known-answer/integration tests. После 05.8 новые
dispatchable requests всегда используют полные captured blocks; старые
`metadata_hash_only` rows не объявляются полными задним числом.

## Append-only shadowing и операции

Compaction не удаляет provenance:

```text
A B C -> summary S
audit: A B C S(source_refs=[A,B,C])
model-visible: S
```

Правила операций:

- `summary`: создаётся один shadow row на каждый original из
  `compression.source_ids`; summary получает source refs на эти rows и на
  соответствующие `model_request_sources`;
- `prune`: создаётся shadow row для каждого dropped item с его
  `drop_reason`; текст не попадает в projection, но captured original и
  provenance сохраняются, пока не сработает cap/redaction/retention;
- `include`: отдельного shadow row не требует, но его `source_refs` должны
  указывать на сохранённые immutable sources;
- повторный prune идемпотентен по уникальному ключу и не создаёт второй
  audit-копии того же original;
- при S1 → S2 новый summary получает `parent_shadow_id = S1`, а исходные
  source refs разрешаются транзитивно. Один captured block может быть
  deduplicирован, но provenance edges не схлопываются и не теряются;
- partial write, цикл `parent_shadow_id` или source ref на сам summary
  блокируют commit.

## Взаимодействие с 05.8

05.8 владеет lifecycle shadow rows и обязан включать их в ту же транзакцию,
что и изменение источника, `model_request_sources` и envelope:

- `forget` памяти тумбстоунирует shadow payload, но сохраняет digest памяти,
  если это разрешено правилом 05.8;
- удаление ambient-эпизода и `forget_window` удаляют captured bytes и hash
  shadow/source rows по ambient-правилу, оставляя только разрешённую metadata
  tombstone;
- retention переводит старые full rows в `retention_pruned`/metadata + hash
  по правилам источника и receipts; это состояние отличается от временного
  `metadata_hash_only` cap;
- удаление `context_ledger` не может оставить shadow row: сначала выполняется
  согласованная retention-транзакция для request, shadow rows и sources,
  затем разрешается удаление ledger;
- для действительно отсутствующей pre-05.6 записи допускается
  `REQUEST_SOURCE_MISSING`; наличие shadow row в
  `metadata_hash_only`, `redacted` или `retention_pruned` всегда возвращает
  соответствующее typed-состояние, а не маскируется под missing.

## Тесты

### Unit

- projection имеет ровно `ledger_id`, `context_ledger_hash`, `entries[]` и
  `context_projection_hash` из 05.1; в entry нет дублирующих `content`/
  `token_estimate`;
- `ledger_id` и `context_ledger_hash` совпадают с ledger, а второй список
  selected items не создаётся;
- `include`/`summary`/`prune` разрешаются соответственно в
  `selected_items`/`compression`/`dropped_items`;
- `replace` отвергается, а replacement принимается только как `summary`;
- `context_projection_hash` совпадает с точной формулой 05.1 и меняется при
  изменении model-visible structure/coverage, но не при одном лишь сохранении
  shadow metadata; изменение самих bytes проверяется через `envelope_hash`;
- атомарная запись `model_request_sources`, shadow rows и block refs;
- source mapping, hash/byte validation, цикл и forward reference;
- превышение `MAX_CONTEXT_PROJECTION_BYTES` даёт typed error;
- превышение `MAX_SHADOW_BYTES_PER_TASK` переводит rows в
  `metadata_hash_only` детерминированно, без unbounded growth;
- metadata/hash-only row возвращает `REQUEST_SHADOW_CONTENT_COMPACTED`, а
  старый действительно отсутствующий source — `REQUEST_SOURCE_MISSING`.

### Integration

1. **Compaction:** originals сохраняются, summary содержит source refs,
   projection и ledger согласованы.
2. **Повторный prune:** повторная операция идемпотентна и не дублирует audit
   surface.
3. **Многоуровневое shadowing:** S1 → S2 разрешается до исходных A/B/C без
   цикла и без потери parent edge.
4. **Cap:** task с payload больше 8 MiB получает bounded набор
   `metadata_hash_only` rows; model-visible summary остаётся проверяемым,
   original full text не восстанавливается.
5. **Redaction/retention:** 05.8 атомарно обрабатывает shadow rows,
   `model_request_sources`, envelope и ledger; ambient hash не остаётся,
   memory digest остаётся только по разрешённому правилу.

## Критерии готовности

1. Каждый summary разрешается через `source_refs` в свои originals; для
   многоуровневого summary разрешение транзитивно и детерминировано.
2. Pruning и shadowing не удаляют evidence из audit surface до explicit
   redaction/retention transition.
3. `ContextProjection` строго соответствует 05.1, попадает в
   `ModelRequestEnvelopeV1`, содержит связанные `ledger_id`,
   `context_ledger_hash` и `context_projection_hash`, а второго независимого
   списка контекста нет.
4. `context_projection_hash` детерминированно связан с
   `context_ledger_hash` точной domain-separated формулой 05.1.
5. Превышение `MAX_CONTEXT_PROJECTION_BYTES` даёт типизированную ошибку.
6. `MAX_SHADOW_BYTES_PER_TASK` равен 8 MiB, считается в байтах уникальных
   captured blocks, имеет детерминированный compaction и протестирован до 05.8.
7. Состояния `full`, `metadata_hash_only`, `redacted` и `retention_pruned`
   различимы; `REQUEST_SOURCE_MISSING` не используется для намеренно
   сжатого, redacted или retention-pruned shadow.
