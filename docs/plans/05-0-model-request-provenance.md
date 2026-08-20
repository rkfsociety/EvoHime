# План 05 — Model Request Provenance and Reconstructability

Статус: план реализации.

Цель плана — сделать каждый фактический model request в EvoHime реконструируемым из Core-owned durable state и связать его с существующей signed receipt chain. После реализации должно быть возможно проверить не только то, **что** агент выполнил, но и какие данные увидела модель, какой request был отправлен, какое решение из него последовало и какой effect реально произошёл.

Ключевой инвариант:

```text
MODEL_VISIBLE_MEANS_RECONSTRUCTABLE
```

Всё, что реально попадает в запрос к LLM, должно происходить из durable Core-owned state, immutable request-local snapshot, который становится durable до dispatch, либо deterministic projection из такого состояния. Если authoritative snapshot нельзя сохранить или проверить, request должен fail closed и не уходить provider.

## Зависимости

### Блокирующие

- существующий Core-owned model gateway и routing pipeline;
- существующий Context Budget Manager;
- существующая SQLite persistence;
- существующая Signed hash-chain receipts архитектура и `evohime-verify.exe`;
- существующие durable agent/task events и recovery foundation.

Все перечисленные зависимости уже реализованы и описаны в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md).

### Опциональные

Нет. План должен быть реализуем поверх текущего состояния checkout без зависимости от будущих планов 04.x.

## Non-goals

На этом этапе не требуется:

- заменять существующий event journal;
- переписывать agent loop ради event-sourcing как отдельной цели;
- внедрять DeepSeek Harness, Cordis или runtime plugin system;
- менять Electron/Core trust boundary;
- логировать API keys, Authorization headers, DPAPI plaintext либо иные credentials;
- сохранять скрытый provider reasoning/CoT, если он не является разрешённым model-visible output;
- гарантировать повторение того же ответа модели при одинаковом request;
- давать renderer прямой доступ к raw provenance payload;
- превращать каждый внутренний Core event в signed receipt.

## Целевой flow

```text
Durable conversation state
        │
        ├── memory evidence
        ├── workspace evidence
        ├── child reports
        ├── tool catalog
        ├── routing snapshot
        └── system context
                │
                ▼
        ContextProjection
                │
                ▼
       ModelRequestEnvelopeV1
                │
          durable commit
                │
                ▼
          provider dispatch
                │
                ▼
        assistant response
                │
                ▼
           tool intent
                │
                ▼
      signed execution receipts
```

Envelope описывает **фактически отправляемый** request и создаётся после routing, context budgeting, compaction/pruning, prompt assembly, tool filtering и provider capability resolution, но до provider dispatch.

## 05.1 Canonical request contract

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

### Canonicalization

Контракт должен иметь:

- deterministic canonical bytes;
- bounded sizes;
- stable validation/error codes;
- known-answer vectors;
- hash, пригодный для linkage с receipt chain.

Использовать тот же строгий подход, что Canonical Receipt v1. Не копировать receipt schema механически, если request contract имеет другой domain.

## 05.2 Exact request reconstruction

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
- verifier способен проверить hash.

Reconstruction исторического request не должна читать текущее состояние workspace, memory либо child task как замену историческому snapshot.

## 05.3 Context evidence и provenance graph

Добавить явный provenance для model-visible context.

Логическая структура:

```text
ContextEvidenceRef {
    kind
    source_id
    source_version?
    source_hash
    classification
    projection
}
```

Минимальные `kind`:

```text
conversation_event
memory
workspace_file
workspace_index
child_report
plan_review
system_context
compaction
tool_result
generated_summary
core_static
```

Derived context должен хранить `source_refs[]`, образуя DAG происхождения поверх существующей линейной receipt chain.

Пример:

```text
workspace chunk ─┐
memory entry ────┼─> summary ──> model request
child report ────┘
```

Receipt chain остаётся линейной. Provenance graph не заменяет её.

### Workspace evidence

Для выбранного фрагмента сохранять минимум:

```text
canonical path
content hash
selected range/chunk identity
captured content или immutable artifact ref
```

Путь сам по себе недостаточен. После capture файл может измениться.

Snapshot operation должна согласованно получить bytes, hash и metadata. Именно captured bytes/projection участвуют в request.

### Memory evidence

Исторический request должен ссылаться на точную revision/version:

```text
memory_id
revision
content_hash
```

Последующее supersede/forget не меняет факт того, что модель видела старую revision. UI/export по-прежнему применяют текущую privacy/redaction policy.

### Child evidence

Для child report сохранять:

```text
child_task_id
child_revision
report_hash
parent_sequence
```

Parent request не должен восстанавливаться из "последнего" child report; он должен ссылаться на exact accepted revision.

## 05.4 ContextProjection

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

