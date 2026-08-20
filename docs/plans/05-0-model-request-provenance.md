# План 05 — Provenance и реконструируемость model request

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

## Что есть в коде сейчас

Этот план не начинается с пустого места. Значительная часть учёта одного model call уже существует, и envelope обязан её **расширить**, а не продублировать.

### `context_ledger`

`crates/context-budget/src/ledger.rs` определяет `ContextLedgerEntry` — immutable запись «одна на один model call». В ней уже есть:

```text
model_call_id, task_id, session_id, created_at
provider, model
profile_version + profile_snapshot, tokenizer/normalizer/strategy versions
selected_items[] с оценкой токенов
dropped_items[] с DropReason
compression[] с summary_id и source_ids
loadout с tool_ids и schema_tokens
replan_of
outcome, budget_unavailable
context_ledger_hash
```

Хранилище — `crates/evohime-local-storage/src/context_ledger_store.rs`: таблица `context_ledger`, append-only `context_ledger_usage` с фактическим usage провайдера и `context_ledger_receipts` со связью на receipts. Запись идёт одной транзакцией `BEGIN IMMEDIATE` до model call (`crates/evohime-core/src/lib.rs`, `record_context_ledger`).

Следствия для плана:

1. `ContextProjection` — не новая сущность рядом с ledger, а его расширение до model-visible содержимого. `projection_entry_id` ложится на `selected_items[].id`, `operation = summary` и `source_refs[]` — на существующие `compression[].summary_id`/`source_ids`, `operation = prune` — на `dropped_items[].drop_reason`. Второй независимый список выбранных item заводить запрещено.
2. Два хеша одного и того же контекста недопустимы. Либо `projection_hash` вычисляется из `context_ledger_hash` и добавленного content-покрытия, либо `context_ledger_hash` объявляется его входом. Явно зафиксировать связь в контракте.
3. Таблица `model_requests` не дублирует колонки ledger: `provider`, `model`, `task_id`, `created_at` остаются в ledger, а `model_requests` ссылается на `ledger_id`. Дублировать допустимо только то, что нужно для offline-верификации без ledger.
4. `context_ledger_receipts` уже существует. Signed request receipt подключается к этой связи, а не создаёт вторую.

Чего в ledger нет и ради чего нужен этот план: фактического system prompt, messages, tool schemas, effective model parameters, hash источников и captured bytes. Ledger отвечает на вопрос «какие item были выбраны», envelope — «что именно увидела модель».

### `model_call_id`

Сейчас это `format!("{task_id}-{iteration}")` (`crates/evohime-core/src/lib.rs`). Он не уникален по attempt: retry и fallback внутри одной итерации дают то же значение. Требование «новый `request_id` на каждый фактический dispatch» несовместимо с текущим идентификатором. Решение этапа 05.3: `logical_request_id` соответствует `model_call_id`, `request_id` создаётся заново на каждый attempt, а ledger получает миграцию для связи с attempt.

### Запись ledger сегодня не fail-closed

Комментарий в `crates/evohime-core/src/lib.rs` фиксирует действующее поведение: «Неудача записи — diagnostic `ledger_write_failed`, а не повтор вызова модели». То есть при неудачной durable записи model call **выполняется**.

План это поведение меняет: неудача durable commit становится `REQUEST_PROVENANCE_COMMIT_FAILED` и запрещает dispatch. Это осознанная смена уже задокументированного контракта, а не недосмотр; `ledger_write_failed` перестаёт быть чистой диагностикой. Изменение выполняется на этапе 05.3 вместе с остальной fail-closed границей.

### Точек dispatch несколько

Гейтвей вызывается минимум из `stream_chat_with_policy`, `chat_with_tools_with_policy`, `chat_with_tools_with_policy_and_route` в `crates/evohime-core/src/lib.rs` и из саммаризатора в `crates/evohime-core/src/context_budget.rs`. Единого чокпойнта сегодня нет, и этап 05.3 обязан его создать, а не предположить.

Отдельный случай — саммаризатор: он сам делает model call, результат которого становится model-visible evidence следующего запроса. Его запрос коммитит собственный envelope с `request_kind = internal_summary`, а summary в родительском запросе получает `source_refs`, среди которых `request_id` породившего его вызова. Без этого граф происхождения обрывается на первом же summary.

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

### Canonicalization

Контракт должен иметь:

- deterministic canonical bytes;
- bounded sizes;
- stable validation/error codes;
- known-answer vectors;
- hash, пригодный для linkage с receipt chain.

Использовать тот же строгий подход, что Canonical Receipt v1. Не копировать receipt schema механически, если request contract имеет другой domain.

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

