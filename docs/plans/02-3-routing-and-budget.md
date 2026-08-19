# Этап 02.3: Routing и budget

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этапы 02.1 и 02.2. Context Budget Manager реализован: route
decision опирается на budget/profile snapshot; без capability/health и local
route этап невыполним. Это единственный этап плана, опирающийся на контракт
Context Budget Manager.

Разблокирует: 02.4 — UI показывает именно этот trace.

## Что этап отдаёт наружу

Route decision с воспроизводимым trace и учётом бюджета.

## Что уже есть в коде

Есть: `RoutingTelemetry` с детерминированным JSON, fallback notice и счётчики
итераций/tool calls/времени в `RoutingRuntime`.

Нет: budget snapshot (приходит из Context Budget Manager), evaluation gate для small route,
блокировки cloud при незавершённой classification или непрошедшей redaction, и
самого подключения к `ToolAgent` — сегодня агент вызывает маршрут `"default"`
напрямую.

## Содержание

- Добавить trace decision: candidates, selected route, reason, privacy label,
  fallback count и budget snapshot.
- Зафиксировать интерфейс Context Budget Manager: Core вызывает
  `prepare(request, profile) -> Result<BudgetSnapshot, BudgetError>` до
  `select_route`, а после model/tool attempt вызывает
  `reserve(snapshot_id, usage) -> Result<BudgetSnapshot, BudgetError>`.
  Manager владеет snapshot и deadline; selector получает read-only копию, не
  может увеличить остатки и останавливается при stale/version mismatch.
  Передать из Context Budget Manager owned `BudgetSnapshot` с budget id,
  remaining input/output tokens, remaining tool calls, deadline и policy
  version. Snapshot создаётся до `select_route`, не мутируется селектором и
  сериализуется в trace без prompt или секретов.
- При ошибке Budget Manager Core (не selector) запрашивает второй
  `prepare(request, profile_fallback)` с заранее настроенным
  `minimal_read_only` профилем — без mutation/approval и без sensitive
  prompt; если такой профиль не разрешён policy, итог — `budget_unavailable`
  truthful refusal. Обработка `BudgetError` фиксирована таблицей ниже.
- Evaluation gate — Core-owned pre-flight проверка по фиксированному набору
  `simple` критериев и offline evaluation catalog. Catalog хранится как
  versioned JSONL в `tests/evals/catalog/routing/`: `catalog_version`,
  `task_class`, `dataset_hash`, `large_route_id`, `small_route_id`, `metric`,
  `large_score`, `small_score`, `quality_floor`, `generated_at`, `expires_at`,
  `signature`. Обновляет catalog только CI/release process атомарной заменой
  (symlink swap, не partial write); Core проверяет signature, schema, TTL и
  совместимость `route_id`/`capability_epoch`/`policy_version`. Small route
  допускается только при `small_score >= quality_floor` и `small_score >=
  large_score - configured_delta`, где `configured_delta` — часть
  `policy_version` и передаётся в `prepare()` вместе с остальным snapshot.
  Missing/stale/invalid catalog или неопределённая классификация выбирают
  large/политически допустимый route; renderer и provider не могут пропустить
  gate.
- Не отправлять cloud route, если classification не завершена или secrets
  не прошли redaction.
- Перед fallback сравнивать оценённый input context + reserved output/tool
  budget с `context_limit`; при переполнении fallback запрещается с честной
  причиной `context_limit_exceeded`.
- При повторных fallback остановиться по `run_budget`, сохранить фактический
  trace и вернуть truthful refusal с `safe_next_action`.
- `simple/complex` до исполнения означает expected tool-call count,
  вычисленный на этапе classification (02.1) и переданный в metadata
  запроса; после tool result разрешён ровно один post-analysis re-routing за
  run, если `actual_tool_calls > expected_tool_calls * 1.5`. Re-routing не
  расходует `max_fallbacks`, а ограничен отдельным `max_reroutes` (см.
  `run_budget`). Re-routing запрашивает новый snapshot через `prepare()` с
  обновлённым контекстом, заново проходит evaluation gate и, если новый
  выбор — cloud, переводит run в `pending_approval` (не terminal, не
  truthful_refusal) вместо автоматического перехода; пользователь либо
  подтверждает cloud route, либо run завершается `truthful_refusal` с
  `terminal_status = reroute_approval_declined` и
  `safe_next_action = manual_review`. Multi-hop всегда complex до
  доказанного отсутствия зависимости.
- Интегрировать `select_route` в `ToolAgent`: route decision создаётся до
  `chat_with_tools_for_route`, а каждый retry использует только snapshot,
  разрешённый fallback chain и тот же tool/approval/sandbox context; literal
  `"default"` остаётся лишь миграционным fallback конфигурации.

## Формат trace и наблюдаемость

