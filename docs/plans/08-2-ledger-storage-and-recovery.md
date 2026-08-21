# 08-2 — SQLite storage и recovery

## Цель

Сделать публикацию execution events и action state атомарной, сохранить
совместимость с текущей SQLite schema v29 и корректно классифицировать
незавершённые операции после рестарта supervisor/Core.

## Изменения

1. Аддитивно расширить существующий `events` journal полями typed schema,
   `event_id`, `run_id`, `session_id` и correlation indexes; глобальный
   `sequence_id` оставить канонической sequence. Новые typed rows валидировать
   до commit, legacy columns/rows не перезаписывать.
2. Поднять storage schema с v29 до v30 через штатный transactional
   migration/backup path. Тесты должны открыть минимум v29 fixture, сохранить
   legacy rows и доказать idempotent reopen; internal schema versions receipts
   и provenance не смешивать с `PRAGMA user_version`.
3. Зафиксировать ownership matrix вместо второй action-базы:
   `events` — глобальный ledger, `receipt_actions`/`receipt_records` — signed
   tool/approval audit, `workflow_runs`/`workflow_run_nodes`/
   `workflow_node_attempts` — workflow state, `workflow_run_events` —
   per-run projection. Каждая projection row получает `event_id` или
   `ledger_sequence_id`; новая общая таблица допускается только при явной
   ссылке на эти владельцы и без второго terminal guard.
4. Предоставить Core-owned storage operation, которая в одной SQLite
   transaction публикует ledger event, соответствующий projection transition
   и correlation link. Нельзя составлять atomic operation из нескольких
   публичных методов, каждый из которых сам делает commit.
5. При startup публиковать один bounded `supervisor_restart`/`core_start`
   event на новое Core instance и классифицировать незавершённые действия:
   pre-dispatch без dispatch marker → `interrupted` с безопасным resumable
   решением по контракту; dispatch marker/started без terminal outcome →
   `unknown_outcome` + blocked automatic retry и read-only reconciliation;
   pending approval → `waiting_for_confirmation` с обычным TTL; уже terminal
   action не трогать. `stuck`/`dead_letter` применять только по явному
   bounded recovery rule, а не ко всем открытым строкам. Reconciliation создаёт
   отдельное read-only action/decision и никогда не переписывает исходный
   `unknown_outcome` вторым terminal outcome.
6. Исправления и recovery decisions публиковать отдельными immutable events;
   ранее опубликованные записи не изменять. Retention/compaction не должны
   ломать `earliest_available_sequence`: перед удалением сохраняется bounded
   snapshot/marker, а replay gap становится явным и проверяемым.

## Проверки

- migration из текущей schema v29 в v30 и сохранение legacy rows;
- atomic write rollback при ошибке SQLite;
- duplicate request/idempotency tests;
- crash/reopen tests для pre-dispatch, running-after-dispatch, approval и
  cancellation;
- supervisor restart без повторного tool/provider side effect и с различием
  `interrupted`/`unknown_outcome`;
- atomic linkage workflow projection ↔ global `event_id`/`ledger_sequence_id`;
- retention/redaction tests для action metadata и typed failures.

## Готово, когда

Открытие базы после сбоя восстанавливает только подтверждённые или явно
разрешённые контрактом resumable состояния, не скрывает `unknown_outcome`,
сохраняет audit linkage и не допускает повторного выполнения уже
идентифицированного side effect.
