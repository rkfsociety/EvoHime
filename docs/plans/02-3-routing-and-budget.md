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
- При ошибке Budget Manager используется только заранее настроенный
  `minimal_read_only` budget без mutation/approval и без sensitive prompt; если
  такой budget не разрешён policy, итог — `budget_unavailable` truthful refusal.
- Evaluation gate — Core-owned pre-flight проверка по фиксированному набору
  `simple` критериев и offline evaluation catalog. Catalog хранится как
  versioned JSONL в `tests/evals/catalog/routing/`: `catalog_version`,
  `task_class`, `dataset_hash`, `large_route_id`, `small_route_id`, `metric`,
  `large_score`, `small_score`, `quality_floor`, `generated_at`, `expires_at`,
  `signature`. Обновляет catalog CI/release process; Core проверяет signature,
  schema, TTL и совместимость route/capability epoch. Small route допускается
  только при `small_score >= quality_floor` и `small_score >= large_score -
  configured_delta`. Missing/stale/invalid catalog или неопределённая
  классификация выбирают large/политически допустимый route; renderer и
  provider не могут пропустить gate.
- Не отправлять cloud route, если classification не завершена или secrets
  не прошли redaction.
- Перед fallback сравнивать оценённый input context + reserved output/tool
  budget с `context_limit`; при переполнении fallback запрещается с честной
  причиной `context_limit_exceeded`.
- При повторных fallback остановиться по run budget, сохранить фактический
  trace и запросить пользователя.
- `simple/complex` до исполнения означает expected tool-call count от
  Core-owned classifier; после tool result разрешён один post-analysis
  re-routing, если фактический count пересёк threshold. Multi-hop всегда
  complex до доказанного отсутствия зависимости.
- Интегрировать `select_route` в `ToolAgent`: route decision создаётся до
  `chat_with_tools_for_route`, а каждый retry использует только snapshot,
  разрешённый fallback chain и тот же tool/approval/sandbox context; literal
  `"default"` остаётся лишь миграционным fallback конфигурации.

## Формат trace и наблюдаемость

Trace — versioned JSONL, одна запись на решение/попытку, с обязательными
`schema_version`, `trace_id`, `run_id`, `sequence`, RFC3339 `observed_at`,
`policy_version`, `snapshot_hash`, `budget_snapshot_hash`, `classification`,
`candidates[]` (route id, capability epoch, health state, reject reason),
`selected_route`, `reason_code`, `attempt`, `fallback_count`, `event`,
`latency_ms`, `usage`, `terminal_status`. Prompt, token text, API key и raw
model output не записываются. JSON Schema validation обязательна; replay
сортирует записи по `(trace_id, sequence)` и получает тот же route при тех же
версиях policy/catalog.

RoutingTelemetry считает `decision_total`, `route_selected_total`,
`fallback_total`, `refusal_total`, `provider_failure_total` и latency p50/p95/p99
по route/status. В логах всегда есть `trace_id`, значения prompt и секретов
редактируются.

## Проверки

- route matrix по privacy, complexity, offline и health;
- cloud timeout → local fallback → final truthful status;
- no-secret-on-cloud test: незавершённая classification или непрошедшая
  redaction блокируют cloud route;
- evaluation сравнивает quality floor small vs large model.
- catalog signature/schema/TTL и stale catalog fallback;
- Budget Manager stale snapshot, reserve exhaustion и minimal read-only fallback;
- trace schema validation и deterministic replay;
- circuit open внутри run прекращает retry-loop;
- оба route unavailable → `truthful_refusal` с `route_unavailable`, безопасным
  следующим действием и без ложного ответа модели.

## Критерии готовности

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox.
- evaluation gate, budget snapshot и context-window check определены;
- `ToolAgent` подключён к `select_route`;
- есть versioned evaluation catalog, trace schema, replay и operational metrics;
- отказ Budget Manager и отсутствие обоих routes имеют проверенный bounded UX.
