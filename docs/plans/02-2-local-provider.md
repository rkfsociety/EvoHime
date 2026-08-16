# Этап 02.2: Local provider

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 02.1 — локальный провайдер объявляет capabilities по общему
контракту. Context Budget Manager здесь не требуется: локальный адаптер не
касается бюджета.

Разблокирует: 02.3 — без локального route падать некуда.

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

Supervisor остаётся владельцем процесса local provider: Core отправляет ему
валидированный launch/stop request, а supervisor создаёт процесс только с
loopback bind address, Job Object, timeout, memory/CPU limits и cancellation.
Core не принимает произвольный порт или command line от renderer. Статусы
`starting`, `ready`, `unavailable`, `degraded` и `stopped` публикуются через
provider health contract.

Если local model отсутствует, повреждена, не прошла capability probe или
запущена не на loopback, route получает `unavailable` с безопасной причиной.
Для sensitive/offline задачи это окончательный bounded refusal с указанием
действия пользователя; cloud не пробуется. Для non-sensitive задачи разрешён
только явно предусмотренный cloud fallback.

## Проверки

- malformed tool-call tests: такой ответ не засчитывается как успешный fallback;
- local unavailable → bounded refusal, а не маскировка под успех;
- Windows loopback and supervisor lifecycle tests;
- streaming/cancellation/resource cleanup.

## Критерии готовности

- отсутствие локальной модели видно как `unavailable`;
- local route не выходит за loopback и подчиняется supervisor lifetime;
- sensitive/offline задача при недоступной local model завершается truthful refusal.
