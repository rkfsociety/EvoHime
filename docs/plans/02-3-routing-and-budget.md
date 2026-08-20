# Этап 02.3: Routing и budget

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этапы 02.1 и 02.2. От 02.1 берутся `RoutePolicySnapshot`,
`RunHealthOverlay`, `now_ms`, retry/circuit контракт и контракт terminal
result; без capability/health и local route этап невыполним.

Context Budget Manager **уже реализован**, его канонический контракт живёт в
[`../architecture.md`](../architecture.md) («Context Budget Manager»), а не в
этом файле. Этап 02.3 не проектирует бюджет заново и не вводит для него новых
API: он только читает существующие `ModelContextProfile`, `TokenEstimator`,
`assemble`/`replan`, `BudgetUnavailable` и `context_ledger_hash`. Аналогично
лимиты запуска берутся из существующего `evohime_core::run_policy`
(`RunPolicy`/`RunUsage`/`BudgetExceeded`), а не из нового параллельного
бюджета.

Разблокирует: 02.4 — UI показывает именно этот trace.

## Что этап отдаёт наружу

Route decision с воспроизводимым trace, с учётом уже существующего context
budget и в границах уже существующего `RunPolicy`.

## Что уже есть в коде

Есть: `RoutingTelemetry` с детерминированным JSON, fallback notice и счётчики
итераций/tool calls/времени в `RoutingRuntime`; `RunPolicy`/`RunUsage` с
`max_iterations`/`max_wall_clock_ms`/`max_tool_calls`/`max_tokens`/
`max_cost_micros` и остановкой по `BudgetExceeded`; `ContextBudget::assemble`,
`replan`, `record_actual_usage` и профили `crates/context-budget/profiles.json`,
подключённые к agent loop.

Нет: evaluation gate для small route, evaluation catalog как артефакта,
привязки route decision к context budget (сегодня профиль выбирается уже
по факту известных provider/model), блокировки cloud при незавершённой
classification или непрошедшей redaction, trace-схемы этого этапа и самого
подключения `select_route` к `ToolAgent` — сегодня агент вызывает маршрут
`"default"` напрямую.

Расходится с этим документом и правится при подключении: существующий
`select_route(&RoutingRequest, &[RouteCandidate])` не видит snapshot, overlay,
catalog и `now_ms`, а его tie-break — `cost → latency → fallback_rank →
route_id`. Целевая сигнатура и порядок — из 02.1 и раздела «API contracts»
ниже; двух разных порядков tie-break в кодовой базе быть не должно.

## Порядок шагов run

Фиксированная последовательность Core; ни один шаг не выполняется селектором:

1. `classify` (02.0) — privacy label, `simple/complex`, `expected_tool_calls`,
   статус redaction;
2. `estimate` — route-независимая консервативная оценка размера запроса
   `estimated_input_tokens` существующим `TokenEstimator` (без профиля: профиль
   привязан к provider/model, которые ещё не выбраны);
3. `collect candidates` + `create snapshot` (02.1), включая `budget_id`,
   `policy_hashes`, `catalog_version` и `quality_delta`;
4. `select_route(...)` — фильтры, evaluation gate, tie-break;
5. `assemble` выбранного route его собственным `ModelContextProfile` —
   единственное место, где применяется полный контракт Context Budget Manager;
6. `execute_attempt`; при отказе — возврат к шагу 4 с тем же snapshot и
   обновлённым overlay, в границах `RunPolicy` и retry-лимитов 02.1.

Шаг 2 обязателен именно как route-независимый: попытка получить точный бюджет
до выбора route даёт цикл «профиль зависит от модели, модель зависит от
бюджета». Точность добирается на шаге 5, где отказ уже атрибутируется
конкретному route.

## Содержание

- Добавить trace decision: candidates, selected route, reason, privacy label,
  fallback count и ссылку на context budget (`budget_id`,
  `context_ledger_hash`, `estimated_input_tokens`).