### Append-only shadowing

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

## 05.5 Tool schemas и effective model parameters

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

## 05.6 Routing provenance

Не создавать второй несовместимый routing log.

Использовать существующий redacted model-gateway trace и policy snapshot. Envelope должен ссылаться минимум на:

```text
route_snapshot_hash
policy_snapshot_hash
```

Reconstruction должна показывать:

- фактически выбранные provider/model;
- fallback lineage;
- активный policy snapshot;
- redacted route decision context, достаточный для audit.

Credentials и sensitive provider internals туда не входят.

## 05.7 Durable storage

Добавить SQLite migration и repository layer.

Предпочтительный logical layout:

```text
model_requests
- request_id PK
- logical_request_id
- attempt
- parent_request_id
- run_id
- task_id
- step_id
- request_kind
- provider
- model
- envelope_version
- envelope_hash
- envelope_blob / immutable artifact ref
- projection_hash
- route_snapshot_hash
- policy_snapshot_hash
- status
- created_at
- completed_at
```

```text
model_request_sources
- request_id
- ordinal
- source_kind
- source_id
- source_version
- source_hash
```

Индексы должны обеспечивать быстрые запросы:

```text
request -> sources
source -> requests
run -> requests
logical_request -> attempts
```

Committed envelope immutable. Terminal status может обновляться отдельно и не меняет canonical request payload/hash.

## 05.8 Dispatch integration и fail-closed boundary

Интегрировать непосредственно перед provider dispatch:

```text
route
context budget
prompt assembly
tool schemas
effective config
        ↓
build envelope
        ↓
validate
        ↓
durable commit
        ↓
dispatch
```

Provider dispatch запрещён, если:

```text
envelope validation failed
canonical hash failed
durable commit failed
required evidence unresolved
captured source integrity failed
policy snapshot invalid
```

После successful commit provider/network failure является обычным request outcome и не удаляет envelope.

## 05.9 Signed model-request receipt

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

## 05.10 Link request -> response -> tool effect

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

## 05.11 Retry/fallback semantics

Каждый реальный provider dispatch — отдельный committed envelope.

Запрещено:

```text
one envelope -> provider A failed -> silently send provider B
```

Нужно:

```text
R1 -> provider A -> failed
R2(parent=R1, same logical_request_id) -> provider B
```

Если меняются model, provider, context, tools либо policy, это естественно отражается новым envelope/hash.

## 05.12 Crash recovery

Committed request без terminal outcome после restart не удаляется.

Recovery должен присвоить честный explicit outcome:

```text
interrupted
```

если отсутствие завершения доказуемо, либо:

```text
unknown_outcome
```

если реальное внешнее состояние нельзя доказать.

Нельзя автоматически превращать неизвестный request в success или ordinary failure.

Общий принцип:

```text
never erase incomplete work; close or reconcile it explicitly
```

## 05.13 Size limits

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

При превышении request не dispatchится. Возвращается typed Core error и bounded audit event. Нельзя "обрезать JSON" после assembly.

## 05.14 Sensitive data и trust boundary

Reconstructability не означает бесконтрольное дублирование секретов.

Никогда не сериализовать в envelope/export:

```text
API keys
Authorization headers
provider secrets
DPAPI plaintext
```

Если Sensitive/Secret context policy разрешила отправить модели, authoritative historical evidence остаётся Core-owned. Renderer по умолчанию получает только redacted typed summary.

Renderer не участвует в:

- envelope construction;
- canonical hashing;
- provenance validation;
- authoritative verification.

Минимальная UI/IPC projection при необходимости:

```text
request id
model
provider
timestamp
context item count
tool count
status
integrity status
```

Полный raw-envelope IPC в первом этапе не требуется.

## 05.15 Plan review и другие model-call paths

`plan_review`, `plan_revision`, child, memory, schedule/ambient и internal summarization должны использовать общий provenance pipeline.

Не создавать отдельные механизмы для каждого feature.

Для plan review/revision различие выражается policy и `request_kind`:

```text
tools = none
read-only policy
request_kind = plan_review | plan_revision
```

Соседние Markdown context files получают такие же immutable workspace evidence refs.

## 05.16 Offline verification и export

Расширить `evohime-verify.exe` и существующий receipt export bundle.

Verifier должен проверять:

```text
request envelope canonical hash
signed request receipt
receipt chain linkage
source hash references
tool receipt linkage
```

Export bundle может добавить versioned sections:

```text
model_requests/
context_evidence/
manifest
```

Manifest содержит минимум:

```text
schema versions
request count
receipt count
hashes
chain roots/checkpoints
```

