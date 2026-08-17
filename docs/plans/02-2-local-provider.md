# Этап 02.2: Local provider

Этап плана [02 Локальный SLM fallback и routing](02-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 02.1 — локальный провайдер объявляет capabilities по общему
контракту. Context Budget Manager здесь не требуется: локальный адаптер не
касается бюджета.

Опциональные: 02.3 routing — до его реализации адаптер вызывается только
контрактными тестами и возвращает тот же `local_route_unavailable`; cloud
fallback не добавляется в этот этап. Authenticated Core IPC и supervisor
runtime являются уже существующими блокирующими границами проекта.

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
- Аутентифицировать каждый запрос короткоживущей provider-session capability,
  выданной supervisor через уже authenticated Core IPC; одного loopback
  недостаточно. Это opaque 32-byte random bearer token, представленный как
  `Authorization: Bearer <base64url>`, с TTL 30 секунд, audience
  `local-provider`, `request_id` и одноразовым использованием. Supervisor
  хранит только hash токена, Core передаёт его только адаптеру, адаптер
  проверяет hash, expiry, audience и request binding. JWT, ключи в renderer,
  prompt, command line, environment после запуска и логах не используются.
  Просроченный, повторно использованный или чужой токен даёт
  `provider_session_invalid`, не retry и не success.
- Проверять model capabilities при startup и перед первым запросом. Каноническая
  схема capabilities содержит `schema_version`, `model_id`, `tools` (имя,
  version, JSON Schema аргументов, `requires_approval`) и `streaming`/
  `cancellation`; неизвестная версия, дубликаты имён, отсутствующая схема или
  несоответствие allowlist дают `capability_probe_failed`.
- Валидировать tool call до отправки: обязательны строковые `id` и `name` и
  JSON-объект `arguments`; имя должно присутствовать в capabilities, arguments
  должны проходить указанную JSON Schema, а mutation tool — иметь approval
  metadata. Невалидный запрос возвращается как `tool_call_malformed` с
  семантикой HTTP 400. Невалидный ответ модели возвращается как
  `provider_malformed_response` и никогда не считается fallback success.
- Поддержать graceful absence: если local model не установлена, сообщать
  `unavailable`, не маскировать это как provider success. Различать безопасные
  причины `local_model_not_found`, `capability_probe_failed`,
  `provider_session_invalid`, `loopback_policy_violation`, `timeout`,
  `cancelled` и `provider_malformed_response`; пути, токены и командные строки
  в причине не раскрывать.
- Ограничить local process/resource lifetime supervisor policy: по умолчанию
  adapter process получает memory limit 512 MiB, CPU limit 50% одного logical
  core, connect timeout 2 s, read-idle timeout 10 s, total request timeout
  30 s и cancellation grace period 2 s. Значения приходят из typed supervisor
  config, проходят upper bounds (memory 4 GiB, total timeout 120 s) и
  фиксируются в health event; renderer не может их менять.

### Классификация и routing policy

Core принимает классификацию только из структурированных `TaskMetadata`, а не
из текста prompt: `classification: "high"` или `mode: "offline"` означает
`sensitive/offline`; отсутствие этих полей означает `non-sensitive`. При
конфликте выбирается более строгая политика. Для `sensitive/offline`
`unavailable` — окончательный bounded refusal с кодом причины и действием
пользователя; cloud route не пробуется и не вызывается скрытый retry. Для
`non-sensitive` допускается только fallback, явно разрешённый 02.3 policy,
после truthful local failure. Local adapter сам не выбирает cloud provider.

Supervisor остаётся владельцем процесса local provider: Core отправляет ему
валидированный, allowlisted launch request с model id и immutable config
profile, а supervisor сам выбирает свободный loopback-порт из диапазона
`127.0.0.1:49152-49252`, проверяет bind перед выдачей capability и создаёт
процесс в Job Object. Core не принимает произвольный порт или command line от
renderer. Launch/stop — keyed by `request_id`; stop до завершения launch
помечает request cancelled, а завершившийся launch немедленно закрывается.
Повторный stop идемпотентен и возвращает `already_cancelled`. Статусы
`starting`, `ready`, `unavailable`, `degraded` и `stopped` публикуются через
provider health contract вместе с redacted reason, request_id и durations.

Каждый запрос имеет bounded connect/read/total timeout. При timeout или
cancellation Core отправляет supervisor stop request; supervisor закрывает
pipe и завершает process tree через Job Object, а после grace period применяет
принудительное завершение. Отмена по authenticated Core IPC идемпотентна.

Если local model отсутствует, повреждена, не прошла capability probe или
запущена не на loopback, route получает `unavailable` с безопасной причиной.
Проверка loopback принимает только IPv4 `127.0.0.0/8` и IPv6 `::1`, запрещает
redirect/proxy/external DNS и тестируется по фактическим socket destinations.

## Проверки

- malformed tool-call tests: невалидный JSON, неизвестное имя, отсутствие
  `id/name/arguments`, нарушение JSON Schema и mutation без approval дают
  `tool_call_malformed`/400; malformed model response не засчитывается как
  успешный fallback;
- local unavailable → `unavailable` с каждой из безопасных причин и bounded
  refusal, а не маскировка под успех;
- Windows loopback и supervisor lifecycle: bind вне `127.0.0.0/8`/`::1`,
  внешний redirect, конфликт порта, launch→stop race и Job Object cleanup;
- token authentication: правильный token проходит, неверный/чужой/истёкший
  через 30 s/повторно использованный отвергается без retry;
- capability-version и JSON Schema probe: неизвестная версия, duplicate tool
  и неподдерживаемая capability не дают `ready`;
- streaming/cancellation: отмена во время stream завершает запрос максимум за
  5 s, публикует `cancelled`, закрывает pipe и не оставляет child process;
- timeout/resource cleanup: total timeout 30 s, подтверждённое завершение
  процесса, memory/CPU limits и идемпотентный повтор cancellation;
- routing compatibility: 02.2 регистрирует `local` через provider trait и
  сохраняет `local_route_unavailable`; только 02.3 может разрешить cloud
  fallback;
- observability: для каждого lifecycle перехода есть redacted health event
  `starting/ready/degraded/unavailable/stopped`, а секреты, prompt и command
  line отсутствуют в JSONL logs.

## Критерии готовности

- отсутствие локальной модели видно как `unavailable`;
- local route не выходит за loopback и подчиняется supervisor lifetime;
- sensitive/offline задача при недоступной local model завершается truthful
  refusal с явным кодом причины, без cloud attempt;
- non-sensitive задача использует cloud только при явном разрешении routing
  policy, а local adapter сам cloud не вызывает;
- malformed response, timeout и cancellation никогда не маскируются под
  success;
- authenticated session token, capability schema, лимиты и state/error
  contract покрыты acceptance tests, включая concurrent requests и
  launch/stop race;
- `routing_runtime.rs` подключает адаптер через существующий provider trait,
  а `local_route_unavailable` остаётся совместимым с 02.3.
