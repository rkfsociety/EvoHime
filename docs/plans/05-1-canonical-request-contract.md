# План 05.1 — Canonical request contract

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — описать `ModelRequestEnvelopeV1` как versioned contract с детерминированными canonical bytes, границами размеров, стабильными кодами ошибок и known-answer vectors. Код рантайма на этом этапе не меняется: этап даёт контракт, на который опираются все остальные.

## Зависимости

### Блокирующие

- существующий Canonical Receipt v1 (`contracts/receipts/v1/`) как образец строгости и формата лимитов.

### Опциональные

Нет.

## Canonical request contract

Создать versioned contract:

```text
contracts/model-request/v1/
```

Определить canonical logical schema `ModelRequestEnvelopeV1`.

Каноническая логическая схема `ModelRequestEnvelopeV1` содержит только данные,
которые нужны для воспроизведения фактического dispatch. Контекстная запись
`context_ledger` остаётся единственным владельцем run/task/step и времени
создания контекста; envelope ссылается на неё через обязательный `ledger_id`.

Минимальные поля canonical payload:

```text
version
request_id
logical_request_id
attempt
parent_request_id?
ledger_id
request_kind
provider
model
route_snapshot_hash
policy_snapshot_hash
system_prompt
messages
tools
model_parameters
context_projection
previous_request_hash?
```

`status` — обязательное lifecycle-поле committed request (`active`, `redacted`
или `retention_pruned`), но это не model-visible payload и оно не входит в
canonical bytes. Переход в terminal status не меняет envelope или его hash.
`dispatch_at`, если он нужен для аудита, хранится в записи dispatch/ledger, а
не в canonical envelope.

`ledger_id` — идентификатор строки `context_ledger`, а не
`logical_request_id`/`model_call_id`. Для каждого envelope он обязателен, одна
запись ledger может иметь несколько envelope только для retry/fallback без
пересборки контекста, а replan создаёт новую запись ledger и новый
`logical_request_id`.

Допустимые `request_kind` как минимум:

```text
agent
plan_review
plan_revision
memory
child
scheduled
ambient
internal_summary
```

Новые значения добавляются аддитивно.

### Request identity

- `request_id` создаёт Core;
- использовать UUIDv7 либо существующий сортируемый идентификатор EvoHime,
  уникальный для каждого фактического dispatch;
- каждый фактический provider dispatch attempt получает новый `request_id`;
- retry/fallback сохраняет общий `logical_request_id` и тот же `ledger_id`, если
  контекст не пересобирался;
- при `attempt > 1` `parent_request_id` обязателен и должен указывать на
  предыдущий committed attempt с тем же `logical_request_id` и `ledger_id`;
- `logical_request_id` уникален для логической операции и соответствует
  существующему `model_call_id`; replan начинает новую логическую операцию с
  новым `logical_request_id`, даже если исходная задача та же.

Пример:

```text
logical_request_id = A

R1: provider=local, attempt=1
R2: provider=remote, attempt=2, parent_request_id=R1
```

Нельзя переиспользовать один envelope для двух фактических dispatch.

`previous_request_hash` отсутствует у первой попытки. У последующей попытки он
обязателен и равен hash непосредственно предыдущего envelope в той же линии;
отсутствующий или неразрешимый predecessor блокирует dispatch.

`logical_request_id` соответствует существующему `model_call_id`; его текущий
вид и ограничения описаны в разделе «Что есть в коде сейчас» обзора плана.

### Canonicalization

Контракт должен иметь:

- deterministic canonical bytes;
- bounded sizes;
- stable validation/error codes;
- known-answer vectors;
- hash, пригодный для linkage с receipt chain.

Использовать RFC 8785 JCS ровно в соответствии с Canonical Receipt v1:
UTF-8 без BOM и с завершающим LF, сортировка ключей по UTF-16 code units,
без Unicode normalization, с отклонением duplicate keys и malformed input.
Не копировать receipt schema механически, но использовать тот же алгоритм,
правила bytes и SHA-256 domain separation, зафиксированные отдельным
`version-manifest.json` для request contract.

Правила логической схемы до JCS:

- object keys сортируются самим JCS;
- `messages`, content blocks и `context_projection.entries` сохраняют
  фактический model-visible порядок;