Trace — versioned JSONL, одна запись на решение/попытку, с обязательными
`schema_version`, `trace_id`, `run_id`, `sequence`, RFC3339 `observed_at`,
`policy_version`, `snapshot_hash` (хэш request + classification context,
используется для deterministic replay сравнения), `budget_snapshot_hash`
(хэш содержимого `BudgetSnapshot`, используется для обнаружения
stale/mismatched snapshot), `classification`, `candidates[]` (route id,
capability epoch, `health_state` ∈ `{healthy, degraded, unavailable}`, reject
reason), `selected_route`, `reason_code`, `attempt`, `fallback_count`,
`event`, `latency_ms`, `usage`, `terminal_status`, `safe_next_action` (при
`truthful_refusal`; допустимые значения и их привязка к `terminal_status` —
таблица ниже). Prompt, token text, API key и raw model output не
записываются; при sensitive
отказах trace хранит только anonymized `reason_code`/`privacy_label`, полная
диагностика — в audit log. JSON Schema validation обязательна; replay
сортирует записи по `(trace_id, sequence)` и получает тот же route при тех же
версиях policy/catalog.

RoutingTelemetry считает `decision_total`, `route_selected_total`,
`fallback_total`, `refusal_total`, `provider_failure_total` и latency p50/p95/p99
по route/status. В логах всегда есть `trace_id`, значения prompt и секретов
редактируются.

## API contracts и внутренние структуры

Точные сигнатуры, обязательные для реализации:

```
select_route(request: &Request, snapshot: &BudgetSnapshot, catalog: &Catalog)
  -> Result<RouteDecision, RouteError>
prepare(request: &Request, profile: &Profile)
  -> Result<BudgetSnapshot, BudgetError>
reserve(snapshot_id: &str, usage: &Usage, route: &Route)
  -> Result<BudgetSnapshot, BudgetError>
```

`profile` — Core-owned policy profile текущего пользователя/сессии (privacy
label, allowed routes, cost/latency budget, `routing.quality_delta` —
источник `configured_delta` для evaluation gate), а не что-то, что выбирает
selector. Если `reserve` возвращает ошибку после успешного `select_route`,
decision откатывается (не применяется к runtime state), ошибка логируется, и
Core повторяет `prepare()` — inconsistent state недопустим.

`catalog: &Catalog` — Core владеет и передаёт read-only ссылку в
`select_route`; Core загружает catalog из `tests/evals/catalog/routing/` при
старте/обновлении версии и проверяет signature/schema/TTL один раз при
загрузке, а не на каждый вызов. `select_route` только читает уже
провалидированный catalog и не имеет доступа к файловой системе.

**Обработка `BudgetError`:**

| Ошибка | Действие | Trace `terminal_status` |
| --- | --- | --- |
| `budget_exhausted` | Core вызывает второй `prepare()` с `minimal_read_only` профилем, если разрешён policy; иначе `truthful_refusal` | `budget_exhausted` |
| `stale_snapshot` | reject route, `prepare()` заново, не более 2 повторов подряд; после 2 неудачных попыток — `truthful_refusal` | `snapshot_stale` |
| `policy_not_allowed` | `truthful_refusal` | `policy_violation` |
| `internal_error` | `truthful_refusal`, полная ошибка — в audit log | `internal_budget_error` |

**Обработка `RouteError`:**

| Ошибка | Действие | Trace `terminal_status` |
| --- | --- | --- |
| `no_candidates` | все routes в `fallback_chain` отброшены по health/policy | `both_routes_unavailable` |
| `catalog_invalid` | catalog failed signature/schema/TTL check при загрузке — используется large/политически допустимый route без gate | не terminal сам по себе, фиксируется в `candidates[]` reject reason |
| `classification_incomplete` | cloud route запрещён, fallback на allowed route или refusal | `classification_incomplete` |
| `context_limit_exceeded` | fallback запрещён | `context_limit_exceeded` |

**`run_budget`** — жёсткий предел на run, определяется Core до первого
`select_route` и не может быть расширен selector'ом:

```
run_budget = {
  max_fallbacks: u32 = 3,
  max_reroutes: u32 = 1,          // post-analysis re-routing, отдельно от max_fallbacks
  max_runtime_ms: u32 = 30_000,   // либо явный deadline из profile; ограничивает fallbacks и reroutes суммарно
  max_context_window: usize,      // максимум context_limit среди всех route в fallback_chain на момент старта run
}
```

`max_context_window` фиксируется до выбора route — это верхняя граница по
всей цепочке, а не значение уже выбранного route (иначе циклическая
зависимость: маршрут ещё не выбран, когда `run_budget` вычисляется).

При исчерпании `max_fallbacks` или `max_reroutes`: `terminal_status =
fallback_limit_reached`, trace сохраняется целиком, ответ —
`truthful_refusal` с `safe_next_action = contact_support`.

**Lifecycle `BudgetSnapshot`.** Budget Manager — единственный владелец:
создаёт snapshot в `prepare()` и хранит его в `snapshot_id -> (snapshot,
created_at, expiry)`; selector получает только read-only ссылку и не может
её мутировать. После успешного `reserve()` или истечения `expiry` Manager
удаляет запись; обращение к просроченному `snapshot_id` возвращает
`BudgetError::stale_snapshot`. Core обязан вызвать cleanup snapshot на всех
путях завершения run (RAII/`finally`-эквивалент): успешный ответ, любой
`RouteError`/`BudgetError` путь, `run_budget` exhaustion и истечение
`max_runtime_ms` deadline.

