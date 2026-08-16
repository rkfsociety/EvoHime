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
- Передать из Context Budget Manager owned `BudgetSnapshot` с budget id,
  remaining input/output tokens, remaining tool calls, deadline и policy
  version. Snapshot создаётся до `select_route`, не мутируется селектором и
  сериализуется в trace без prompt или секретов.
- Evaluation gate — Core-owned pre-flight проверка по фиксированному набору
  `simple` критериев и offline evaluation catalog: small route допускается
  только если capability/context checks пройдены и quality floor не ниже
  настроенного порога относительно large route. При отсутствии результата,
  stale catalog или неопределённой классификации выбирается large/политически
  допустимый route; renderer и provider не могут самовольно пропустить gate.
- Не отправлять cloud route, если classification не завершена или secrets
  не прошли redaction.
- Перед fallback сравнивать оценённый input context + reserved output/tool
  budget с `context_limit`; при переполнении fallback запрещается с честной
  причиной `context_limit_exceeded`.
- При повторных fallback остановиться по run budget, сохранить фактический
  trace и запросить пользователя.
- Интегрировать `select_route` в `ToolAgent`: route decision создаётся до
  `chat_with_tools_for_route`, а каждый retry использует только snapshot,
  разрешённый fallback chain и тот же tool/approval/sandbox context; literal
  `"default"` остаётся лишь миграционным fallback конфигурации.

## Проверки

- route matrix по privacy, complexity, offline и health;
- cloud timeout → local fallback → final truthful status;
- no-secret-on-cloud test: незавершённая classification или непрошедшая
  redaction блокируют cloud route;
- evaluation сравнивает quality floor small vs large model.

## Критерии готовности

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox.
- evaluation gate, budget snapshot и context-window check определены;
- `ToolAgent` подключён к `select_route`.