Последнее условие исключает прямое переиспользование существующего artifact store из [`../architecture.md`](../architecture.md): он вытесняет содержимое по TTL и последнему обращению, оставляя tombstone. Ссылка envelope на такой артефакт однажды перестанет реконструироваться, и это будет не `redacted`, а тихая потеря. Допустимы два варианта: отдельное provenance-хранилище либо правило «ссылка живого envelope удерживает артефакт от вытеснения» по аналогии с уже действующей защитой ссылок живого ledger entry. Вытеснение по retention самого provenance — другой случай, см. раздел «Удаление и retention».

### Дедупликация payload

Хранить полный payload messages на каждый attempt нельзя: контекст следующего шага почти целиком повторяет предыдущий, и у задачи из сотни шагов рост локальной SQLite квадратичный. Time-based retention это не лечит, потому что дублирование возникает внутри одной живой задачи.

Требование: model-visible блоки хранятся content-addressed, один блок — один blob, envelope хранит упорядоченный список хешей блоков. Повторное включение того же system prompt, того же сообщения или той же tool schema в следующий request не создаёт новую копию. Canonical bytes envelope при этом остаются детерминированными: они считаются по логической схеме, а не по физическому layout хранилища.

Reconstruction исторического request не должна читать текущее состояние workspace, memory либо child task как замену историческому snapshot.

## Context evidence и provenance graph

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

Captured bytes подчиняются тем же правилам удаления, что и остальной model-visible текст, см. раздел «Удаление и retention».

### Memory evidence

Исторический request должен ссылаться на точную revision/version:

```text
memory_id
revision
content_hash
```

Supersede не меняет факт того, что модель видела старую revision: envelope ссылается на зафиксированную revision, а не на «текущую». UI/export по-прежнему применяют текущую privacy/redaction policy.

`forget` — другой случай, см. раздел «Удаление и retention»: provenance не имеет права стать вторым хранилищем стёртого текста.

### Child evidence

Для child report сохранять:

```text
child_task_id
child_revision
report_hash
parent_sequence
```

Parent request не должен восстанавливаться из "последнего" child report; он должен ссылаться на exact accepted revision.

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

## Routing provenance

Не создавать второй несовместимый routing log.

Использовать существующий redacted model-gateway trace и policy snapshot. Envelope должен ссылаться минимум на:

```text
route_snapshot_hash
policy_snapshot_hash
```

### Что есть в коде сейчас

`crates/model-gateway/src/lib.rs` считает один `snapshot.round_trip_hash()`, кладёт его в `RunTrace` как policy hash и при ошибке подставляет строку `snapshot-hash-unavailable`. То есть сегодня это **одно** значение, а не два, и оно может отсутствовать.

Отсюда два следствия для реализации:

1. На первом этапе допустимо писать в оба поля один и тот же route/policy hash, но нельзя выдавать заглушку `snapshot-hash-unavailable` за валидный snapshot: отсутствие хеша — это `policy snapshot invalid` и fail closed по разделу «Dispatch integration и fail-closed boundary».
2. Разделение route hash и policy hash на два независимых значения — часть работы этапа 05.3 Request integration внутри `model-gateway`. Пункт «не менять model gateway selector» из раздела «Не менять без необходимости» относится к логике выбора провайдера, а не к экспорту снапшотов: сам алгоритм routing менять не требуется.

Reconstruction должна показывать:

- фактически выбранные provider/model;
- fallback lineage;
- активный policy snapshot;
- redacted route decision context, достаточный для audit.

Credentials и sensitive provider internals туда не входят.

