# Этап 03.2: Local provider

Этап плана [03 Локальный SLM fallback и routing](03-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 03.1 — локальный провайдер объявляет capabilities по общему
контракту. Context Budget Manager здесь не требуется, поэтому этап можно вести параллельно
с планом 01.

Разблокирует: 03.3 — без локального route падать некуда.

## Что этап отдаёт наружу

Loopback-only local route с честным статусом доступности.

## Что уже есть в коде

Ничего. В `crates/model-gateway/src/providers/` есть только `literouter`,
`openai_compatible` и `mock`. Строка `local_route_unavailable` в
`routing_runtime.rs` — это причина отказа в политике, а не реализованный
локальный провайдер: падать сейчас некуда.

## Содержание

- Добавить OpenAI-compatible local endpoint adapter с loopback-only policy.
- Проверять model capabilities при startup; malformed tool calls не считать
  успешным fallback.
- Поддержать graceful absence: если local model не установлена, сообщать
  `unavailable`, не маскировать это как provider success.
- Ограничить local process/resource lifetime supervisor policy.

## Проверки

- malformed tool-call tests: такой ответ не засчитывается как успешный fallback;
- local unavailable → bounded refusal, а не маскировка под успех;
- Windows loopback and supervisor lifecycle tests;
- streaming/cancellation/resource cleanup.

## Критерии готовности

- отсутствие локальной модели видно как `unavailable`;
- local route не выходит за loopback и подчиняется supervisor lifetime.
