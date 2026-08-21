# План 17.1. Реестр зависимостей и решений

## Цель

Сделать открытые решения явными и не допустить реализации этапа с ошибочным
порядком или скрытой блокирующей зависимостью.

## Изменения

- Вести таблицу решений по первому vertical slice планов 08–10, расширяемым
  IPC/SQLite schemas, schema ownership и migration strategy.
- Для каждого плана 06–16 указать blocking/optional dependencies, fallback до
  появления optional компонента и минимальный release evidence.
- Зафиксировать решение о worker process для browser/voice/vision, а также
  CPU/GPU/memory/disk/latency/retention/concurrency budgets.
- Разделить обязательные для поставки features и optional capabilities;
  unsupported должен быть typed и наблюдаемым.
- Назначить владельца inventory лицензий/attribution и формат хранения decision
  record без секретов.

## Проверки

- граф не содержит циклов и блокирующей зависимости от более позднего плана;
- ссылки на планы, схемы и current state разрешаются;
- каждое optional решение имеет документированный fallback;
- изменение schema/IPC имеет owner, version и migration/rollback note.

## Готово, когда

Любое открытое решение имеет статус, owner, критерий закрытия и влияние на
поставку; порядок планов можно проверить без устного контекста.

