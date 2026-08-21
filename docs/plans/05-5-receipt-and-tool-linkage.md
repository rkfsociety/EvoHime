# План 05.5 — Signed request receipt и tool linkage

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — связать каждый dispatchable model request с существующей signed
hash-chain receipts и довести проверяемую цепочку до эффекта:

```text
request envelope → request receipt → assistant response → tool intent
    → approval → pre-action receipt → effect → terminal receipt
```

Receipt не является вторым хранилищем model-visible текста. Он подписывает
ограниченный набор идентификаторов и digest-ов, а реконструкция request и
проверка его содержимого остаются обязанностью provenance repository.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — canonical envelope,
  `envelope_hash` и bounded digest;
- [05.2](05-2-durable-storage.md) — `model_requests`, committed envelope и
  `context_ledger_receipts`;
- [05.3](05-3-request-integration.md) — единственный checkpoint и terminal
  status;
- существующие Signed hash-chain receipts (`crates/evohime-receipts`) и
  `contracts/receipts/v1/`.

### Опциональные

- [05.4](05-4-evidence-provenance.md) — `source_hash` references. До него
  request receipt всё равно подписывает `context_projection_hash`, а verifier
  явно сообщает `evidence_unavailable`, вместо того чтобы изображать полную
  offline-реконструкцию источников;
- [05.8](05-8-redaction-and-retention.md) — redaction и retention. До него
  receipt immutable уже при commit, но переходы `redacted` и
  `retention_pruned` ещё не создаются;
- [05.9](05-9-verify-and-export.md) — экспорт замкнутой выборки. Локальный
  verifier и его проверки делаются в этом этапе, а упаковка для offline
  verifier подключается позднее.

## Расширение существующего receipt contract

Новый отдельный contract или отдельная hash-chain не создаются. Расширяется
`evohime-receipts`: сохраняются его Ed25519 envelope, canonical JSON, hash
алгоритм, `receipt_records`, `receipt_chain_heads`, key lifecycle,
checkpoint и retention.

Добавляется versioned payload variant `model_request/request_commit` в том же
contract family. Старые effect receipts v1 продолжают валидироваться без
переписывания. Для нового variant в canonical payload обязательны:

```text
receipt_version       = 1 -- существующая версия receipt envelope
payload_version       = 1 -- версия model-request payload variant
receipt_domain        = "model_request"
receipt_type          = "request_commit"
receipt_id            -- UUIDv7 receipt
request_id
logical_request_id
attempt
ledger_id
provider
model
request_envelope_hash
context_projection_hash
route_snapshot_hash
policy_snapshot_hash
previous_receipt_hash
```

`request_envelope_hash` — это ровно lowercase hex `envelope_hash` из 05.1 и
05.2, а не hash `envelope_blob`, не `context_ledger_hash` и не новый digest.
`receipt_version`, `payload_version` и `ledger_id` подписываются внутри
canonical bytes. В receipt не кладутся prompt, message, tool schema, response
text, credentials или raw tool arguments.

Изменение схемы должно быть сделано внутри `evohime-receipts` и
`contracts/receipts/v1/` с сохранением старых vectors. Storage migration
аддитивно добавляет в `receipt_records` domain/type metadata либо эквивалентно
нормализует их из canonical payload; существующие effect rows не меняются.
Verifier принимает legacy effect v1 и новый request variant, но отвергает
неизвестную версию, domain/type или лишние поля.

### Chain semantics

`previous_receipt_hash` — hash предыдущего receipt в существующей цепочке,
не предыдущего request и не предыдущего receipt того же `ledger_id`.
Конкретно новый receipt получает текущий `receipt_chain_heads.receipt_hash`
для signing `key_id`; чтение head и вставка новой строки сериализуются той
же SQLite-транзакцией. Если для этого key/chain ещё нет receipt, значение
`NULL` разрешено ровно для первого receipt. После key rotation применяется
существующий genesis/transition contract.

