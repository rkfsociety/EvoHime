# Этап 04.3: Routing и budget

Этап плана [04 Локальный SLM fallback и routing](04-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 01.1 — route decision опирается на budget/profile snapshot;
этапы 04.1 (capabilities и health) и 04.2 (куда падать). Это единственный этап
плана, которому нужен план 01.

Разблокирует: 04.4 — UI показывает именно этот trace.

## Что этап отдаёт наружу

Route decision с воспроизводимым trace и учётом бюджета.

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
