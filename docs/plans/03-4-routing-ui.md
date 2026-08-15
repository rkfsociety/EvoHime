# Этап 03.4: UI

Этап плана [03 Локальный SLM fallback и routing](03-0-local-slm-fallback-routing.md).

## Зависимости

Блокирующие: этап 03.3 — UI показывает его trace.

Это последний этап плана.

## Что этап отдаёт наружу

Честное отображение фактического route и причин отказа.

## Содержание

- Показывать фактическую модель/route, а не только желаемую.
- Отдельно отображать `cloud unavailable`, `local unavailable` и `route denied`.
- Разрешить пользователю выбрать preferred route, но не обходить privacy и
  approval policy.

## Проверки

- три состояния отказа различимы в UI и не сливаются в одно сообщение;
- выбранный пользователем preferred route не обходит privacy и approval policy;
- UI показывает фактический, а не желаемый route после fallback.

## Критерии готовности

- UI показывает фактический результат routing;
- cloud outage оставляет usable local degraded mode, если он настроен.