- `tools` перед сборкой envelope получают уникальные имена и сортируются по
  имени; schema properties канонизируются рекурсивно JCS;
- `source_refs` сортируются по `(source_kind, source_id, source_version)`;
- массивы, для которых порядок является частью model-visible semantics, не
  сортируются канонизатором постфактум — envelope обязан содержать уже
  фактический порядок dispatch.

Canonical bytes считаются по развёрнутой логической схеме и содержимому
блоков, а не по physical artifact references. Дедуплицированный блок или копия
того же содержимого должны давать одинаковые bytes и hash.

`envelope_hash` равен
`lowercase_hex(SHA-256("evohime-model-request-v1\\0" || canonical_bytes))`.
Это domain-separated hash, который входит в linkage signed request receipt;
точные bytes и expected hash обязательны в known-answer vectors.

`context_projection` — расширение ровно того же ledger, а не второй список
контекста. Он содержит ссылки на `selected_items[].id`, `compression[].summary_id`
и `source_ids`, а pruning — на `dropped_items[].drop_reason`. Обязательные
поля projection:

```text
context_projection
- ledger_id                 -- равен верхнеуровневому ledger_id
- context_ledger_hash       -- hash этой записи context_ledger
- entries[]                 -- фактический model-visible порядок
- context_projection_hash
```

`context_projection_hash` вычисляется детерминированно как
`SHA-256("evohime-context-projection-v1\\0" || context_ledger_hash_bytes ||
JCS(projection_content_coverage))`. Поэтому он всегда связан с
`context_ledger_hash`; второй независимый hash или независимый список
выбранных item запрещён. Изменение ledger hash или любого model-visible
content обязано изменить projection hash.

## Exact request reconstruction

Hash сам по себе не удовлетворяет reconstructability.

Для request должны быть durably доступны фактические:

```text
system prompt
messages/content blocks
tool schemas
effective model parameters
```

Допускается хранить не payload, а immutable artifact reference только если:

- artifact Core-owned;
- reference content-addressed;
- artifact нельзя тихо заменить;
- artifact нельзя тихо **вытеснить** до redaction/retention transition;
- verifier способен проверить hash.

Последнее условие исключает прямое переиспользование существующего artifact store из [`../architecture.md`](../architecture.md): он вытесняет содержимое по TTL и последнему обращению, оставляя tombstone. Ссылка envelope на такой артефакт однажды перестанет реконструироваться, и это будет не `redacted`, а тихая потеря. Физическое решение выбирается на этапе [05.2](05-2-durable-storage.md); контракт лишь запрещает ссылку, которую можно тихо потерять.

Reconstruction исторического request не должна читать текущее состояние
workspace, memory либо child task как замену историческому snapshot.

Если какой-либо required artifact недоступен, изменён или не проходит проверку
hash, request не считается reconstructable и dispatch блокируется typed error.

## Tool schemas и effective model parameters

Envelope фиксирует именно тот tool set, который увидела модель.

Сохранять:

```text
tool name
description
input schema
```

либо immutable content-addressed canonical schema-set artifact.

Runtime-only поля не должны попадать в model request/envelope:

```text
execute callback
approval implementation
timeout internals
UI presentation metadata
```

`request_kind` имеет ограниченные границы provenance:

- `agent` — обычный agent run;
- `plan_review` / `plan_revision` — только с immutable ссылкой на reviewed или
  revised plan;
- `memory` — только с memory extraction/recall provenance;
- `child` — только с child-task/report provenance;
- `scheduled` — только с scheduler event;
- `ambient` — только из ambient episode/listener pipeline и с episode source
  ref;
- `internal_summary` — только из Core-owned summarizer, с source ref на
  породивший request;
- неизвестное значение отвергается как `REQUEST_PROVENANCE_INVALID`.

Для всех значений кроме `agent` обязательна соответствующая provenance-ссылка
в `context_projection`; renderer или внешний caller не может произвольно
назначить `request_kind`.

Сохранять effective model parameters, а не только requested values:

```text
temperature
top_p
max_output_tokens
reasoning mode
provider-specific supported options
```

Если Core знает provider default, materialize effective value. Если default неизвестен, хранить `unspecified/provider_default_unknown`, а не придумывать значение.

