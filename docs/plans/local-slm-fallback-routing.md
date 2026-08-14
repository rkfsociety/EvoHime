# План: Локальный SLM fallback и routing

Статус: draft для ревью.

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

### 1. Provider contract

- Ввести capability metadata: tool calling, structured output, context limit,
  streaming, vision, local/cloud, privacy boundary.
- Разделить route selection и provider execution.
- Health state должен иметь TTL, circuit breaker и last failure category.
- Fallback policy должна быть Core-owned immutable snapshot на запуск задачи.

### 2. Local provider

- Добавить OpenAI-compatible local endpoint adapter с loopback-only policy.
- Проверять model capabilities при startup; malformed tool calls не считать
  успешным fallback.
- Поддержать graceful absence: если local model не установлена, сообщать
  `unavailable`, не маскировать это как provider success.
- Ограничить local process/resource lifetime supervisor policy.

### 3. Routing и budget

- Добавить trace decision: candidates, selected route, reason, privacy label,
  fallback count и budget snapshot.
- На simple requests использовать small route только после evaluation gate.
- Не отправлять cloud route, если classification не завершена или secrets
  не прошли redaction.
- При повторных fallback остановиться по run budget и запросить пользователя.

### 4. UI

- Показывать фактическую модель/route, а не только желаемую.
- Отдельно отображать `cloud unavailable`, `local unavailable` и `route denied`.
- Разрешить пользователю выбрать preferred route, но не обходить privacy и
  approval policy.

## Проверки

- route matrix по privacy, complexity, offline и health;
- provider capability contract и malformed tool-call tests;
- cloud timeout → local fallback → final truthful status;
- local unavailable → bounded refusal;
- no-secret-on-cloud test;
- streaming/cancellation/resource cleanup;
- evaluation сравнивает quality floor small vs large model;
- Windows loopback and supervisor lifecycle tests.

## Критерии готовности

- решение route воспроизводимо по snapshot и trace;
- sensitive data никогда не уходит в запрещённый provider;
- fallback не меняет tool permissions, approval или sandbox;
- UI показывает фактический результат routing;
- cloud outage оставляет usable local degraded mode, если он настроен.

## Зависимости

Нужны Evaluation catalog, Context Budget Manager и provider health model.
Поддержка конкретной SLM/launcher выбирается отдельным ADR после проверки
Windows resource requirements; этот план не фиксирует бренд модели.