Receipt request и effect используют одну существующую chain, поэтому request
receipt не создаёт отдельной ветки. Retention не сбрасывает head в `NULL`:
последний сохранённый receipt и checkpoint продолжат доказывать границу
удалённого префикса, а новый receipt ссылается на прежний head.

## Атомарный request commit

`ModelRequestCheckpoint` из 05.3 должен получить одну операцию
`commit_envelope_with_request_receipt`. На одном connection выполняется
`BEGIN IMMEDIATE` с bounded busy timeout и только после успешной валидации
полного dispatchable envelope. В этом плане «committed envelope» означает
именно такой `FullForDispatch` request; `HashOnlyStorage` fixture/migration
строка из 05.2 не является dispatchable envelope и не получает request receipt.

1. вставляются `model_requests`, sources, block refs и refcount по правилам
   `commit_envelope(FullForDispatch)` из 05.2;
2. проверяются request lineage, `envelope_hash`, `ledger_id`, route/policy
   hashes и отсутствие существующего request receipt;
3. читается chain head, строятся canonical bytes request receipt,
   вычисляется receipt hash и выполняется signing;
4. receipt вставляется в существующий `receipt_records`, обновляется
   `receipt_chain_heads`, а ссылка регистрируется в
   `context_ledger_receipts`;
5. выполняются invariant checks и делается один `COMMIT`.

Provider dispatch начинается только после этого commit. Ошибка signing,
chain conflict, duplicate/conflicting request или любой сбой SQLite делает
`ROLLBACK` всех provenance rows, receipt rows, head update и refcount; provider
не вызывается.

`context_ledger_receipts` расширяется nullable-полями для request linkage:

```text
ledger_id
receipt_id
request_id NULL
request_envelope_hash NULL
receipt_domain
exported
PRIMARY KEY (ledger_id, receipt_id)
UNIQUE request_id WHERE request_id IS NOT NULL
```

Ссылка с `request_id` обязательна для `receipt_domain = model_request`; при
её вставке проверяется FK на `model_requests` и равенство
`request_envelope_hash = model_requests.envelope_hash`. Устаревшие generic
ledger receipts сохраняют прежний nullable-путь. Повтор checkpoint с тем же
`request_id` идемпотентно возвращает уже committed receipt только при полном
совпадении canonical request и digest-ов; другая payload даёт conflict и не
создаёт второй receipt. Так ровно один committed envelope получает ровно один
request receipt.

Hash-only запись не может получить request receipt: 05.3 обязан остановить её
до этой операции с `REQUEST_PROVENANCE_COMMIT_FAILED`.

## Authoritative assistant response

Ввести Core-owned таблицу `model_responses`; response не хранится только в
event journal или UI. Минимальная bounded-модель:

```text
model_responses
- response_id PK
- request_id NOT NULL FK model_requests(request_id)
- provider/model response metadata
- output или content-addressed output reference
- output_hash NULL when status is `redacted`/`retention_pruned`
- usage_json bounded
- finish_reason
- status (complete | interrupted | failed | redacted | retention_pruned)
- started_at
- completed_at NULL
- UNIQUE(request_id)
```

`request_id` — именно attempt, который породил этот response, а не только
`logical_request_id`, task id или последний request в ledger. Response
создаётся после dispatch в Core и immutable после commit; повторная запись
проверяет тот же digest и не создаёт дубль.

`redacted` и `retention_pruned` — разрешённые lifecycle-переходы 05.8:
исходный outcome не переписывается в `complete`, а output становится
недоступен через typed tombstone.

При cancellation, crash или оборванном stream доступный partial output
сохраняется как `status = interrupted` с bounded output/hash и не считается
`complete`. Если provider не дал output, фиксируется `failed` с redacted
metadata/error code, без raw error text. Response table не меняет signed
request receipt.

## Response → tool intent

Ввести отдельную таблицу `tool_intents`; не зашивать linkage в неограниченный
JSON ответа:

