# Этап 03.1: Typed contracts

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этап 01.3 — correlation id связывает tool call ребёнка с receipt
и approval родителя; существующие child runtime и IPC/storage.

Из списка блокирующих зависимостей плана этому этапу нужен только 01.3,
поэтому его можно начать раньше остальных этапов плана.

Разблокирует: все остальные этапы плана 03.

## Что этап отдаёт наружу

Typed input/output контракт child task и сквозные correlation ids.

## Что уже есть в коде

Есть: `ChildTaskRequest` с `role`, `reduced_context`, `max_output_bytes` и
`requested_capabilities`, валидация report до persistence, отказ вложенным
детям и не-read-only capabilities.

Нет: workspace/path grants, token/time/tool-call budget, явной input/output
schema, parent sequence и correlation id на receipt (последний приходит из
этапа 01.3).

## Содержание

- Расширить existing child IPC/storage additive-полями role, grants, budget,
  input/output schema и parent sequence.
- Валидировать report schema до persistence и fan-in.
- Добавить correlation ids для task, child, tool call и receipt.

## Проверки

- malformed report, oversized report и wrong parent id отклоняются до
  persistence;
- role permission matrix и negative tests;
- child cannot commit/push without parent policy and approval.

## Критерии готовности

- каждый child имеет typed input/output и отдельный budget;
- child не расширяет права родителя и не обходит approval.
