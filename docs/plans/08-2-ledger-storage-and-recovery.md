# 08-2 — SQLite storage и recovery

## Цель

Сделать публикацию execution events и action state атомарной, сохранить
совместимость с текущей SQLite schema и корректно закрывать незавершённые
операции после рестарта supervisor/Core.

## Изменения

1. Аддитивно расширить существующий `events` journal полями typed schema,
   `event_id`, `run_id`, `session_id` и correlation indexes; глобальный
   `sequence_id` оставить канонической sequence.
2. Миграцию выполнять транзакционно с существующим backup/rollback механизмом;
   legacy rows сохранить и обозначить как legacy events.
3. Ввести Core-owned action projection с idempotency key и terminal guard.
4. Записывать event и projection transition одной SQLite-транзакцией; при
   ошибке не должно оставаться половины перехода.
5. После restart находить actions без terminal receipt/failure, публиковать
   typed `supervisor_restart`/`unknown_outcome`, переводить состояние в `stuck`
   и запрещать автоматический повтор side effect.
6. Исправления и recovery decisions публиковать отдельными immutable events;
   ранее опубликованные записи не изменять.

## Проверки

- migration из текущей schema v28 и сохранение legacy rows;
- atomic write rollback при ошибке SQLite;
- duplicate request/idempotency tests;
- crash/reopen tests для running, approval и cancellation;
- supervisor restart без повторного tool/provider side effect;
- retention/redaction tests для action metadata и typed failures.

## Готово, когда

Открытие базы после сбоя восстанавливает только подтверждённые состояния,
не скрывает unknown outcome и не допускает повторного выполнения уже
идентифицированного side effect.
