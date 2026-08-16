# Этап 02.1: Provider contract

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: существующая provider health model. Context Budget Manager здесь не
требуется: контракт провайдера не касается бюджета.

Разблокирует: 02.2 (локальный провайдер объявляет свои capabilities) и 02.3
(route selection читает их).

## Что этап отдаёт наружу

Capability metadata провайдеров, разделение route selection и execution,
health state с TTL и circuit breaker.

## Что уже есть в коде

Есть: `RouteCandidate` с capabilities, privacy class, cost, latency и флагом
`available`; `select_route` отделён от исполнения; bounded валидация кандидатов
и запроса.

Нет: health state с TTL, circuit breaker и last failure category — сейчас
доступность это статический `available: bool` на кандидате; нет Core-owned
immutable snapshot fallback policy на запуск задачи.

### Контракт snapshot и health

Перед началом run Core один раз создаёт `RoutePolicySnapshot` и передаёт его
по ссылке всем попыткам этого run. Snapshot содержит версию policy, immutable
список candidates/capabilities, privacy/approval/tool/sandbox policy,
`HealthSnapshot` (статус, observed-at, TTL, circuit state и last failure
category), user preference и `BudgetSnapshot`. Renderer не может изменить ни
один из этих полей. Изменения health после создания влияют только на следующий
run.

Circuit breaker открывается для timeout, connection/refused, 5xx и malformed
provider response; 429 имеет отдельный cooldown и открывает circuit только
после порога повторов. Ошибки policy/approval, invalid request и cancellation
не открывают circuit. TTL и пороги задаются конфигурацией и попадают в trace.

## Содержание

- Ввести capability metadata: tool calling, structured output, context limit,
  streaming, vision, local/cloud, privacy boundary.
- Разделить route selection и provider execution.
- Health state должен иметь TTL, circuit breaker и last failure category.
- Fallback policy должна быть Core-owned immutable snapshot на запуск задачи.

## Проверки

- provider capability contract tests;
- health state истекает по TTL и открывает circuit breaker по категории отказа;
- snapshot fallback policy не меняется в течение запуска задачи.

## Критерии готовности

- решение route воспроизводимо по snapshot;
- capability провайдера нельзя переопределить из renderer;
- snapshot policy и категории circuit breaker определены и протестированы.
