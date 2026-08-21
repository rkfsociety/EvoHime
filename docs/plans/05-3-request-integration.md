# План 05.3 — Request integration и fail-closed boundary

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — единственная точка, через которую проходит любой model request: build → validate → durable commit → dispatch. Здесь же появляются разделённые route/policy hash, семантика retry/fallback и подключение всех остальных model-call paths.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — контракт envelope;
- [05.2](05-2-durable-storage.md) — durable commit;
- существующий Core-owned model gateway и routing pipeline;
- существующий Context Budget Manager.

### Опциональные

- [05.4](05-4-evidence-provenance.md) — evidence provenance. До неё `context_projection` заполняется тем, что уже даёт `context_ledger`: ids выбранных item, решения о compaction и pruning, оценки токенов, но без `source_hash` источников и captured bytes. Fail-closed по `required evidence unresolved` в этом состоянии не срабатывает, потому что required refs ещё не объявлены; остальные условия отказа действуют полностью.

## Что есть в коде сейчас

Единого чокпойнта нет. Гейтвей вызывается минимум из `stream_chat_with_policy`, `chat_with_tools_with_policy`, `chat_with_tools_with_policy_and_route` в `crates/evohime-core/src/lib.rs` и из саммаризатора в `crates/evohime-core/src/context_budget.rs`. Этап обязан этот чокпойнт создать, а не предположить, что он есть.

Запись `context_ledger` сегодня **не** fail-closed: комментарий в `crates/evohime-core/src/lib.rs` фиксирует «Неудача записи — diagnostic `ledger_write_failed`, а не повтор вызова модели», то есть при неудачной durable записи model call выполняется. Этот этап меняет контракт: неудача commit становится `REQUEST_PROVENANCE_COMMIT_FAILED` и запрещает dispatch.

`model_call_id` сейчас `format!("{task_id}-{iteration}")` и не уникален по attempt. Этап вводит `request_id` на каждый фактический dispatch, оставляя `model_call_id` в роли `logical_request_id`. Миграция ledger при этом не нужна: attempt связывается с ledger через `model_requests.ledger_id` (`context_ledger.id`), а сам ledger остаётся записью на одну сборку контекста.

## Интеграция

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

Лимиты envelope из [05.1](05-1-canonical-request-contract.md) передаются в Context Budget Manager как вход планирования. Проверка перед commit остаётся backstop-ом на ошибку планировщика, а не штатным путём отказа.

## Routing provenance

Не создавать второй несовместимый routing log. Использовать существующий redacted model-gateway trace и policy snapshot. Envelope ссылается минимум на:

```text
route_snapshot_hash
policy_snapshot_hash
```

`crates/model-gateway/src/lib.rs` считает один `snapshot.round_trip_hash()`, кладёт его в `RunTrace` как policy hash и при ошибке подставляет строку `snapshot-hash-unavailable`. То есть сегодня это **одно** значение, а не два, и оно может отсутствовать.

Отсюда два следствия:

1. На первом шаге допустимо писать в оба поля один и тот же route/policy hash, но нельзя выдавать заглушку `snapshot-hash-unavailable` за валидный snapshot: отсутствие хеша — это `policy snapshot invalid` и fail closed.
2. Разделение route hash и policy hash на два независимых значения — часть работы этого этапа внутри `model-gateway`. Пункт «не менять model gateway selector» из обзора плана относится к логике выбора провайдера, а не к экспорту снапшотов: сам алгоритм routing менять не требуется.

Reconstruction должна показывать:

- фактически выбранные provider/model;
- fallback lineage;
- активный policy snapshot;
- redacted route decision context, достаточный для audit.

Credentials и sensitive provider internals туда не входят.

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

## Остальные model-call paths

`plan_review`, `plan_revision`, child, memory, schedule/ambient и internal summarization используют общий provenance pipeline. Отдельные механизмы под каждый feature не создаются.

Для plan review/revision различие выражается policy и `request_kind`:

```text
tools = none
read-only policy
request_kind = plan_review | plan_revision
```

Саммаризатор — особый случай: он сам делает model call, результат которого становится model-visible evidence следующего запроса. Его вызов коммитит собственный envelope с `request_kind = internal_summary`, а summary в родительском запросе получает `source_refs`, среди которых `request_id` породившего его вызова. Без этого граф происхождения обрывается на первом же summary.

## Тесты

### Unit

- отказ dispatch по каждому из шести условий fail-closed;
- отсутствие snapshot hash трактуется как `policy snapshot invalid`, а не как валидное значение;
- retry и fallback дают разные `request_id` при общем `logical_request_id` и общем `ledger_id`;
- при fallback `provider`/`model` envelope берутся фактические, а не унаследованные из ledger.

### Integration

1. **Simple chat:** stored envelope реконструирует logical request.
2. **Fallback:** provider A failure + provider B дают два envelope одного logical request.
3. **Commit failed:** искусственный отказ durable записи не приводит к provider dispatch — сегодняшнее поведение `ledger_write_failed` меняется, и тест это фиксирует.
4. **Summarizer:** вызов саммаризатора коммитит собственный envelope `internal_summary`, а summary в родительском запросе ссылается на его `request_id`.
5. **Envelope limit:** контекст, упирающийся в `MAX_REQUEST_ENVELOPE_BYTES`, планируется под лимит и уходит в dispatch, а не отказывается после assembly.
6. **Plan review:** `plan_review`/`plan_revision` проходят тем же pipeline с пустым tool set.

## Критерии готовности

1. Ни один вызов гейтвея не обходит чокпойнт, включая саммаризатор; это проверяется тестом, а не соглашением.
2. Неудача durable commit гарантированно запрещает dispatch.
3. Route hash и policy hash — два независимых значения либо явно помеченный одинаковый источник.
4. Retry/fallback не переиспользует `request_id`.
