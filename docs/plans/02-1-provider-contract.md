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

Для текущего run Core ведёт отдельный `RunHealthOverlay`: после каждого
provider result он атомарно обновляет наблюдение и может прекратить запрещённую
попытку, но не добавляет candidate в immutable snapshot. Изменения policy и
capabilities влияют только на следующий run.

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
не открывают circuit. При переходе circuit в `open` текущий retry-loop сразу
прекращается, в trace пишется `circuit_opened_during_run`, затем выбирается
следующий route из snapshot. TTL, пороги, cooldown и лимит попыток задаются
Core-конфигурацией и попадают в trace.

Capability metadata имеет `schema_version`, `provider_version` и
`capability_epoch`. Startup probe — bounded authenticated request, который
проверяет structured output, tool-call schema, context limit, streaming и
privacy boundary. Неподдерживаемая версия не получает `ready`.

Минимальная сериализация `RoutePolicySnapshot` содержит schema/policy version,
run id, candidates с route id и capability epoch, health state/observed-at/TTL/
circuit/last failure, policy hashes, user preference и budget id. Prompt,
secrets и raw output не входят; unknown fields отклоняются, а round-trip hash
используется для replay.

## Содержание

- Ввести capability metadata: tool calling, structured output, context limit,
  streaming, vision, local/cloud, privacy boundary.
- Разделить route selection и provider execution.
- Health state должен иметь TTL, circuit breaker и last failure category.
- Fallback policy должна быть Core-owned immutable snapshot на запуск задачи.

## Проверки

- provider capability contract tests;
- health state истекает по TTL и открывает circuit breaker по категории отказа;
- snapshot fallback policy не меняется в течение запуска задачи;
- startup probe отклоняет несовместимый capability schema;
- circuit breaker прекращает retry-loop внутри того же run;
- сериализация snapshot не содержит секретов и воспроизводима по hash.

## Критерии готовности

- решение route воспроизводимо по snapshot;
- capability провайдера нельзя переопределить из renderer;
- snapshot policy и категории circuit breaker определены и протестированы.
