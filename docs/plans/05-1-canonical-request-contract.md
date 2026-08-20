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

Минимальные поля:

```text
version
request_id
logical_request_id
attempt
parent_request_id?
run_id
task_id
step_id
request_kind
created_at
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
- использовать UUIDv7 либо существующий сортируемый идентификатор EvoHime;
- каждый фактический provider dispatch attempt получает новый `request_id`;
- retry/fallback сохраняет общий `logical_request_id`;
- повторная попытка связывается через `parent_request_id` и `attempt`.

Пример:

```text
logical_request_id = A

R1: provider=local, attempt=1
R2: provider=remote, attempt=2, parent_request_id=R1
```

Нельзя переиспользовать один envelope для двух фактических dispatch.

`logical_request_id` соответствует существующему `model_call_id`; его сегодняшний вид и ограничения описаны в разделе «Что есть в коде сейчас» обзора плана.

### Canonicalization

Контракт должен иметь:

- deterministic canonical bytes;
- bounded sizes;
- stable validation/error codes;
- known-answer vectors;
- hash, пригодный для linkage с receipt chain.

Использовать тот же строгий подход, что Canonical Receipt v1. Не копировать receipt schema механически, если request contract имеет другой domain.

Canonical bytes считаются по логической схеме и не зависят от физического layout хранилища: дедуплицирован блок или лежит копией, hash обязан совпадать.

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
- artifact нельзя тихо **вытеснить**;
- verifier способен проверить hash.

Последнее условие исключает прямое переиспользование существующего artifact store из [`../architecture.md`](../architecture.md): он вытесняет содержимое по TTL и последнему обращению, оставляя tombstone. Ссылка envelope на такой артефакт однажды перестанет реконструироваться, и это будет не `redacted`, а тихая потеря. Физическое решение выбирается на этапе [05.2](05-2-durable-storage.md); контракт лишь запрещает ссылку, которую можно тихо потерять.

Reconstruction исторического request не должна читать текущее состояние workspace, memory либо child task как замену историческому snapshot.

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

При превышении request не dispatchится. Возвращается typed Core error и bounded audit event. Нельзя «обрезать JSON» после assembly.

Из этого следует обязательное требование к Context Budget Manager: лимиты envelope — его **вход**, а не проверка после факта. Планировщик получает `MAX_REQUEST_ENVELOPE_BYTES`, `MAX_CONTEXT_PROJECTION_BYTES` и `MAX_TOOL_SET_BYTES` наравне с token budget и планирует под них. Иначе легитимно большой контекст просто заблокирует агента: обрезать после assembly нельзя, а собрать заново уже нечем. Проверка перед commit остаётся, но как backstop на ошибку планировщика, а не как штатный путь отказа. Передача лимитов в планировщик выполняется на этапе [05.3](05-3-request-integration.md); здесь фиксируются сами значения и требование.

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

## Тесты

### Unit

- canonical serialization;
- stable hashes;
- source ordering;
- tool schema ordering;
- size limits;
- unknown version rejection;
- retry lineage (`logical_request_id`, `attempt`, `parent_request_id`);
- независимость canonical bytes от дедупликации блоков.

### Known-answer vectors

Создать:

```text
contracts/model-request/v1/test-vectors/
```

Проверять canonical bytes и expected hash. Rust vectors обязательны. Cross-language parity нужна только если Electron реально будет проверять этот contract.

## Критерии готовности

1. Схема, лимиты и коды ошибок лежат в `contracts/model-request/v1/`.
2. Canonical bytes детерминированы и покрыты векторами.
3. Неизвестная версия отвергается типизированной ошибкой, а не игнорируется.
4. Ни одно поле контракта не требует чтения текущего состояния workspace/memory для реконструкции.
