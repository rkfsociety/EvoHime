# План 05 — Provenance и реконструируемость model request

Статус: план реализации.

Цель плана — сделать каждый фактический model request в EvoHime реконструируемым из Core-owned durable state и связать его с существующей signed receipt chain. После реализации должно быть возможно проверить не только то, **что** агент выполнил, но и какие данные увидела модель, какой request был отправлен, какое решение из него последовало и какой effect реально произошёл.

Ключевой инвариант:

```text
MODEL_VISIBLE_MEANS_RECONSTRUCTABLE
```

Всё, что реально попадает в запрос к LLM, должно происходить из durable Core-owned state, immutable request-local snapshot, который становится durable до dispatch, либо deterministic projection из такого состояния. Если authoritative snapshot нельзя сохранить или проверить, request должен fail closed и не уходить к provider.

## Зависимости

### Блокирующие

- существующий Core-owned model gateway и routing pipeline;
- существующий Context Budget Manager;
- существующая SQLite persistence;
- существующая Signed hash-chain receipts архитектура и `evohime-verify.exe`;
- существующие durable agent/task events и recovery foundation.

Все перечисленные зависимости уже реализованы и описаны в [`../architecture.md`](../architecture.md) и [`../current-state.md`](../current-state.md).

### Опциональные

Нет. План должен быть реализуем поверх текущего состояния checkout без зависимости от незавершённых этапов 04.5–04.7.

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

1. Ключ записи ledger — собственный `context_ledger.id`; `model_call_id` — отдельная колонка, и уникальности по ней нет: поле `replan_of` прямо допускает несколько записей ledger для одного логического вызова. Поэтому `ledger_id` envelope ссылается на `context_ledger.id`, а не на `model_call_id`.
2. `ContextProjection` — не новая сущность рядом с ledger, а его расширение до model-visible содержимого. `projection_entry_id` ложится на `selected_items[].id`, `operation = summary` и `source_refs[]` — на существующие `compression[].summary_id`/`source_ids`, `operation = prune` — на `dropped_items[].drop_reason`. Второй независимый список выбранных item заводить запрещено.
3. Два хеша одного и того же контекста недопустимы. `context_projection_hash`
   вычисляется из `context_ledger_hash` и добавленного content-покрытия по
   правилам [05.1](05-1-canonical-request-contract.md); второй независимый
   список контекста запрещён.
4. Таблица `model_requests` не дублирует колонки ledger: `run_id`, `task_id`,
   `step_id` и `created_at` остаются в ledger, а `model_requests` ссылается на
   `ledger_id`. Дублировать допустимо только то, что нужно для
   offline-верификации без ledger; `provider` и `model` дублируются именно
   поэтому — они входят в подписанный request receipt. При этом ledger фиксирует
   provider/model на момент планирования контекста, а envelope — фактические:
   при fallback они расходятся, и authoritative значение — в envelope.
5. `context_ledger_receipts` уже существует. Signed request receipt подключается к этой связи, а не создаёт вторую.

Чего в ledger нет и ради чего нужен этот план: фактического system prompt, messages, tool schemas, effective model parameters, hash источников и captured bytes. Ledger отвечает на вопрос «какие item были выбраны», envelope — «что именно увидела модель».

### `model_call_id`

Сейчас это `format!("{task_id}-{iteration}")` (`crates/evohime-core/src/lib.rs`). Он не уникален по attempt: retry и fallback внутри одной итерации дают то же значение. Требование «новый `request_id` на каждый фактический dispatch» несовместимо с текущим идентификатором. Решение этапа 05.3: `logical_request_id` соответствует `model_call_id`, а `request_id` создаётся заново на каждый фактический dispatch. Миграции ledger для этого не требуется: связь attempt с ledger хранится в `model_requests.ledger_id`, а сам ledger остаётся записью «одна на одну сборку контекста».

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

## Состав этапов

Разделы этого обзора — сквозные: инвариант, границы, что уже есть в коде и критерий готовности плана целиком. Конкретные требования живут в файлах этапов; дублировать их здесь не нужно.