- Использовать бюджет через существующий контракт, без новых API:
  - `budget_id` берётся из snapshot 02.1. Отсутствие бюджета — разрешённый
    режим (`budget_id = null`, в trace `budget_absent`); заявленный, но
    неактивный бюджет даёт `budget_unavailable` и selection не запускается —
    ровно как в 02.1;
  - pre-flight фильтр кандидата: candidate отбрасывается, если
    `estimated_input_tokens + profile(candidate).reserves_total() >
    profile(candidate).hard_limit_tokens`, с reject reason
    `context_limit_exceeded`. Профиль читается из уже загруженного каталога
    профилей, его `profile_version` пишется в trace;
  - после выбора route `assemble` выполняется штатно. `BudgetUnavailable`
    (стадии `mandatory_overflow`, `drops_exhausted`, `estimator_unavailable`,
    `provider_replan_failed`) не ретраится — это прямой запрет канонического
    контракта. Он исключает **этот** route из selection (запись в overlay,
    reject reason `context_assembly_failed`) и возвращает управление на шаг 4;
    если допустимых кандидатов не осталось, run завершается
    `context_assembly_failed`;
  - `context-length error` провайдера обрабатывает Context Budget Manager
    ровно одним `replan`; routing в этот момент route не меняет.
- Evaluation gate — Core-owned pre-flight проверка по фиксированному набору
  `simple` критериев (02.0) и offline evaluation catalog. Catalog хранится как
  versioned JSONL: `catalog_version`, `task_class`, `dataset_hash`,
  `large_route_id`, `small_route_id`, `metric`, `large_score`, `small_score`,
  `quality_floor`, `generated_at`, `expires_at`, `signature`. Источник —
  `tests/evals/catalog/routing/`, но Core читает **не** его: packaging копирует
  каталог в app resources, и runtime-путь задаётся конфигурацией. Артефакт,
  который не попадает в установленный дистрибутив, не может быть runtime-
  зависимостью.
- Обновляет catalog только CI/release process атомарной заменой: запись во
  временный файл в той же директории и `rename`/`ReplaceFile` поверх целевого.
  Symlink swap запрещён — на Windows он требует привилегий или developer mode
  и не является переносимой атомарной операцией.
- Core проверяет signature, schema и совместимость `route_id`/
  `capability_epoch`/`policy_version` один раз при загрузке; `expires_at`
  дополнительно сверяется с `now_ms` при создании snapshot — процесс живёт
  дольше TTL, и проверки «только при загрузке» недостаточно.
  `catalog_version` замораживается в snapshot и не меняется внутри run.
- Small route допускается только при `small_score >= quality_floor` и
  `small_score >= large_score - quality_delta`. `quality_delta` — поле
  Core-owned policy profile (`routing.quality_delta`), входящее в
  `policy_hashes` и замороженное в snapshot. Missing/stale/invalid catalog или
  неопределённая классификация переводят gate в состояние `gate_unavailable`:
  small route не выбирается, берётся large/политически допустимый route.
  Renderer и provider пропустить gate не могут.
- Не отправлять cloud route, если classification не завершена или secrets не
  прошли redaction: такой candidate отбрасывается с reject reason
  `classification_incomplete` до всех прочих критериев.
- При повторных fallback остановиться по лимитам `RunPolicy` и retry-конфигу
  02.1, сохранить фактический trace и вернуть truthful refusal с
  `safe_next_action`.
- `simple/complex` до исполнения означает `expected_tool_calls`, вычисленный на
  этапе classification (правила — в [обзоре плана](02-0-local-slm-fallback-routing.md),
  раздел «Routing policy») и переданный в metadata запроса. После tool result
  разрешён ровно один post-analysis re-routing за run, если
  `actual_tool_calls > ceil(max(expected_tool_calls, 1) * 1.5)`. Нижняя
  граница обязательна: при `expected_tool_calls = 0` порог иначе срабатывает
  на первом же вызове инструмента. Re-routing ограничен отдельным
  `max_reroutes` (см. ниже), не расходует retry-бюджет 02.1, использует тот же
  snapshot (он immutable до конца run), заново проходит evaluation gate и, если
  новый выбор — cloud, переводит run в промежуточное состояние
  `pending_approval` вместо автоматического перехода. Multi-hop всегда complex
  до доказанного отсутствия зависимости.
