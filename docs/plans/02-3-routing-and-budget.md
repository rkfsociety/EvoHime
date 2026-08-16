# Этап 02.3: Routing и budget

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: нет. Context Budget Manager реализован: route decision опирается на budget/profile snapshot;
этапы 02.1 (capabilities и health) и 02.2 (куда падать). Это единственный этап
плана, опирающегося на контракт Context Budget Manager.

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
- На simple requests использовать small route только после evaluation gate.
- Не отправлять cloud route, если classification не завершена или secrets
  не прошли redaction.
- При повторных fallback остановиться по run budget и запросить пользователя.

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
