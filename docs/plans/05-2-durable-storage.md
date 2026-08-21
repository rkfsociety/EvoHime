# План 05.2 — Durable storage

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — SQLite migration и repository layer для committed envelope:
immutable запись, связь с существующим `context_ledger`, content-addressed
хранение model-visible блоков и индексы для provenance-запросов. Интеграции с
dispatch на этом этапе ещё нет.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — без контракта нечего хранить;
- существующая SQLite persistence и общая миграция базы;
- существующий `context_ledger`
  (`crates/evohime-local-storage/src/context_ledger_store.rs`).

### Опциональные

- [05.8](05-8-redaction-and-retention.md) — правила удаления и retention. До
  её завершения хранилище работает в **hash-only режиме**: сохраняются
  метаданные, хеши блоков и linkage, но captured model-visible текст в базу не
  пишется. Реконструкция в этом режиме недоступна, а verifier/repository
  возвращает `REQUEST_RETENTION_PRUNED`, а не generic storage error. Это
  заранее объявленное исключение из reconstructability-инварианта 05.1;
  полный payload включается тем же этапом 05.8, который вводит его стирание.

## Что уже есть в коде сейчас

- текущая схема SQLite — `v26` (`SCHEMA_VERSION` в
  `crates/evohime-local-storage/src/lib.rs`);
- `context_ledger` уже содержит обязательный `id` и
  `context_ledger_hash`, а запись и чтение выполняются через
  `context_ledger_store.rs`;
- `model_requests`, `model_request_sources`,
  `model_request_blocks` и `model_request_block_refs` пока отсутствуют;
- миграция 05.2 должна быть additive: новая схема получает `v27`, старые
  строки и их `context_ledger_hash` не переписываются;
- для существующих ledger-записей backfill `model_requests` не выполняется:
  без сохранённого canonical envelope нельзя честно придумать
  `request_id`, `envelope_hash`, lineage или source refs. Такие записи остаются
  доступными как ledger, но provenance-реконструкция для них не заявляется.

Миграция выполняется в общей транзакции мигратора, идемпотентна при повторном
открытии базы и проверяется на fixture с существующей ledger-строкой. До
создания новых таблиц включаются/проверяются `foreign_keys`, а ошибки миграции
оставляют `user_version` и данные в прежнем состоянии.

## Logical layout

```text
model_requests
- request_id PK
- logical_request_id
- attempt
- UNIQUE (logical_request_id, attempt)
- parent_request_id NULL
- previous_request_hash NULL
- request_kind
- ledger_id NOT NULL FK на context_ledger(id)
- provider
- model
- envelope_version
- envelope_hash
- envelope_blob NOT NULL
- context_projection_hash NOT NULL -- ровно hash из ModelRequestEnvelopeV1/05.1
- route_snapshot_hash
- policy_snapshot_hash
- status
- dispatch_at NULL
- completed_at NULL
```

`previous_request_hash` обязателен для `attempt > 1`, отсутствует у первой
попытки и равен `envelope_hash` непосредственно предыдущего committed attempt
в той же линии. `parent_request_id` проверяется одновременно: тот же
`logical_request_id`, тот же `ledger_id`, предыдущий `attempt` и совпадающий
`envelope_hash`. Неразрешимый predecessor блокирует запись.

`context_projection_hash` не является новым независимым projection hash: это
поле `context_projection.context_projection_hash` из 05.1. Сам canonical
envelope содержит `context_projection.ledger_id` и
`context_projection.context_ledger_hash`; отдельная колонка
`context_ledger_hash` в `model_requests` не дублируется. Repository проверяет,
что `ledger_id` и `context_ledger_hash` внутри envelope соответствуют строке
`context_ledger`.

`status` имеет ограниченный enum:

```text
active | redacted | retention_pruned |
completed | failed | interrupted | unknown_outcome
```

Новая запись начинается с `active`; terminal outcome выставляется только
переходом, согласованным с 05.3/05.7, и тогда `completed_at` становится
ненулевым. Пока terminal outcome не зафиксирован, `completed_at` остаётся
`NULL`; переход в `redacted` или `retention_pruned` не придумывает и не
переписывает время завершения. `redacted` и `retention_pruned` — финальные
retention-состояния; они не переписывают canonical payload или исходный
`envelope_hash`. Неведомое значение status отвергается как
`REQUEST_PROVENANCE_INVALID`.

```text
model_request_sources
- request_id
- ordinal
- source_kind
- source_id
- source_version
- source_hash NULL             -- тумбстоунится вместе с источником, см. 05.8
- PRIMARY KEY (request_id, ordinal)
```

Content-addressed хранение блоков:

