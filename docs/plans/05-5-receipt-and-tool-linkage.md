# План 05.5 — Signed request receipt и tool linkage

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — связать envelope с существующей signed hash-chain receipts и довести цепочку до эффекта: request → response → tool intent → approval → receipt → effect.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — `envelope_hash` и bounded digest;
- [05.2](05-2-durable-storage.md) — committed envelope и `context_ledger_receipts`;
- [05.3](05-3-request-integration.md) — чокпойнт и terminal status;
- существующие Signed hash-chain receipts (`crates/evohime-receipts`) и `contracts/receipts/v1/`.

### Опциональные

- [05.4](05-4-evidence-provenance.md) — evidence provenance. Без неё подписанный receipt покрывает envelope и projection hash, но `source hash references` в нём отсутствуют, и offline-проверка источников недоступна: verifier сообщает, что раздел evidence пуст, а не что он повреждён.

## Signed model-request receipt

Связать `request_envelope_hash` с существующим `evohime-receipts`.

Добавить receipt domain/type для model request либо отдельный строго разделённый request receipt contract, если существующий runtime receipt contract семантически предназначен только для effects.

Signed payload должен содержать минимум:

```text
request_id
logical_request_id
attempt
provider
model
request_envelope_hash
context_projection_hash
route_snapshot_hash
policy_snapshot_hash
previous_receipt_hash
```

Не подписывать огромный request payload напрямую: подписывать canonical bounded digest contract.

Связь пишется в уже существующую таблицу `context_ledger_receipts`, вторая связь для того же не заводится.

## Request -> response

Каждый authoritative assistant response хранит:

```text
request_id
provider response metadata
model-visible output
usage
finish reason
interrupted?
```

Partial stream после cancellation/crash нельзя тихо считать normal complete response.

## Response -> tool effect

Каждый tool intent, возникший из response, получает:

```text
origin_request_id
```

Tool execution/receipt path должен иметь bounded linkage на `origin_request_envelope_hash` либо эквивалентный immutable reference.

Целевая audit chain:

```text
request envelope
    ↓
assistant response
    ↓
tool intent
    ↓
approval
    ↓
pre receipt
    ↓
effect
    ↓
terminal receipt
```

## Тесты

### Unit

- canonical bounded digest детерминирован и покрыт векторами;
- receipt отвергается при несовпадении `request_envelope_hash`;
- tool intent без `origin_request_id` не создаётся.

### Integration

1. **Tool call:** request → tool → approval → receipt → result имеет однозначный linkage.
2. **Chain linkage:** request receipt встраивается в существующую цепочку и проверяется вместе с ней.
3. **Interrupted stream:** прерванный ответ не помечается как complete и сохраняет связь с `request_id`.

## Критерии готовности

1. Каждый committed envelope имеет ровно один signed request receipt.
2. Каждый tool call ссылается на породивший его request.
3. Существующая цепочка receipts остаётся валидной и не переписывается.