- `pending_approval` ограничен по времени: ожидание подтверждения не входит в
  `retry.max_elapsed_ms` и в `RunPolicy.max_wall_clock_ms` (иначе run падает по
  дедлайну из-за того, что пользователь не смотрит на экран), но ограничено
  собственным `reroute_approval_timeout_ms = 120_000`. Истёкший таймаут
  эквивалентен отказу: `terminal_status = reroute_approval_declined`.
- Интегрировать `select_route` в `ToolAgent`: route decision создаётся до
  `chat_with_tools_for_route`, а каждый retry использует тот же snapshot,
  разрешённый fallback chain и тот же tool/approval/sandbox context; literal
  `"default"` остаётся лишь миграционным fallback конфигурации.

## Формат trace и наблюдаемость

Trace — versioned JSONL, `schema_version = 1`, одна запись на решение/попытку.
Обязательные поля: `schema_version`, `trace_id`, `run_id`, `sequence`,
`attempt_id`, `now_ms` (то же значение, что передано в `select_route` —
единственный источник времени решения; RFC3339 `observed_at` допускается
дополнительно, как человекочитаемая отметка записи, и в replay не участвует),
`policy_version`, `catalog_version`, `snapshot_hash` (round-trip hash snapshot
из 02.1), `classification`, `candidates[]`, `selected_route`, `reason_code`,
`fallback_count`, `event`, `latency_ms`, `usage`, `terminal_status`,
`safe_next_action` (при refusal). Бюджетная часть: `budget_id` или
`budget_absent`, `estimated_input_tokens`, `profile_version` выбранного route и
`context_ledger_hash` после `assemble`.

Каждый элемент `candidates[]` содержит `route_id`, `capability_epoch`,
`health_status` ∈ `{ready, degraded, stale, unavailable}`, `circuit_state` ∈
`{closed, open, cooldown}` (два независимых измерения 02.1, смешивать их
нельзя), производный `health_state` для UI и `reject_reason`.

Prompt, token text, API key и raw model output не записываются; при sensitive
отказах trace хранит только anonymized `reason_code`/`privacy_label`, полная
диагностика — в audit log. JSON Schema validation обязательна; replay сортирует
записи по `(trace_id, sequence)` и получает тот же route при тех же версиях
policy/catalog и записанных `now_ms`.

RoutingTelemetry считает `decision_total`, `route_selected_total`,
`fallback_total`, `refusal_total`, `provider_failure_total` и latency p50/p95/p99
по route/status. В логах всегда есть `trace_id`, значения prompt и секретов
редактируются.

## API contracts и внутренние структуры

Сигнатура — расширение контракта 02.1, а не второй параллельный `select_route`:

```
select_route(
  request: &RoutingRequest,
  snapshot: &RoutePolicySnapshot,   // владелец — Core (02.1), immutable за run
  overlay: &RunHealthOverlay,       // read-only для селектора
  catalog: &Catalog,                // уже провалидированный, read-only
  attempt_id: u32,
  now_ms: u64,
) -> Result<RouteDecision, RouteError>
```

`select_route` не имеет доступа к файловой системе и часам, не мутирует
snapshot и overlay и не обращается к Context Budget Manager: всё, что ему нужно
от бюджета, лежит в snapshot (`budget_id`, `estimated_input_tokens`, профили
кандидатов, `quality_delta`). Порядок правил — фиксированный порядок 02.0:
privacy → offline → approval/tool → context/capability → health/circuit →
evaluation gate → budget/cost → user preference → lexical `route_id`.

`Catalog` загружается Core при старте и при смене версии; `select_route` только
читает уже провалидированный экземпляр.

**Обработка `RouteError`:**

| Ошибка | Действие | `terminal_status` | 02.1 terminal result |
| --- | --- | --- | --- |
| `no_candidates` | snapshot пуст — ни один провайдер не прошёл schema/probe | `no_routes_configured` | `route_exhausted` / `no_candidates` |
| `all_routes_excluded` | candidates были, но все отброшены фильтрами/overlay | `both_routes_unavailable` | `route_exhausted` / `all_routes_excluded` |
| `classification_incomplete` | cloud route запрещён; fallback на allowed local route, иначе refusal | `classification_incomplete` | `route_exhausted` / `all_routes_excluded` |
| `context_limit_exceeded` | все кандидаты не вмещают запрос по pre-flight оценке | `context_limit_exceeded` | `route_exhausted` / `all_routes_excluded` |
| `policy_denied` | запрошенный route запрещён policy и допустимых нет | `policy_violation` | `failed` / `policy_violation` |