```text
model_request_blocks
- content_hash PK
- byte_len NOT NULL
- bytes NULL                   -- NULL в hash-only режиме
- refcount NOT NULL DEFAULT 0
- last_referenced_at NULL
```

```text
model_request_block_refs
- request_id
- ordinal
- role (system_prompt | message | tool_schema)
- content_hash NOT NULL
- PRIMARY KEY (request_id, ordinal)
- FOREIGN KEY (content_hash) REFERENCES model_request_blocks(content_hash)
```

`refcount` — число строк `model_request_block_refs`, а не число запросов.
`last_referenced_at` обновляется при каждом новом reference, включая повторное
использование уже существующего блока. В hash-only режиме `byte_len` и
`content_hash` всё равно проверяются; `bytes` может быть `NULL`. В полном
режиме вставка или чтение блока обязаны проверить
`SHA-256(domain || bytes) == content_hash` и совпадение `byte_len`.

`envelope_blob` хранит одну versioned JCS-структуру `StoredEnvelopeV1` с
логическими полями canonical envelope и ссылками на блоки по `content_hash`.
В него не кладётся альтернатива вида `immutable artifact ref` и не попадает
текст блока. При записи repository разворачивает ссылки, строит canonical
`ModelRequestEnvelopeV1` и вычисляет:

```text
envelope_hash = lowercase_hex(
  SHA-256("evohime-model-request-v1\\0" || expanded_canonical_bytes)
)
```

Поэтому hash зависит от model-visible содержимого и порядка, но не от
physical layout или дедупликации. При чтении blob разбирается по
`envelope_version`, все ссылки разрешаются, каждый блок проверяется, затем
заново вычисляются `context_projection_hash` и `envelope_hash`. Несовпадение
возвращает `REQUEST_HASH_MISMATCH`; неизвестная версия —
`REQUEST_UNSUPPORTED_VERSION`.

### Транзакционная запись

Repository предоставляет одну операцию `commit_envelope`. Она выполняется на
одном SQLite connection в `BEGIN IMMEDIATE` и включает все следующие шаги:

1. проверить версию/размеры/enum, `ledger_id`, hash ledger, request lineage и
   уникальность `request_id`;
2. для каждого блока выполнить deduplicating insert по `content_hash`, при
   конфликте проверить `byte_len` и содержимое, если оно доступно;
3. вставить `model_requests`, `model_request_sources` и
   `model_request_block_refs`;
4. увеличить `refcount` ровно на число реально вставленных reference-строк и
   выставить для них `last_referenced_at`;
5. проверить локальные инварианты и сделать `COMMIT`.

Любая ошибка, включая отказ несовместимой `envelope_version`, конфликт hash,
пропавший ledger/source или duplicate `request_id`, делает `ROLLBACK` всей
операции. Поэтому частичная запись envelope, sources, refs или refcount
невозможна. `BEGIN IMMEDIATE` получает bounded busy timeout; повторяется только
вся транзакция при временном `SQLITE_BUSY`, а не отдельное обновление
refcount. In-memory счётчик не используется.

При переходе request в `redacted` или `retention_pruned` по 05.8 repository в
той же транзакции удаляет ставшие недействительными block refs, уменьшает
`refcount` на число удалённых строк и пересчитывает `last_referenced_at` по
оставшимся refs. Блоки с `refcount = 0` удаляются физически только после
успешного удаления refs; это не считается мутацией committed envelope.

### Зачем дедупликация

Хранить полный payload messages на каждый attempt нельзя: контекст
следующего шага почти целиком повторяет предыдущий, и у задачи из сотни
шагов рост локальной SQLite квадратичный. Time-based retention это не лечит,
потому что дублирование возникает внутри одной живой задачи. Повторное
включение того же system prompt, того же сообщения или той же tool schema в
следующий request не создаёт новую копию.

Ссылка живого envelope удерживает блок от вытеснения — это и есть решение
проблемы «artifact нельзя тихо вытеснить» из [05.1](05-1-canonical-request-contract.md).
Существующий artifact store с TTL-вытеснением для этой роли не используется.

## Связь с `context_ledger`

`ledger_id` обязателен и ссылается на `context_ledger.id`, а не на
`model_call_id`: последний не уникален (см. `replan_of`). Отношение
направленное — у envelope ровно один `ledger_id`, у одной записи ledger может
быть несколько envelope, по одному на attempt: retry и fallback контекст не
пересобирают и новой записи ledger не создают.

До вставки repository проверяет наличие ledger и равенство его
`context_ledger_hash` значению в `context_projection`. Удаление ledger,
на который ссылается request, запрещается внешним ключом либо сначала
обрабатывается согласованной retention-транзакцией 05.8.

