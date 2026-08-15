# Этап 03.1: Provider contract

Этап плана [03 Локальный SLM fallback и routing](03-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: существующая provider health model. Context Budget Manager здесь не
требуется — контракт провайдера не касается бюджета, поэтому этот этап можно
вести параллельно с планом 01.

Разблокирует: 03.2 (локальный провайдер объявляет свои capabilities) и 03.3
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
- capability провайдера нельзя переопределить из renderer.