| Этап | Файл | Блокирующие зависимости |
| --- | --- | --- |
| 05.1 Canonical request contract | [05-1](05-1-canonical-request-contract.md) | — |
| 05.2 Durable storage | [05-2](05-2-durable-storage.md) | 05.1 |
| 05.3 Request integration | [05-3](05-3-request-integration.md) | 05.1, 05.2 |
| 05.4 Evidence provenance | [05-4](05-4-evidence-provenance.md) | 05.1, 05.2, 05.3 |
| 05.5 Signed request receipt и tool linkage | [05-5](05-5-receipt-and-tool-linkage.md) | 05.1, 05.2, 05.3 |
| 05.6 ContextProjection и append-only shadowing | [05-6](05-6-compaction-shadowing.md) | 05.1, 05.2, 05.4 |
| 05.7 Crash recovery | [05-7](05-7-crash-recovery.md) | 05.2, 05.3, 05.5 |
| 05.8 Удаление и retention | [05-8](05-8-redaction-and-retention.md) | 05.2, 05.4 |
| 05.9 Offline verification и export | [05-9](05-9-verify-and-export.md) | 05.1, 05.2, 05.5 |

```text
05.1 контракт ── 05.2 хранение ── 05.3 интеграция ─┬── 05.4 evidence ─┬── 05.6 shadowing
                                                   │                  │
                                                   │                  └── 05.8 удаление и retention
                                                   └── 05.5 receipts + tool linkage ┬── 05.9 verify/export
                                                                                    └── 05.7 recovery
```

Стрелка означает «блокирующая зависимость от левого узла»; 05.7 начинается
после 05.5, потому что recovery использует authoritative
`model_responses`/`tool_intents`. 05.6, 05.8 и 05.9 дополнительно зависят от
05.1 и 05.2, что видно из таблицы выше.

Обратные связи разрешены как опциональные с описанной деградацией и перечислены в самих файлах этапов. Три из них существенны:

1. **05.2 ждёт 05.8 по хранению текста.** До появления правил удаления хранилище может принимать явно маркированные hash-only metadata/hash-записи, но они storage-only: checkpoint возвращает `REQUEST_PROVENANCE_COMMIT_FAILED` и запрещает dispatch без полного snapshot. Включать dispatchable хранение текста раньше, чем существует его удаление, запрещено; `MODEL_VISIBLE_MEANS_RECONSTRUCTABLE` не имеет исключения.
2. **05.6 ждёт 05.8 по retention.** До неё append-only shadowing ничего не удаляет, поэтому действует временный потолок по объёму на задачу с явным статусом вместо тихого роста базы.
3. **05.9 ждёт 05.8 по состояниям.** До неё verifier различает только валидное и повреждённое; различение `redacted`/`retention_pruned` обязательно до выпуска.

Каждый этап доводится до зелёных unit/integration tests до перехода к следующему.

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

Ни один из этапов 05.1–05.9 новой IPC-поверхности не добавляет: перечисленная выше projection — это форма, в которой provenance когда-нибудь будет отдан UI, а не работа этого плана. Поэтому здесь требование действует как запрет: этап, который всё-таки добавит IPC-поверхность к provenance, обязан принести с собой интеграционный сценарий **Sensitive context:** renderer не получает raw payload даже тогда, когда policy разрешила отправить эти данные модели. Отдавать raw envelope в существующие IPC-команды по ходу этапов 05.1–05.9 запрещено.

## Runtime invariants

Добавить test/debug assertions:

1. Ни один provider request не dispatchится без committed envelope.
2. `request_id` уникален.
3. Каждый tool call имеет `origin_request_id`.
4. Каждый derived context item имеет `source_refs`.
5. Required refs разрешаются только в уже существующий immutable state.
6. Request envelope после commit immutable. Единственные разрешённые изменения — переход в `redacted` или `retention_pruned` по этапу [05.8](05-8-redaction-and-retention.md) и обновление terminal status; любая другая мутация payload запрещена. Инвариант проверяется именно в такой формулировке, иначе он противоречит правилам удаления.
7. Renderer не может создать или изменить envelope.
8. Credentials отсутствуют в serialized envelope.
9. Retry/fallback не переиспользует старый `request_id`.
10. Reconstruction не читает текущее workspace state как замену historical evidence.
11. Удалённый пользователем источник не остаётся восстановимым ни в одном committed envelope — ни как текст, ни как хеш там, где хеш приравнивается к содержимому.
12. Envelope в состоянии `redacted` или `retention_pruned` отличается от повреждённого и от hash mismatch.
13. Один model call не порождает двух независимых описаний контекста: у каждого envelope ровно один `ledger_id`. Обратное отношение не взаимно однозначно — одной записи ledger соответствуют все envelope её attempt-ов (retry/fallback контекст не пересобирают), а одному `logical_request_id` может соответствовать несколько записей ledger при replan. Второго описания контекста рядом с ledger при этом не возникает.
14. Ни один model call не обходит provenance-чокпойнт, включая вызовы саммаризатора.

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
15. какая запись `context_ledger` соответствует запросу — ровно одна на envelope, без второго независимого описания того же контекста.

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
