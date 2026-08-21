# План 05.2 — Durable storage

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — SQLite migration и repository layer для committed envelope: immutable запись, связь с существующим `context_ledger`, content-addressed хранение model-visible блоков и индексы для provenance-запросов. Интеграции с dispatch на этом этапе ещё нет.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — без контракта нечего хранить;
- существующая SQLite persistence и общая миграция базы;
- существующий `context_ledger` (`crates/evohime-local-storage/src/context_ledger_store.rs`).

### Опциональные

- [05.8](05-8-redaction-and-retention.md) — правила удаления и retention. До её завершения хранилище работает в **hash-only режиме**: сохраняются метаданные, хеши блоков и linkage, но captured model-visible текст в базу не пишется. Реконструкция в этом режиме недоступна, verifier сообщает `REQUEST_RETENTION_PRUNED`, а гарантия удаления не нарушается. Полное хранение payload включается тем же этапом 05.8, который вводит правила его стирания. Это ограничение обязательно: включать хранение текста раньше, чем существует его удаление, запрещено.

## Logical layout

```text
model_requests
- request_id PK
- logical_request_id
- attempt
- parent_request_id
- request_kind
- ledger_id (FK на context_ledger)
- provider
- model
- envelope_version
- envelope_hash
- envelope_blob / immutable artifact ref
- context_projection_hash
- route_snapshot_hash
- policy_snapshot_hash
- status
- dispatch_at
- completed_at
```

```text
model_request_sources
- request_id
- ordinal
- source_kind
- source_id
- source_version
- source_hash            -- тумбстоунится вместе с источником, см. 05.8
```

Content-addressed хранение блоков:

```text
model_request_blocks
- content_hash PK
- byte_len
- bytes
- refcount / last_referenced_at

model_request_block_refs
- request_id
- ordinal
- role (system_prompt | message | tool_schema)
- content_hash
```

`envelope_blob` в этом варианте хранит логическую структуру со ссылками на `content_hash`, а не сам текст. Canonical bytes считаются по развёрнутой логической схеме, чтобы hash не зависел от того, дедуплицирован блок или нет.

### Зачем дедупликация

Хранить полный payload messages на каждый attempt нельзя: контекст следующего шага почти целиком повторяет предыдущий, и у задачи из сотни шагов рост локальной SQLite квадратичный. Time-based retention это не лечит, потому что дублирование возникает внутри одной живой задачи. Повторное включение того же system prompt, того же сообщения или той же tool schema в следующий request не создаёт новую копию.

Ссылка живого envelope удерживает блок от вытеснения — это и есть решение проблемы «artifact нельзя тихо вытеснить» из [05.1](05-1-canonical-request-contract.md). Существующий artifact store с TTL-вытеснением для этой роли не используется.

### Связь с `context_ledger`

`ledger_id` обязателен и ссылается на `context_ledger.id`, а не на `model_call_id`: последний не уникален (см. `replan_of`). Отношение направленное — у envelope ровно один `ledger_id`, у одной записи ledger может быть несколько envelope, по одному на attempt: retry и fallback контекст не пересобирают и новой записи ledger не создают.

`provider` и `model` продублированы из ledger намеренно: они входят в
подписанный request receipt, и offline-верификатор обязан читать их без ledger.
При fallback значения расходятся — ledger хранит provider/model на момент
планирования контекста, envelope — фактические; authoritative значение в
envelope. Остальные поля ledger (`run_id`, `task_id`, `step_id`, `created_at`)
не дублируются.

`status` и `dispatch_at` — lifecycle/audit metadata. Они не входят в canonical
envelope bytes и могут обновляться по правилам 05.8/интеграции без изменения
`envelope_hash`; mutation committed payload по-прежнему запрещена.

Полный разбор того, что уже есть в ledger и как на него ложится `ContextProjection`, — в разделе «Что есть в коде сейчас» обзора плана.

## Индексы

```text
request -> sources
source -> requests
logical_request -> attempts
ledger -> request
content_hash -> requests
```

Запросы по `run_id`/`task_id`/`step_id` выполняются через индекс и join на
`context_ledger`; прямых дублирующих колонок в `model_requests` нет.

## Immutability

Committed envelope immutable, с единственным исключением — переход в `redacted`/`retention_pruned` по [05.8](05-8-redaction-and-retention.md). Terminal status может обновляться отдельно и не меняет canonical request payload/hash.

## Тесты

### Unit

- запись и чтение envelope через repository API;
- отказ повторной записи того же `request_id`;
- отказ мутации committed payload;
- дедупликация: сто последовательных запросов с почти одинаковым контекстом не дают линейного дублирования блоков;
- refcount не позволяет вытеснить блок, на который ссылается живой envelope;
- каждый индекс действительно используется соответствующим запросом.

### Integration

1. **Ledger parity:** у каждого записанного envelope ровно одна запись `context_ledger`; fallback внутри одного model call даёт два envelope на одну запись ledger, и это не считается нарушением.
2. **Hash-only режим:** при выключенном хранении payload запись содержит метаданные и хеши, но не текст.

## Критерии готовности

1. Миграция применяется на существующей базе без потери данных.
2. Committed envelope нельзя изменить средствами repository API.
3. Дедупликация подтверждена тестом на росте хранилища.
4. Связь `ledger_id` обязательна для каждого envelope и проверяется тестом, включая случай нескольких attempt на одну запись ledger.
