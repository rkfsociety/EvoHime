# 01. Журнал выполнения и typed receipts

## Цель

Зафиксировать единый Core-owned журнал выполнения, чтобы UI, supervisor,
diagnostics, memory и evaluation видели одну воспроизводимую историю, а не
разрозненное mutable state.

## Scope

- `run_id`, `session_id`, `event_id` и монотонная sequence;
- типизированные `ActionRequest`, `ToolCall`, `Observation`, `ToolReceipt`;
- связи `action_id ↔ tool_call_id ↔ observation`;
- provider/model response ID, error class и artifact references;
- durable events отдельно от временных streaming deltas;
- состояния `running`, `paused`, `waiting_for_confirmation`, `finished`,
  `error`, `stuck`;
- атомарная запись и replay после reconnect.

## Не входит

- UI как источник durable state;
- произвольный JSON без versioned schema;
- разрешение опасных действий только на основании записи модели;
- новая база данных вне существующего Core/SQLite слоя.

## Требования

- События неизменяемы после публикации; исправления идут отдельным событием.
- Каждая операция имеет начало, результат или typed failure.
- Timeout, cancellation, rejection и supervisor restart видны в журнале.
- Streaming delta не подменяет durable result.
- Replay возвращает события в исходном порядке и явно сообщает о gap/stale
  Core revision.
- Чувствительный output redacted до сохранения и до передачи в UI.

## Тестовый контур

- schema/serialization contract tests;
- порядок и отсутствие дублей при replay;
- reconnect во время running, approval и cancellation;
- atomic write при ошибке SQLite;
- supervisor restart с восстановлением последнего состояния;
- typed mapping для timeout, rejection, provider failure и unknown result.

## Критерии готовности

- Core и SQLite остаются единственным durable source of truth;
- у каждого action есть связанный receipt или typed failure;
- sequence replay работает после reconnect;
- renderer получает projection и не может изменить журнал напрямую;
- миграции, rollback и tests покрывают новую схему.

## Зависимости

Нет. Этот раздел — первый блокирующий этап и основа для всех остальных.
