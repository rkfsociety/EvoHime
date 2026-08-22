# 08-2 — SQLite storage и recovery

## Цель

Сделать публикацию execution events и action state атомарной, сохранить
совместимость с текущей SQLite schema v29 и корректно классифицировать
незавершённые операции после рестарта supervisor/Core.

## Зависимости

### Блокирующие

- [08-1](08-1-ledger-contract.md): typed contract, словарь состояний и
  correlation links;
- `LocalDatabase` (schema v29, `LEGACY_SCHEMA_VERSION = 26`) и её
  transactional migration/backup path;
- workflow storage (`workflow_runs`, `workflow_run_nodes`,
  `workflow_node_attempts`, `workflow_run_events`) и recovery-механика
  `run_effects`/`run_leases`/`run_recovery`/`run_reconciliations`;
- `receipts_v1` storage и model provenance storage как отдельные владельцы
  своих таблиц и internal schema versions.

### Опциональные

- retention/compaction политика из 12-x: до её появления действует текущая
  bounded retention журнала, а `earliest_available_sequence` остаётся
  единственным контрактом на границу replay;
- telemetry из 07-4: без неё recovery-классификация проверяется собственными
  fixtures, а не общим harness.

## Что уже есть в коде

- `events` пишется через `append_event`, глобальная `sequence_id` уже
  канонична;
- `run_effects` уже хранит `idempotency_key`, `immutable_intent_hash`, `state`,
  `started_at`/`completed_at` — это и есть существующий dispatch marker;
- `run_recovery` уже хранит decision/verifier/evidence по `effect_id`;
- workflow runtime уже классифицирует прерывания как `interrupted` и
  `unknown_outcome` и запрещает слепой повтор.

Нет атомарной публикации «ledger event + переход проекции» в одной транзакции,
нет `event_id`/`run_id`/`session_id` в `events` и нет обратной ссылки
projection → глобальное событие.

## Изменения

1. Аддитивно расширить существующий `events` journal полями typed schema,
   `event_id`, `run_id`, `session_id` и correlation indexes; глобальный
   `sequence_id` оставить канонической sequence. Новые typed rows валидировать
   до commit, legacy columns/rows не перезаписывать.
2. Поднять storage schema с v29 до v30 через штатный transactional
   migration/backup path. Тесты должны открыть fixtures минимально
   поддерживаемой v26 и текущей v29, сохранить legacy rows и доказать
   idempotent reopen; база ниже v26 по-прежнему отклоняется, а internal schema
   versions receipts и provenance не смешиваются с `PRAGMA user_version`.
3. Зафиксировать ownership matrix вместо второй action-базы:
   `events` — глобальный ledger, `receipt_actions`/`receipt_records` — signed
   tool/approval audit, `workflow_runs`/`workflow_run_nodes`/
   `workflow_node_attempts` — workflow state, `workflow_run_events` —
   per-run projection, `run_effects` — dispatch marker и idempotency,
   `run_leases`/`run_recovery`/`run_reconciliations` — recovery ownership.
   Каждая projection row получает `event_id` или `ledger_sequence_id`; новая
   общая таблица допускается только при явной ссылке на этих владельцев и без
   второго terminal guard.
4. Предоставить Core-owned storage operation, которая в одной SQLite
   transaction публикует ledger event, соответствующий projection transition
   и correlation link. Нельзя составлять atomic operation из нескольких
   публичных методов, каждый из которых сам делает commit.
5. При startup публиковать один bounded `supervisor_restart`/`core_start`
   event на новое Core instance и классифицировать незавершённые действия по
   уже существующему признаку — наличию dispatch marker в `run_effects`:
   pre-dispatch без marker → `interrupted` с безопасным resumable решением по
   контракту; marker/started без terminal outcome → `unknown_outcome` +
   blocked automatic retry и read-only reconciliation; pending approval →
   `waiting_approval` с обычным TTL; уже terminal action не трогать.
   `dead_letter` применять только по явному bounded recovery rule, а не ко всем
   открытым строкам. Reconciliation создаёт отдельное read-only
   action/decision и никогда не переписывает исходный `unknown_outcome` вторым
   terminal outcome.
6. Исправления и recovery decisions публиковать отдельными immutable events;
   ранее опубликованные записи не изменять. Retention/compaction не должны
   ломать `earliest_available_sequence`: перед удалением сохраняется bounded
   snapshot/marker, а replay gap становится явным и проверяемым.

## Проверки

- migration v26→v30 и v29→v30 с сохранением legacy rows;
- atomic write rollback при ошибке SQLite;
- duplicate request/idempotency tests поверх существующего
  `run_effects.idempotency_key`;
- crash/reopen tests для pre-dispatch, running-after-dispatch, approval и
  cancellation;
- supervisor restart без повторного tool/provider side effect и с различием
  `interrupted`/`unknown_outcome`;
- atomic linkage workflow projection ↔ global `event_id`/`ledger_sequence_id`;
- `dead_letter` не назначается вне явного recovery rule;
- retention/redaction tests для action metadata и typed failures.

## Готово, когда

Открытие базы после сбоя восстанавливает только подтверждённые или явно
разрешённые контрактом resumable состояния, не скрывает `unknown_outcome`,
сохраняет audit linkage и не допускает повторного выполнения уже
идентифицированного side effect.
