# Этап 03.1: Typed contracts

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: существующие child runtime и IPC/storage. Этап 01.3 нужен для
полной receipt-интеграции: до него сохраняется task/child/tool correlation, а
receipt correlation остаётся `pending`.

Из зависимостей плана 03 этот этап можно начать раньше остальных: базовые
контракты не требуют готового coordinator. Receipt-поле и соответствующие
проверки остаются gated этапом 01.3 и не считаются завершёнными до его
интеграции.

Разблокирует: все остальные этапы плана 03.

## Что этап отдаёт наружу

Typed input/output контракт child task и сквозные correlation ids.

## Что уже есть в коде

Есть: `ChildTaskRequest` с `role`, `reduced_context`, `max_output_bytes` и
`requested_capabilities`, валидация report до persistence, отказ вложенным
детям и не-read-only capabilities.

Нет: workspace/path grants, token/time/tool-call budget, явной input/output
schema, parent sequence и correlation id на receipt (последний приходит из
этапа 01.3). Базовые Context Budget Manager и Artifact Store уже существуют,
но child policy enforcement — часть этого плана.

## Содержание

- Расширить existing child IPC/storage additive-полями role, grants, budget,
  input/output schema, acceptance criteria, `max_revisions` и parent sequence.
- Ввести `contract_version` с major/minor: minor совместима при неизвестных
  additive-полях, major отклоняется до явной миграции.
- Валидировать report schema до persistence и fan-in.
- Добавить correlation ids для task, child, tool call и receipt.
- В provenance включить input/evidence hashes, версии tool/schema, model IDs,
  timestamps и parent sequence; проверять их перед persistence.
- Проверять, что каждый child grant является подмножеством parent grant, и
  передавать grants в Core tool policy на каждый вызов.

## Проверки

- malformed report, oversized report и wrong parent id отклоняются до
  persistence;
- role permission matrix и negative tests;
- child cannot commit/push without parent policy and approval.
- grant/path/capability escalation и stale provenance отклоняются;
- parent sequence монотонен в пределах parent task и однозначно упорядочивает
  fan-in.

## Критерии готовности

- каждый child имеет typed input/output и отдельный budget;
- child не расширяет права родителя и не обходит approval.