Export atomic, bounded и без credentials.

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
```

На IPC boundary не использовать generic string errors вместо typed contract.

## Runtime invariants

Добавить test/debug assertions:

1. Ни один provider request не dispatchится без committed envelope.
2. `request_id` уникален.
3. Каждый tool call имеет `origin_request_id`.
4. Каждый derived context item имеет `source_refs`.
5. Required refs разрешаются только в уже существующий immutable state.
6. Request envelope после commit immutable.
7. Renderer не может создать или изменить envelope.
8. Credentials отсутствуют в serialized envelope.
9. Retry/fallback не переиспользует старый `request_id`.
10. Reconstruction не читает текущее workspace state как замену historical evidence.

## Тесты

### Unit

Покрыть:

- canonical serialization;
- stable hashes;
- source ordering;
- tool schema ordering;
- provenance graph validation;
- size limits;
- unknown version rejection;
- retry lineage;
- workspace captured evidence;
- memory revision references.

### Known-answer vectors

Создать:

```text
contracts/model-request/v1/test-vectors/
```

Проверять canonical bytes и expected hash. Rust vectors обязательны. Cross-language parity нужна только если Electron реально будет проверять этот contract.

### Integration

Обязательные сценарии:

1. **Simple chat:** stored envelope реконструирует logical request.
2. **Tool call:** request -> tool -> approval -> receipt -> result имеет однозначный linkage.
3. **Fallback:** provider A failure + provider B дают два envelope одного logical request.
4. **Compaction:** originals сохраняются, summary содержит source refs.
5. **File mutation:** файл меняется после capture, historical request восстанавливает старый captured content.
6. **Crash:** envelope committed до response; recovery сохраняет request и честный interrupted/unknown outcome.
7. **Sensitive context:** renderer не получает raw payload.
8. **Child:** parent request указывает exact accepted child revision.

### Property tests

Для accepted envelope:

```text
reconstruct(envelope) == original logical request
```

и:

```text
hash(reconstruct(envelope)) == envelope_hash
```

Каждый required provenance ref обязан разрешаться.

## Порядок реализации

Рекомендуемая декомпозиция для Codex:

1. **05.1 Contract** — schema, canonical bytes, limits, errors, vectors.
2. **05.2 Storage** — SQLite migration, repository API, immutable committed envelope.
3. **05.3 Request integration** — build/validate/commit непосредственно перед dispatch.
4. **05.4 Evidence provenance** — conversation, memory, workspace, child, summaries.
5. **05.5 Signed request receipt** — linkage с существующей receipt chain.
6. **05.6 Tool linkage** — `origin_request_id` и request hash reference в effect receipts.
7. **05.7 Compaction/shadowing** — provenance-preserving Context Budget Manager projection.
8. **05.8 Recovery** — reconcile committed requests без terminal outcome.
9. **05.9 Verify/export** — offline verifier и export bundle.

Каждый этап должен быть доведён до зелёных unit/integration tests до перехода к следующему. Если при реализации потребуется отдельный reviewable файл этапа, разбить этот обзор на `05-1-...md`, `05-2-...md` и т.д. без изменения инвариантов плана.

## Не менять без необходимости

Codex должен избегать unrelated refactor. В частности, не требуется:

- переделывать Electron shell;
- менять provider credential storage;
- менять supervisor ownership;
- заменять protobuf/named-pipe transport;
- переписывать Canonical Receipt v1;
- заменять model gateway selector;
- менять существующую child workflow state machine;
- вводить arbitrary plugin loading;
- вводить Node runtime в Core.

## Документация после реализации

После полного завершения перенести стабильный контракт в:

```text
../architecture.md
../current-state.md
../security/model-request-provenance-v1.md
```

После подтверждённого переноса и зелёных тестов временные файлы плана 05 удалить согласно правилам каталога `docs/plans/`.

## Definition of Done

Для любого завершённого либо прерванного model call должно быть возможно определить без доверия к renderer и без чтения текущего workspace вместо historical snapshot:

1. какой logical operation его породил;
2. какой provider/model использовался;
3. какой route/policy snapshot был активен;
4. какой system prompt увидела модель;
5. какие messages/content blocks увидела модель;
6. какие tools увидела модель;
7. какие memory/workspace/child данные вошли в context;
8. откуда произошёл каждый derived context item;
9. какой canonical envelope был committed перед dispatch;
10. совпадает ли его hash;
11. есть ли валидный signed request receipt;
12. какие tool effects произошли вследствие request;
13. какие signed terminal receipts соответствуют этим effects.

Ключевой итог:

```text
source evidence
      ↓
context projection
      ↓
signed model request
      ↓
model response
      ↓
tool intent
      ↓
policy / approval
      ↓
signed execution receipt
      ↓
effect
      ↓
signed terminal outcome
```

План расширяет текущую receipt architecture от доказательства "что агент выполнил" к проверяемой causal history: "что увидела модель -> что было отправлено -> какое действие из этого последовало -> что реально произошло".