`provider` и `model` продублированы из ledger намеренно: они входят в
подписанный request receipt, и offline-верификатор обязан читать их без
ledger. При fallback значения расходятся — ledger хранит provider/model на
момент планирования контекста, envelope — фактические; authoritative значение
в envelope. Остальные поля ledger (`run_id`, `task_id`, `step_id`, `created_at`)
не дублируются.

## Индексы

Обязательные индексы:

```text
model_request_sources(source_kind, source_id, source_version, request_id)
model_requests(logical_request_id, attempt)
model_requests(parent_request_id)
model_requests(ledger_id, attempt)
model_request_block_refs(content_hash, request_id)
```

Индекс `content_hash -> requests` строится только через
`model_request_block_refs(content_hash)`; прямой индекс блока на
`model_requests` не создаётся. Запросы по `run_id`/`task_id`/`step_id`
выполняются через индекс и join на `context_ledger`; прямых дублирующих
колонок в `model_requests` нет.

## Immutability и hash-only

После commit нельзя менять `envelope_blob`, `envelope_hash`, lineage,
`ledger_id`, sources или block content средствами repository API. Допустимы
только lifecycle/retention-переходы, явно перечисленные выше и в 05.8.

До 05.8 отсутствие `bytes` в hash-only режиме является намеренным состоянием,
а не повреждением. Попытка реконструировать такой request возвращает
`REQUEST_RETENTION_PRUNED`; generic `REQUEST_RECONSTRUCTION_FAILED` используется
только для повреждённой структуры, отсутствующей ссылки при заявленном полном
режиме или иной ошибки реконструкции. После 05.8 отсутствие обязательного
полного блока при статусе, допускающем реконструкцию, — это уже
`REQUEST_HASH_MISMATCH`/corruption, а не допустимая деградация.

## Тесты

### Unit

- запись и чтение envelope через repository API;
- отказ повторной записи того же `request_id` без остаточных rows или
  изменения refcount;
- отказ мутации committed payload и несовместимой `envelope_version`;
- проверка `previous_request_hash`, parent и нескольких attempt на один ledger;
- проверка `context_projection_hash`/`context_ledger_hash` и обязательной
  связи с существующим ledger;
- проверка `envelope_hash` при чтении, включая изменение blob, блока, порядка
  refs и неверный `byte_len`;
- дедупликация: сто последовательных запросов с почти одинаковым контекстом
  дают одну физическую копию каждого повторяемого блока;
- повторное добавление существующего блока не создаёт новую строку, но
  увеличивает refcount ровно на число новых refs и обновляет
  `last_referenced_at`;
- отказ в середине `commit_envelope` откатывает model request, sources, refs,
  blocks и refcount целиком;
- конкурентные commits не теряют increment и не оставляют refcount меньше
  количества refs;
- hash-only: metadata/hash сохраняются, `bytes` отсутствуют, реконструкция
  возвращает `REQUEST_RETENTION_PRUNED`;
- каждый индекс используется соответствующим provenance-запросом.

### Migration и integration

1. **Migration:** база v26 с существующими ledger-строками переходит на v27
   без потери строк, hash и пользовательских данных; повторный запуск
   миграции не создаёт дублей.
2. **Ledger parity:** у каждого записанного envelope ровно одна запись
   `context_ledger`; fallback внутри одного model call даёт два envelope на
   одну запись ledger и не считается нарушением.
3. **Atomicity:** искусственный отказ после вставки refs или после первого
   refcount update оставляет все четыре таблицы в исходном состоянии.
4. **Read verification:** изменение `envelope_blob` или блока обнаруживается
   до выдачи реконструкции и не маскируется как hash-only.

## Критерии готовности

1. Additive-миграция v26 -> v27 применяется на существующей базе без потери
   данных и без синтетического backfill старых ledger.
2. Committed envelope нельзя изменить средствами repository API.
3. Дедупликация подтверждена тестом на физическом росте хранилища.
4. Связь `ledger_id` обязательна и проверяется, включая несколько attempt на
   одну запись ledger.
5. Запись envelope, sources, refs и обновление refcount выполняются атомарно
   в `BEGIN IMMEDIATE`; тест проверяет rollback при отказе в середине.
6. При чтении проверяется соответствие `envelope_hash` развёрнутому
   содержимому и `context_projection_hash` соответствующему контракту 05.1.
7. В hash-only режиме при отсутствии payload-блока возвращается
   `REQUEST_RETENTION_PRUNED`.
8. При redaction/retention 05.8 тумбстоунит `source_hash`, удаляет ставшие
   недействительными block refs и уменьшает refcount в одной транзакции.
