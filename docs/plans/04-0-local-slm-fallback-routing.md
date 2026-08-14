# План 04: Локальный SLM fallback и routing

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

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 04.1 | [Provider contract](04-1-provider-contract.md) | capability metadata и health state | 04.2, 04.3 |
| 04.2 | [Local provider](04-2-local-provider.md) | loopback-only local route | 04.3 |
| 04.3 | [Routing и budget](04-3-routing-and-budget.md) | route decision с trace | 04.4 |
| 04.4 | [UI](04-4-routing-ui.md) | фактический route в интерфейсе | UI |

Этапы 04.1 и 04.2 можно вести параллельно с планом 01: provider contract и
локальный адаптер не касаются бюджета.

## Зависимости плана

Блокирующие: этап 01.1 — route decision опирается на budget/profile snapshot,
который определён именно там; существующие evaluation catalog (`tests/evals/`)
и provider health model. Остальные этапы плана 01 этому плану не нужны.

Опциональных интеграций нет. Поддержка конкретной SLM/launcher выбирается
отдельным ADR после проверки Windows resource requirements; этот план не
фиксирует бренд модели.

## Критерии готовности плана

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox;
- UI показывает фактический результат routing;
- cloud outage оставляет usable local degraded mode, если он настроен.
