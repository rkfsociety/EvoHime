# План 02: Локальный SLM fallback и routing

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Добавить в model gateway управляемую маршрутизацию между cloud provider и
локальной SLM: чувствительные/offline/simple задачи выполняются локально,
сложные non-sensitive задачи используют основной route, а недоступность cloud
не ломает всю Еву.

## Границы

Локальная модель — дополнительный provider gateway route, не отдельный runtime.
Electron не выбирает модель напрямую: Core получает route decision и применяет
policy. Provider secrets не передаются local route.

## Routing policy

Входные признаки:

- sensitivity/privacy labels и наличие секретов;
- offline mode и provider health;
- complexity/required context/tool-call capability;
- latency, token budget и cost budget;
- user-selected model preference в пределах разрешённой policy.

Начальные правила:

1. secret/private/offline → local route;
2. bounded read-only/simple → local route;
3. complex multi-hop non-sensitive → configured cloud route;
4. cloud timeout/5xx/rate limit → local fallback, если tool calling capable;
5. mutation/approval semantics не ослабляются при смене модели.

## Что уже есть в коде

`crates/model-gateway/src/routing_policy.rs` и `routing_runtime.rs` уже
содержат детерминированный выбор маршрута: bounded `RouteCandidate` с
capabilities, privacy class, cost и latency, `select_route` с fallback chain и
причинами отказа, режимы `RoutingMode`, лимиты запуска и `RoutingTelemetry`.

**Но это библиотека, не поведение продукта.** Она вызывается только из
`evals.rs`; сам `ToolAgent` по-прежнему ходит в `chat_with_tools_for_route`
с фиксированным маршрутом `"default"`. Планировать работу нужно от этого:
основная часть — подключение и недостающие части контракта, а не написание
селектора с нуля.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 02.1 | [Provider contract](02-1-provider-contract.md) | capability metadata и health state | 02.2, 02.3 |
| 02.2 | [Local provider](02-2-local-provider.md) | loopback-only local route | 02.3 |
| 02.3 | [Routing и budget](02-3-routing-and-budget.md) | route decision с trace | 02.4 |
| 02.4 | [UI](02-4-routing-ui.md) | фактический route в интерфейсе | UI |

Этапы 02.1 и 02.2 не касаются бюджета и потому не зависят от Context Budget
Manager.

## Зависимости плана

Блокирующие: нет. Context Budget Manager реализован: route decision опирается на budget/profile snapshot,
который определён именно там; существующие evaluation catalog (`tests/evals/`)
и provider health model.

Опциональных интеграций нет. Поддержка конкретной SLM/launcher выбирается
отдельным ADR после проверки Windows resource requirements; этот план не
фиксирует бренд модели.

## Критерии готовности плана

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox;
- UI показывает фактический результат routing;
- cloud outage оставляет usable local degraded mode, если он настроен.