```text
tool_intents
- intent_id PK
- origin_request_id NOT NULL FK model_requests(request_id)
- origin_request_envelope_hash NOT NULL
- response_id NULL FK model_responses(response_id)
- ordinal NOT NULL
- origin_kind (assistant_response | system | recovery)
- tool_name
- tool_args_hash
- state
- UNIQUE(response_id, ordinal) для response-origin intents
```

Каждый intent обязан иметь одновременно `origin_request_id` и
`origin_request_envelope_hash`. Core при создании и перед receipt path
проверяет, что request существует, его immutable `envelope_hash` совпадает,
`response_id` принадлежит тому же request, ordinal находится в bounded
диапазоне, а `tool_args_hash` соответствует нормализованным аргументам.

Обычно `response_id` обязателен и `origin_kind=assistant_response`.
`system`/`recovery` intent может иметь `response_id = NULL`, но всё равно
обязан ссылаться на конкретный request attempt и пройти отдельную policy
проверку. Несколько intents из одного response разрешены по ordinal; intent
без response не становится «ничейным».

Существующий `receipt_actions`/approval path получает `intent_id`,
`origin_request_id` и `origin_request_envelope_hash` как immutable linkage.
Эти поля также входят в canonical payload effect receipt (pre, post или
refusal) либо в его signed domain-specific projection. Поэтому verifier не
доверяет одной SQLite-связи: он сверяет intent → action → approval → signed
receipt и оба origin поля.

## Порядок tool effect

Для каждого approved intent:

```text
authoritative response commit
    ↓
intent commit + bounded origin validation
    ↓
approval grant/claim с exact call_hash
    ↓
pre-action receipt в существующей chain
    ↓
dispatch tool
    ↓
post-action receipt (succeeded | failed | cancelled)
```

Refusal (`policy_denied`, `approval_denied`, `approval_expired`,
`approval_stale`, `call_changed`) получает signed refusal receipt и не
достигает tool dispatch. Pre receipt создаётся до dispatch и не может быть
создан повторно для того же `action_id`; terminal receipt идемпотентен по
существующему action contract. Approval, pre receipt и action state не должны
отрываться друг от друга при rollback.

Это не меняет правило 05.3: request receipt создаётся до model dispatch,
tool pre receipt — до tool dispatch, terminal receipt — только после
наблюдаемого tool outcome. После crash между pre receipt и effect recovery
сохраняет `pending_recovery/unknown`, автоматический blind retry запрещён.

## Redaction и retention

Request receipt и effect receipts immutable: redaction не переписывает
canonical envelope, receipt payload, `receipt_hash`, `previous_receipt_hash`
или chain head. 05.8 может удалить/заменить model-visible blocks, sources и
response output, а также перевести связанные `tool_intents` в
`redacted`/`retention_pruned`; metadata, разрешённые digest и request/tool
linkage остаются. Эти lifecycle-переходы выполняются одной транзакцией с
`model_requests.status`, refs и source hashes; receipts по-прежнему не
переписываются.

После redaction verifier:

- проверяет подпись, canonical bytes, receipt hash и chain как обычно;
- сверяет `request_envelope_hash` с сохранённым immutable hash строки
  `model_requests`;
- возвращает валидный результат со статусом `redacted`, не требуя
  реконструкции удалённого текста;
- отличает `retention_pruned` от повреждения и `REQUEST_HASH_MISMATCH`.

Таким образом receipt остаётся проверяемым после redaction, но не становится
способом восстановить удалённый текст. Тест 05.5 фиксирует это до реализации
полного retention-пути 05.8.

## Request receipt verifier

Verifier обязан сверить не только Ed25519 signature и chain:

1. canonical payload variant/domain/type/version и отсутствие неизвестных полей;
2. `previous_receipt_hash` с предыдущим receipt/head или checkpoint boundary;
3. `request_id`, `logical_request_id`, `attempt`, `ledger_id`, provider и
   model со строкой `model_requests`;
