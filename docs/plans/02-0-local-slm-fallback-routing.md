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

Правила применяются в фиксированном порядке (от более сильного ограничения к
более слабому): `privacy/secret` → `offline` → `approval/tool policy` →
`context/capability` → `health/circuit breaker` → `evaluation gate` →
`budget/cost` → `user preference` → стабильный tie-break по `route_id`.
Предпочтение пользователя является только подсказкой внутри уже разрешённого
множества и не может поднять route выше privacy, approval или sandbox policy.

Классификация запроса формальна: `simple` — read-only, не более 2 tool calls,
без multi-hop зависимости и с контекстом не более 25% окна выбранной модели;
`complex` — mutation/approval, более 2 tool calls, multi-hop либо превышение
этого порога. Неопределённая классификация считается `complex` и не проходит
small-route gate.

Начальные правила: secret/private/offline требуют local; bounded
read-only/simple допускает local после evaluation gate; complex non-sensitive
использует configured cloud; cloud timeout/5xx/rate-limit допускает local
fallback только при совместимых capabilities. Mutation, approval, tool
permissions и sandbox не меняются при смене модели.

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

Блокирующие: нет. До появления этапов 02.1–02.4 существующий режим `default`
остаётся единственным route, а недоступность local/cloud возвращается честным
`unavailable`/`route_denied`, без маскировки под успех.

Context Budget Manager, evaluation catalog и provider health model считаются
доступными контрактами; их минимальные интерфейсы зафиксированы в этапах ниже.

Опциональных интеграций нет. Поддержка конкретной SLM/launcher выбирается
отдельным ADR после проверки Windows resource requirements; этот план не
фиксирует бренд модели.

## Критерии готовности плана

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox;
- UI показывает фактический результат routing;
- cloud outage оставляет usable local degraded mode, если он настроен.
- ToolAgent использует `select_route`, а не фиксированный `"default"`;
- приоритеты правил и разрешение конфликтов определены явно.