Различие `no_candidates` и `all_routes_excluded` перенесено из 02.1 без
изменений: первое — отсутствие конфигурации, второе — отказы провайдеров;
смешивать их запрещено.

Состояния, возникающие вне `select_route`:

| Ситуация | `terminal_status` | 02.1 terminal result |
| --- | --- | --- |
| `budget_id` заявлен, но не найден/неактивен | `budget_unavailable` | `failed` / `budget_unavailable` |
| `BudgetUnavailable` на всех допустимых routes | `context_assembly_failed` | `route_exhausted` / `all_routes_excluded` |
| исчерпан `retry.max_attempts` | `fallback_limit_reached` | `route_exhausted` / `max_attempts_reached` |
| исчерпан `retry.max_elapsed_ms` или `RunPolicy` | `run_deadline_exceeded` | `route_exhausted` / `max_elapsed_reached` |
| re-routing в cloud не подтверждён или истёк таймаут | `reroute_approval_declined` | `failed` / `policy_violation` |
| внутренняя ошибка Core | `internal_error` | `failed` / `invalid_request` |

`terminal_status` — уточнение для UI поверх terminal result 02.1, а не второй
независимый жизненный цикл: каждая строка обязана иметь пару в 02.1, иначе
trace нельзя сопоставить с run trace провайдерского слоя.

**Лимиты run.** Отдельного `run_budget` этот этап не вводит: fallback-попытки
ограничены `retry.max_attempts = 3`, `retry.max_attempts_per_route = 2` и
`retry.max_elapsed_ms = 15000` из 02.1, а весь run — существующим `RunPolicy`
(`max_iterations`, `max_wall_clock_ms`, `max_tool_calls`, `max_tokens`,
`max_cost_micros`). Второй набор лимитов поверх них давал бы мёртвые значения:
`max_runtime_ms = 30000` при `max_elapsed_ms = 15000` не наступает никогда.

Добавляется ровно одно новое поле политики — post-analysis re-routing:

```
routing.max_reroutes                  = 1
routing.reroute_approval_timeout_ms   = 120_000
routing.quality_delta                 // порог evaluation gate
```

Все три входят в `policy_hashes` и замораживаются в snapshot.

`max_context_window` не вводится: верхняя граница по цепочке уже выражена
pre-flight фильтром по `hard_limit_tokens` каждого кандидата, а фильтр по
конкретному route честнее, чем максимум по цепочке, который ни одному
реальному route не соответствует.

**Fallback chain.** `fallback_chain = [original_route, fallback_1, ...]`
конфигурируется Core policy, не селектором во время run. При загрузке
конфигурации Core прогоняет валидацию: каждый route в цепочке обязан сохранять
те же `tool_permissions`, sandbox context и approval requirements, что и
original route; конфигурация с несовпадающими permissions отклоняется на этапе
config load (fail-fast), а не во время run.

**`health_state`** — производная UI-проекция для trace и 02.4, вычисляемая по
фиксированной таблице (сверху вниз, до первого совпадения). Она не заменяет
`health_status`/`circuit_state` и не смешивает их: оба исходных поля остаются в
`candidates[]`.

| условие | `health_state` |
| --- | --- |
| `circuit_state ∈ {open, cooldown}` | `unavailable` (reject reason `circuit_open`/`rate_limited`) |
| `health_status ∈ {stale, unavailable}` | `unavailable` |
| `health_status = degraded` | `degraded` — gate применяется, small route требует explicit approval |
| `health_status = ready` | `healthy` |

**`safe_next_action`** — закрытое перечисление, обязательное для каждого
refusal:

| `terminal_status` | `safe_next_action` |
| --- | --- |
| `budget_unavailable` | `retry_later` |
| `context_assembly_failed` | `clarify_request` |
| `context_limit_exceeded` | `clarify_request` |
| `classification_incomplete` | `clarify_request` |
| `both_routes_unavailable` | `contact_support` |
| `no_routes_configured` | `contact_support` |
| `fallback_limit_reached` | `contact_support` |
| `run_deadline_exceeded` | `retry_later` |
| `policy_violation` | `manual_review` |
| `reroute_approval_declined` | `manual_review` |
| `internal_error` | `contact_support` |