## Size limits

Все структуры bounded.

Определить constants минимум для:

```text
MAX_REQUEST_ENVELOPE_BYTES
MAX_SYSTEM_PROMPT_BYTES
MAX_MESSAGE_BYTES
MAX_TOOL_SCHEMA_BYTES
MAX_TOOL_SET_BYTES
MAX_EVIDENCE_REFS
MAX_SOURCE_REFS_PER_ENTRY
MAX_CONTEXT_PROJECTION_BYTES
```

Все лимиты считаются в байтах UTF-8 после canonical assembly. Лимиты envelope,
projection и tool set передаются Context Budget Manager как входные параметры
планирования вместе с token budget. Проверка непосредственно перед commit
остаётся только backstop; штатный путь не должен собирать заведомо
непомещающийся request и затем обрезать JSON.

При превышении request не dispatchится. Возвращается typed Core error и bounded audit event. Нельзя «обрезать JSON» после assembly.

Передача лимитов в планировщик выполняется на этапе [05.3](05-3-request-integration.md),
а здесь фиксируются сами значения и требование.

Лимиты задаются в том же виде, что `contracts/receipts/v1/limits.json`, и покрываются такими же known-answer vectors.

## Typed errors

Добавить stable Core error codes минимум:

```text
REQUEST_PROVENANCE_TOO_LARGE
REQUEST_PROVENANCE_INVALID
REQUEST_PROVENANCE_COMMIT_FAILED
REQUEST_SOURCE_MISSING
REQUEST_SOURCE_CHANGED
REQUEST_RECONSTRUCTION_FAILED
REQUEST_HASH_MISMATCH
REQUEST_UNSUPPORTED_VERSION
REQUEST_REDACTED
REQUEST_RETENTION_PRUNED
REQUEST_LEDGER_MISMATCH
REQUEST_EVIDENCE_EVICTED
```

На IPC boundary не использовать generic string errors вместо typed contract.
Любая ошибка из этого списка блокирует dispatch (fail closed), а audit event
может содержать только bounded код и безопасную диагностическую projection.

## Тесты

### Unit

- canonical serialization;
- stable hashes;
- exact canonical bytes, включая UTF-8/JCS rules и rejection duplicate keys;
- source ordering;
- tool schema ordering;
- size limits;
- unknown version rejection;
- retry lineage (`logical_request_id`, `attempt`, `parent_request_id`);
- same-ledger retry/fallback и new-ledger replan;
- обязательную связь `context_projection_hash` с `context_ledger_hash`;
- terminal status transitions без изменения canonical hash;
- fail-closed для typed errors;
- независимость canonical bytes от дедупликации блоков и artifact references;
- отказ реконструкции при чтении только текущего workspace/memory state.

### Known-answer vectors

Создать:

```text
contracts/model-request/v1/test-vectors/
```

Проверять canonical bytes и expected hash, включая vectors с разным physical
layout (копия блока против content-addressed ссылки), retry lineage, status
transition и source/tool ordering. Rust vectors обязательны. Cross-language
parity нужна только если Electron реально будет проверять этот contract.

## Критерии готовности

1. Схема, лимиты и коды ошибок лежат в `contracts/model-request/v1/`; в схеме
   есть обязательный `ledger_id`, а run/task/step/created_at не дублируются из
   ledger.
2. `context_projection` явно расширяет тот же ledger, содержит
   `context_projection_hash`, связанный с `context_ledger_hash`, и не создаёт
   второго списка контекста.
3. RFC 8785 JCS, UTF-8 bytes, ordering rules и hash domain зафиксированы;
   canonical bytes и expected hash покрыты known-answer vectors.
4. Неизвестная версия и любая typed provenance/size/reconstruction ошибка
   отвергаются fail-closed.
5. Лимиты envelope/projection/tool set переданы Context Budget Manager как
   входные данные, а post-assembly check остаётся только backstop.
6. `parent_request_id` и `previous_request_hash` обязательны для последующих
   попыток; retry/fallback сохраняют `ledger_id`, replan создаёт новый
   `ledger_id` и `logical_request_id`.
7. Ни одно поле контракта не требует чтения текущего состояния
   workspace/memory/child task для реконструкции.
