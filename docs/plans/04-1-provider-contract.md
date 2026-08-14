# Этап 04.1: Provider contract

Этап плана [04 Локальный SLM fallback и routing](04-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: существующая provider health model. Этапа 01.1 здесь не
требуется — контракт провайдера не касается бюджета, поэтому этот этап можно
вести параллельно с планом 01.

Разблокирует: 04.2 (локальный провайдер объявляет свои capabilities) и 04.3
(route selection читает их).

## Что этап отдаёт наружу

Capability metadata провайдеров, разделение route selection и execution,
health state с TTL и circuit breaker.

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
