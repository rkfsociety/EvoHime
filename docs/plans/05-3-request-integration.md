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

Единого чокпойнта нет. Гейтвей вызывается из
`stream_chat_with_policy`, `chat_with_tools_with_policy`,
`chat_with_tools_with_policy_and_route` в `crates/evohime-core/src/lib.rs` и из
саммаризатора в `crates/evohime-core/src/context_budget.rs`; отдельным поиском
по provider-client и `dispatch` нужно подтвердить отсутствие дополнительных
путей. Этап обязан создать чокпойнт и закрыть архитектурный обход, а не только
добавить общий helper и тест на соглашение.

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
payload_mode = hash_only или отсутствует обязательный полный block payload
```

Для hash-only до 05.8 результатом checkpoint является
`REQUEST_PROVENANCE_COMMIT_FAILED`: provider не вызывается, даже если строки
metadata/hash-only storage были успешно записаны repository-слоем. Это
обязательное fail-closed поведение, а не штатный degraded dispatch.

После successful commit provider/network failure является обычным request outcome и не удаляет envelope.

До фактического вызова provider checkpoint отдельной bounded транзакцией
выставляет и коммитит `model_requests.dispatch_at`. Если эта запись не
закоммичена, provider не вызывается. Marker означает только «dispatch мог
начаться»; он не является доказательством ответа. `dispatch_at IS NULL` после
restart является доказательством отсутствия provider dispatch и используется
этапом [05.7](05-7-crash-recovery.md) для классификации `interrupted`.

Лимиты envelope из [05.1](05-1-canonical-request-contract.md) передаются в Context Budget Manager как вход планирования. Проверка перед commit остаётся backstop-ом на ошибку планировщика, а не штатным путём отказа.

### Архитектурный барьер checkpoint

Ввести Core-owned `ModelRequestCheckpoint`, который является единственным
владельцем последовательности `build/validate -> commit_envelope(FullForDispatch)
-> dispatch`. Все публичные обёртки сначала строят envelope, затем передают
его checkpoint; provider client и raw dispatch handle не экспортируются в
модули feature-кода. На уровне Rust видимостью модулей/trait boundaries
запретить прямой вызов provider из `lib.rs`, `context_budget.rs`, child,
memory, schedule/ambient, plan review и plan revision. Тест дополнительно
сканирует список разрешённых call sites, но компиляторная граница является
обязательной защитой.

Минимальный refactor inventory этого этапа:

1. `stream_chat_with_policy`;
2. `chat_with_tools_with_policy`;
3. `chat_with_tools_with_policy_and_route`;
4. summarizer в `context_budget.rs`;
5. все child, memory, schedule/ambient, `plan_review`, `plan_revision` и
   internal summarization пути, найденные поиском provider dispatch.

Каждый пункт либо вызывает checkpoint напрямую, либо вызывает обёртку,
которая не имеет доступа к raw provider dispatch. Добавление нового gateway
call site после этого этапа должно быть невозможно без изменения закрытого
интерфейса checkpoint и соответствующего compile-time/architecture test.

## Routing provenance

Не создавать второй несовместимый routing log. Использовать существующий redacted model-gateway trace и policy snapshot. Envelope ссылается минимум на:

```text
route_snapshot_hash
policy_snapshot_hash
```

В envelope также сохраняется `route_policy_hash_shared`. Он равен `true` ровно
тогда, когда оба поля являются одним и тем же canonical snapshot из одного
источника; при независимом route snapshot и policy snapshot он равен `false`,
даже если значения случайно совпали. До выделения двух источников допустим
один общий snapshot с явным `true`; это не скрытое предположение и не
заглушка.

`crates/model-gateway/src/lib.rs` считает один `snapshot.round_trip_hash()`, кладёт его в `RunTrace` как policy hash и при ошибке подставляет строку `snapshot-hash-unavailable`. То есть сегодня это **одно** значение, а не два, и оно может отсутствовать.

Отсюда два следствия:

1. На первом шаге допустимо писать в оба поля один и тот же route/policy hash с
   `route_policy_hash_shared = true`, но нельзя выдавать заглушку
   `snapshot-hash-unavailable` за валидный snapshot: отсутствие хеша — это
   `policy snapshot invalid` и fail closed. В полном режиме оба значения
   обязательны; `NULL` или placeholder не являются provenance evidence.
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

- отказ dispatch по каждому из семи условий fail-closed;
- hash-only и missing full block payload возвращают
  `REQUEST_PROVENANCE_COMMIT_FAILED` и не достигают provider;
- отсутствие snapshot hash трактуется как `policy snapshot invalid`, а не как валидное значение;
- shared route/policy source требует `route_policy_hash_shared = true`, а два
  независимых источника — `false`;
- retry и fallback дают разные `request_id` при общем `logical_request_id` и общем `ledger_id`;
- при fallback `provider`/`model` envelope берутся фактические, а не унаследованные из ledger.

### Integration

1. **Simple chat:** stored envelope реконструирует logical request.
2. **Fallback:** provider A failure + provider B дают два envelope одного logical request.
3. **Commit failed:** искусственный отказ durable записи не приводит к provider dispatch — сегодняшнее поведение `ledger_write_failed` меняется, и тест это фиксирует.
4. **Hash-only fail closed:** до 05.8 отсутствие полного payload не приводит к
   provider dispatch; checkpoint возвращает `REQUEST_PROVENANCE_COMMIT_FAILED`.
5. **Summarizer:** вызов саммаризатора коммитит собственный envelope `internal_summary`, а summary в родительском запросе ссылается на его `request_id`.
6. **Envelope limit:** контекст, упирающийся в `MAX_REQUEST_ENVELOPE_BYTES`, планируется под лимит и уходит в dispatch, а не отказывается после assembly.
7. **Plan review:** `plan_review`/`plan_revision` проходят тем же pipeline с пустым tool set.
8. **Checkpoint boundary:** прямой provider dispatch из перечисленных feature
   paths не компилируется/проваливает architecture test.

## Критерии готовности

1. Ни один вызов гейтвея не обходит чокпойнт, включая саммаризатор; это проверяется тестом, а не соглашением.
2. Неудача durable commit гарантированно запрещает dispatch.
3. Route hash и policy hash — два независимых значения либо явно помеченный одинаковый источник.
4. Retry/fallback не переиспользует `request_id`.
5. До 05.8 hash-only режим никогда не приводит к provider dispatch;
   `REQUEST_PROVENANCE_COMMIT_FAILED` наблюдаем снаружи checkpoint.