## Durable storage

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
- ledger_id (FK на context_ledger)
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
- source_hash            -- тумбстоунится вместе с источником, см. «Удаление и retention»
```

Content-addressed хранение блоков по разделу «Дедупликация payload»:

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

Индексы должны обеспечивать быстрые запросы:

```text
request -> sources
source -> requests
run -> requests
logical_request -> attempts
ledger -> request
content_hash -> requests
```

`provider` и `model` продублированы из ledger намеренно: они входят в подписанный request receipt, и offline-верификатор обязан читать их без ledger. Остальные поля ledger не дублируются.

Committed envelope immutable, с единственным исключением — переход в `redacted`/`retention_pruned` по разделу «Удаление и retention». Terminal status может обновляться отдельно и не меняет canonical request payload/hash.

### Удаление и retention

Reconstructability не отменяет уже данных пользователю гарантий удаления. В [`../architecture.md`](../architecture.md) зафиксированы два разных режима, и envelope обязан различать их:

- `forget` памяти — logical deletion с tombstone из одних metadata **и digest**: хеш остаётся;
- удаление ambient-эпизода и `forget_window` — metadata-only tombstone **без текста и без хеша**, плюс физическое удаление высказываний, производных memory-кандидатов и `ambient.%`-строк журнала в одной транзакции.

Разница не косметическая. Для ambient хеш не сохраняют намеренно: там же зафиксировано, что короткую фразу перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому.

Envelope с полным system prompt и messages по умолчанию стал бы вторым местом, где стёртый текст лежит вечно. Это запрещено.

Правила:

1. Удаление источника (`forget` памяти, удаление эпизода, `forget_window`) в той же транзакции редактирует payload затронутых envelope: model-visible текст заменяется на typed tombstone, метаданные (`request_id`, `provider`, `model`, времена, счётчики, linkage) сохраняются.
2. `model_request_sources.source_hash` удалённого источника тумбстоунится по тому же правилу, что и сам источник: для ambient-высказывания и `forget_window` хеш удаляется, для `forget` памяти сохраняется digest. Оставлять хеш короткого удалённого текста в provenance запрещено — это восстановимость перебором, ровно та, ради которой ambient-tombstone его не хранит. `envelope_hash` это правило не затрагивает: он покрывает весь request целиком и перебору не поддаётся.
3. Такой envelope переходит в состояние `redacted` и перестаёт быть полностью реконструируемым. Это явное наблюдаемое состояние, а не тихая потеря данных: verifier обязан отличать `redacted` от повреждения и от несовпадения хеша.
4. Canonical hash оригинала и подписанный request receipt остаются. Цепочка receipts не переписывается: доказательство того, что request был именно такой, сохраняется, восстановимость текста — нет. Это тот же приём, что уже применён к `verified_pruned` в receipts.
5. У самого provenance-хранилища должен быть собственный retention, согласованный с retention receipts: envelope и captured evidence старше срока сжимаются до metadata + hash. Без этого append-only shadowing из раздела «ContextProjection» даёт неограниченный рост локальной SQLite, потому что вытесненные из контекста `A`, `B`, `C` не удаляются никогда.
6. Остаточное окно в бэкапах называется пользователю так же прямо, как для ambient-транскриптов, и вращается той же продовой константой.

Для этого нужны дополнительные typed errors и статусы, см. «Typed errors».

## Dispatch integration и fail-closed boundary

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

## Link request -> response -> tool effect

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

## Retry/fallback semantics

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

## Crash recovery

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

При превышении request не dispatchится. Возвращается typed Core error и bounded audit event. Нельзя "обрезать JSON" после assembly.

Из этого следует обязательное требование к Context Budget Manager: лимиты envelope — его **вход**, а не проверка после факта. Планировщик получает `MAX_REQUEST_ENVELOPE_BYTES`, `MAX_CONTEXT_PROJECTION_BYTES` и `MAX_TOOL_SET_BYTES` наравне с token budget и планирует под них. Иначе легитимно большой контекст просто заблокирует агента: обрезать после assembly нельзя, а собрать заново уже нечем. Проверка перед commit остаётся, но как backstop на ошибку планировщика, а не как штатный путь отказа.

Лимиты задаются в том же виде, что `contracts/receipts/v1/limits.json`, и покрываются такими же known-answer vectors.

## Sensitive data и trust boundary

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

## Plan review и другие model-call paths

`plan_review`, `plan_revision`, child, memory, schedule/ambient и internal summarization должны использовать общий provenance pipeline.

Не создавать отдельные механизмы для каждого feature.

Для plan review/revision различие выражается policy и `request_kind`:

```text
tools = none
read-only policy
request_kind = plan_review | plan_revision
```

Соседние Markdown context files получают такие же immutable workspace evidence refs.

## Offline verification и export

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
REQUEST_REDACTED
REQUEST_RETENTION_PRUNED
REQUEST_LEDGER_MISMATCH
REQUEST_EVIDENCE_EVICTED
```

На IPC boundary не использовать generic string errors вместо typed contract.

## Runtime invariants

Добавить test/debug assertions:

1. Ни один provider request не dispatchится без committed envelope.
2. `request_id` уникален.
3. Каждый tool call имеет `origin_request_id`.
4. Каждый derived context item имеет `source_refs`.
5. Required refs разрешаются только в уже существующий immutable state.
6. Request envelope после commit immutable. Единственные разрешённые изменения — переход в `redacted` или `retention_pruned` по разделу «Удаление и retention» и обновление terminal status; любая другая мутация payload запрещена. Инвариант проверяется именно в такой формулировке, иначе он противоречит правилам удаления.
7. Renderer не может создать или изменить envelope.
8. Credentials отсутствуют в serialized envelope.
9. Retry/fallback не переиспользует старый `request_id`.
10. Reconstruction не читает текущее workspace state как замену historical evidence.
11. Удалённый пользователем источник не остаётся восстановимым ни в одном committed envelope — ни как текст, ни как хеш там, где хеш приравнивается к содержимому.
12. Envelope в состоянии `redacted` или `retention_pruned` отличается от повреждённого и от hash mismatch.
13. Один model call не порождает двух независимых описаний контекста: у каждого envelope есть ровно один `ledger_id`, и обратно.
14. Ни один model call не обходит provenance-чокпойнт, включая вызовы саммаризатора.

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
- memory revision references;
- независимость canonical bytes от дедупликации блоков;
- соответствие `ContextProjection` записи `context_ledger`.

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
9. **Удаление:** `forget` памяти и удаление ambient-эпизода редактируют затронутые envelope; текст не восстанавливается, linkage и signed receipt сохраняются, verifier сообщает `redacted`.
10. **Retention:** envelope старше срока сжат до metadata + hash; цепочка receipts по-прежнему проверяется.
11. **Ambient-удаление:** после удаления эпизода в provenance не остаётся ни текста высказывания, ни его хеша; `envelope_hash` и receipt сохраняются.
12. **Commit failed:** искусственный отказ durable записи не приводит к provider dispatch — сегодняшнее поведение `ledger_write_failed` меняется, и тест это фиксирует.
13. **Ledger parity:** у каждого committed envelope ровно одна запись `context_ledger`, состав `ContextProjection` совпадает с `selected_items`/`compression`/`dropped_items` этой записи.
14. **Summarizer:** вызов саммаризатора коммитит собственный envelope `internal_summary`, а summary в родительском запросе ссылается на его `request_id`.
15. **Дедупликация:** сто последовательных запросов с почти одинаковым контекстом не дают линейного дублирования блоков в хранилище.
16. **Envelope limit:** контекст, упирающийся в `MAX_REQUEST_ENVELOPE_BYTES`, планируется под лимит и уходит в dispatch, а не отказывается после assembly.

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

Разделы выше — тематические, а не этапы: их заголовки намеренно без номеров, чтобы номер `05.N` означал ровно одно — этап из списка ниже и будущий файл `05-N-...md`. Ссылаться между разделами следует по названию раздела.

Рекомендуемая декомпозиция для Codex:

1. **05.1 Contract** — schema, canonical bytes, limits, errors, vectors. Разделы «Canonical request contract», «Exact request reconstruction», «Tool schemas и effective model parameters», «Size limits», «Typed errors».
2. **05.2 Storage** — SQLite migration, repository API, immutable committed envelope. Раздел «Durable storage» без правил удаления.
3. **05.3 Request integration** — build/validate/commit непосредственно перед dispatch, разделение route/policy hash. Разделы «Dispatch integration и fail-closed boundary», «Routing provenance», «Retry/fallback semantics», «Plan review и другие model-call paths».
4. **05.4 Evidence provenance** — conversation, memory, workspace, child, summaries. Раздел «Context evidence и provenance graph».
5. **05.5 Signed request receipt** — linkage с существующей receipt chain. Раздел «Signed model-request receipt».
6. **05.6 Tool linkage** — `origin_request_id` и request hash reference в effect receipts. Раздел «Link request -> response -> tool effect».
7. **05.7 Compaction/shadowing** — provenance-preserving Context Budget Manager projection. Раздел «ContextProjection».
8. **05.8 Recovery** — reconcile committed requests без terminal outcome. Раздел «Crash recovery».
9. **05.9 Redaction/retention** — согласование с `forget`, удалением эпизодов и retention receipts. Раздел «Удаление и retention».
10. **05.10 Verify/export** — offline verifier и export bundle. Раздел «Offline verification и export».

Раздел «Sensitive data и trust boundary» сквозной: его требования проверяются на каждом этапе, отдельного этапа под него нет.

Каждый этап должен быть доведён до зелёных unit/integration tests до перехода к следующему. Перед началом кода этот обзор разбивается на файлы этапов `05-1-...md` … `05-10-...md` по правилу каталога: у каждого этапа своя секция «Зависимости» с разделением блокирующих и опциональных. Инварианты плана при разбиении не меняются.

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
13. какие signed terminal receipts соответствуют этим effects;
14. если часть данных удалена пользователем — что именно недоступно и почему, отличимо от повреждения: `redacted` и `retention_pruned` наблюдаемы, а удалённый источник не восстанавливается ни текстом, ни хешем;
15. какой `context_ledger` соответствует запросу, без второго независимого описания того же контекста.

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