**Fallback chain.** `fallback_chain = [original_route, fallback_1, ...]`
конфигурируется Core policy, не selector'ом во время run. При загрузке
конфигурации Core прогоняет валидацию: каждый route в цепочке обязан
сохранять те же `tool_permissions`, sandbox context и approval requirements,
что и original route; конфигурация с несовпадающими permissions отклоняется
на этапе config load (fail-fast), а не во время run — селектор не принимает
решение об этом динамически.

**`health_state` кандидатов** ∈ `{healthy, degraded, unavailable}`:
`healthy` — gate применяется без дополнительных условий; `degraded` — gate
применяется, small route требует explicit approval; `unavailable` —
кандидат отбрасывается автоматически, переход к следующему в
`fallback_chain`. Значение всегда попадает в `candidates[]` trace.

**`safe_next_action`** — перечисление в trace для каждого `truthful_refusal`:

| `terminal_status` | `safe_next_action` |
| --- | --- |
| `budget_unavailable` | `retry_later` |
| `context_limit_exceeded` | `clarify_request` |
| `classification_incomplete` | `clarify_request` |
| `both_routes_unavailable` | `contact_support` |
| `fallback_limit_reached` | `contact_support` |
| `reroute_approval_declined` | `manual_review` |

`reroute_approval_declined` — единственный `terminal_status`, ведущий к
`safe_next_action = manual_review`; это финализация случая, когда
post-analysis re-routing выбрал cloud route, но пользователь не подтвердил
переход (см. re-routing в разделе «Содержание»). Пока подтверждение не
получено, run находится в промежуточном `pending_approval` state — это не
`terminal_status` и в таблицу не входит.

**Логирование чувствительных отказов.** При `budget_unavailable` или
`both_routes_unavailable` trace содержит только anonymized `reason_code` и
`privacy_label` — без prompt, secrets или model output. Полная диагностика
(включая internal error detail) пишется отдельно в audit log с ограниченным
доступом; связь `trace_id` ↔ audit log используется только privileged
debugging путём, не выставляется в обычный trace consumer.

**Версионирование policy/catalog.** `policy_version` и `catalog_version`
замораживаются в snapshot при `prepare()`; `select_route` и все дальнейшие
вызовы в рамках run используют версии из snapshot, а не текущие активные.
Если активная версия успела измениться — в trace пишется warning, но run не
блокируется. При `reserve()` версия из snapshot сверяется с текущей: расхождение
возвращает `BudgetError::stale_snapshot` (используется тот же trace-код
`snapshot_stale`, что и для истёкшего snapshot).

## Проверки

- route matrix по privacy, complexity, offline и health;
- cloud timeout → local fallback → final truthful status;
- no-secret-on-cloud test: незавершённая classification или непрошедшая
  redaction блокируют cloud route;
- evaluation сравнивает quality floor small vs large model.
- catalog signature/schema/TTL и stale catalog fallback;
- Budget Manager stale snapshot, reserve exhaustion и minimal read-only fallback;
- trace schema validation и deterministic replay;
- circuit breaker: `select_route` проверяет circuit state кандидата перед
  включением его в `candidates[]` (до gate и health check); открытый circuit
  помечает candidate `unavailable` с reject reason `circuit_open` и
  прекращает retry-loop для этого route до истечения circuit timeout;
- оба route unavailable → `truthful_refusal` с `terminal_status =
  both_routes_unavailable`, безопасным следующим действием и без ложного
  ответа модели;
- `run_budget` исчерпание → `fallback_limit_reached` с полным trace и
  `safe_next_action = contact_support`;
- post-analysis re-routing срабатывает ровно один раз за run и не зацикливается
  при повторном превышении threshold;
- fallback chain не допускает route с иными `tool_permissions`, чем у
  original route.

## Критерии готовности

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox.
- evaluation gate, budget snapshot и context-window check определены;
- `ToolAgent` подключён к `select_route`;
- есть versioned evaluation catalog, trace schema, replay и operational metrics;
- отказ Budget Manager и отсутствие обоих routes имеют проверенный bounded UX;
- API contracts (`select_route`/`prepare`/`reserve`) зафиксированы сигнатурами,
  `BudgetError`/`RouteError` обработка и `run_budget`
  (`max_fallbacks`/`max_reroutes`/`max_runtime_ms`/`max_context_window`)
  определены таблицей/структурой;
- lifecycle `BudgetSnapshot` (cleanup на всех путях завершения run) и
  `fallback_chain` (permissions preserved, валидируется при config load)
  описаны однозначно;
- `safe_next_action` и `health_state` — закрытые перечисления, присутствуют в
  trace schema; `reroute_approval_declined`/`pending_approval` покрыты
  тестом.