`pending_approval` — промежуточное состояние, а не `terminal_status`, и в
таблицу не входит.

**Логирование чувствительных отказов.** При `budget_unavailable`,
`both_routes_unavailable` и `internal_error` trace содержит только anonymized
`reason_code` и `privacy_label` — без prompt, secrets или model output. Полная
диагностика пишется отдельно в audit log с ограниченным доступом; связь
`trace_id` ↔ audit log используется только privileged debugging путём.

**Версионирование policy/catalog.** `policy_version` и `catalog_version`
замораживаются в snapshot при его создании; `select_route` и все дальнейшие
вызовы в рамках run используют версии из snapshot, а не текущие активные. Если
активная версия успела измениться — в trace пишется warning, но run не
блокируется: snapshot immutable до terminal result (02.1), поэтому «устаревший
snapshot» внутри run — недостижимое состояние и отдельного кода отказа не
требует.

## Проверки

- route matrix по privacy, complexity, offline, health и circuit state;
- cloud timeout → local fallback → final truthful status;
- no-secret-on-cloud test: незавершённая classification или непрошедшая
  redaction блокируют cloud route;
- evaluation сравнивает quality floor small vs large model;
- catalog signature/schema fail при загрузке и истёкший `expires_at` на
  подставленном `now_ms` дают `gate_unavailable` и large route, а не small;
- catalog обновляется atomic rename: прерванная запись не даёт частичного файла
  и не ломает работающий Core;
- pre-flight `context_limit_exceeded` на кандидате с малым окном и успешный
  выбор кандидата с большим окном при том же `estimated_input_tokens`;
- `BudgetUnavailable` на первом route исключает его и переводит run на
  следующий кандидат без повторной сборки для того же route;
- отсутствие бюджета даёт `budget_absent` в trace, а не `budget_unavailable`;
- trace schema validation и deterministic replay по записанным `now_ms`;
- circuit breaker: открытый circuit помечает candidate `unavailable` с reject
  reason `circuit_open` и прекращает попытки по этому route, не завершая run,
  пока остаются кандидаты;
- `no_routes_configured` и `both_routes_unavailable` различаются в trace;
- исчерпание `retry.max_attempts` даёт `fallback_limit_reached`, исчерпание
  `max_elapsed_ms`/`RunPolicy` — `run_deadline_exceeded`, оба с полным trace;
- post-analysis re-routing срабатывает не более `max_reroutes` раз за run, не
  зацикливается при повторном превышении threshold и не срабатывает при
  `expected_tool_calls = 0` на первом же tool call;
- `pending_approval` → подтверждение продолжает run; отказ и истёкший
  `reroute_approval_timeout_ms` дают `reroute_approval_declined`; ожидание не
  расходует `max_elapsed_ms` и `max_wall_clock_ms`;
- fallback chain не допускает route с иными `tool_permissions`, чем у original
  route (проверка на config load);
- `health_state` вычисляется по таблице проекции, а `health_status`/
  `circuit_state` остаются в trace раздельно.

## Критерии готовности

- решение route воспроизводимо по snapshot, catalog и записанному `now_ms`;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox;
- evaluation gate, pre-flight context check и привязка к существующему Context
  Budget Manager определены без введения нового бюджетного API;
- `ToolAgent` подключён к `select_route`;
- есть versioned evaluation catalog с runtime-путём в дистрибутиве, trace
  schema `v1`, replay и operational metrics;
- каждый `terminal_status` имеет пару в terminal result 02.1, а каждый refusal
  — `safe_next_action`;
- `safe_next_action` и `health_state` — закрытые перечисления, присутствуют в
  trace schema; `reroute_approval_declined`/`pending_approval` покрыты тестом;
- лимиты run выражены через `RunPolicy` и retry-конфиг 02.1; единственные новые
  параметры — `routing.max_reroutes`, `routing.reroute_approval_timeout_ms` и
  `routing.quality_delta`.