4. `request_envelope_hash` с `model_requests.envelope_hash`;
5. `context_projection_hash`, `route_snapshot_hash` и `policy_snapshot_hash`
   с committed request;
6. статус request: `complete`/`active` реконструируется обычным путём,
   `interrupted`, `redacted` и `retention_pruned` возвращаются как явные
   наблюдаемые состояния, а не как успешная complete-реконструкция;
7. для tool receipt — intent id, origin request id/hash, action id, approval
   id/call_hash, args hash и terminal result hash.

Несовпадение любого immutable поля — `REQUEST_RECEIPT_LINKAGE_MISMATCH`;
повреждение canonical envelope/block — `REQUEST_HASH_MISMATCH`; redaction и
retention не маскируются под эти ошибки.

## Тесты

### Unit

- canonical request receipt bounded digest детерминирован и покрыт vectors;
- request payload содержит `receipt_version`, `payload_version`,
  `request_envelope_hash`, `ledger_id` и
  все route/policy/projection hashes;
- первый receipt имеет `previous_receipt_hash = NULL`, следующий получает
  именно текущий общий chain head, а не предыдущий request;
- request receipt отвергается при несовпадении envelope, ledger, provider,
  model или projection hash;
- duplicate request receipt идемпотентен только при byte-for-byte равном
  canonical payload;
- tool intent без `origin_request_id` или с неверным
  `origin_request_envelope_hash` не создаётся;
- multiple intents одного response получают bounded ordinals;
- interrupted response не считается complete и сохраняет request linkage;
- signed receipt verifier принимает redacted request по immutable hash, но не
  выдаёт удалённый текст;
- unknown receipt domain/type/version и chain previous mismatch отвергаются.

### Integration

1. **Atomic request commit:** отказ после вставки envelope, после signing или
   после `context_ledger_receipts` оставляет все request/receipt/refcount/head
   rows в исходном состоянии; provider не вызывается.
2. **Exactly once:** retry checkpoint с тем же request id не растит
   `context_ledger_receipts` и receipt chain; изменённый request получает
   conflict.
3. **Request chain:** request receipt и существующий effect receipt идут в
   одной chain и проходят совместную проверку, включая retention checkpoint.
4. **Response:** complete и interrupted stream сохраняют authoritative
   response с `request_id` соответствующей attempt; interrupted не становится
   terminal complete.
5. **Tool linkage:** request → response → один или несколько intents → exact
   approval → pre receipt → tool → terminal receipt; verifier проверяет оба
   origin поля.
6. **Non-response intent:** system/recovery intent имеет bounded linkage и
   проходит policy/receipt path; intent без linkage блокируется.
7. **Redaction:** после удаления model-visible block/source receipt signature,
   hash-chain и request linkage остаются валидными, verifier сообщает
   `redacted`, а текст не возвращается.
8. **Concurrent append:** два commit-а не получают один `previous` и не
   создают fork; bounded busy retry повторяет всю транзакцию.

## Критерии готовности

1. Каждый dispatchable committed envelope имеет ровно один signed
   `model_request/request_commit` receipt в `context_ledger_receipts`.
2. Receipt создаётся до provider dispatch в той же транзакции, что и envelope;
   при rollback нет частичной provenance/receipt записи.
3. Каждый tool intent содержит `origin_request_id` и
   `origin_request_envelope_hash`; tool receipt path и verifier проверяют оба.
4. Существующая signed chain остаётся валидной; `previous_receipt_hash` — hash
   предыдущего receipt общей chain, а retention не сбрасывает её head.
5. Authoritative response хранит request id конкретной attempt; partial stream
   после cancellation/crash имеет `interrupted` и не считается complete.
6. Redaction не изменяет request receipt; verifier успешно проверяет receipt и
   различает `redacted`, `retention_pruned` и повреждение.
7. Multiple response-origin intents и bounded system/recovery intents имеют
   однозначную provenance-связь.
