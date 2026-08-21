# 08-1 — Versioned execution ledger contract

## Цель

Зафиксировать машинно проверяемый контракт событий выполнения и корреляцию
между action, tool call, observation и terminal outcome.

## Изменения

1. Ввести `ExecutionEventV1` с bounded полями `schema_version`, `event_id`,
   `sequence_id`, `run_id`, `session_id`, `task_id`, `event_type`, timestamp,
   `state_after`, correlation IDs и redaction metadata. Для новых событий
   `run_id`, `session_id` и `event_id` обязательны; для глобальных lifecycle/
   recovery events применяется зарезервированный `system` scope, а не
   фиктивный пользовательский run. Legacy rows получают отдельный `legacy`
   scope и не маскируются под новый run. `session_id` здесь означает логическую
   execution session, а не transport generation из `session_epoch`.
2. Описать typed variants: `ActionRequest`, `ToolCall`, `Observation`,
   `ToolReceipt`, `TypedFailure`, `ApprovalDecision`, `Cancellation` и
   `RecoveryDecision`. `sequence_id` — единственная глобальная durable
   sequence; `workflow_run_events.run_sequence` в неё не переименовывается.
3. Зафиксировать state machine отдельно от event type: обычные состояния
   `pending`, `running`, `paused`, `waiting_for_confirmation` и
   `cancelling`; terminal outcomes `succeeded`, `failed`, `cancelled`,
   `refused` и `unknown_outcome`. `unknown_outcome` запрещает автоматический
   повтор, но остаётся reconciliation-required; `stuck`/`dead_letter` —
   bounded recovery projection, а не скрытый terminal success/failure.
4. Связать `action_id ↔ tool_call_id ↔ observation_id ↔ receipt_id` или
   `failure_id`; для approval, model request и workflow attempt определить
   отдельные optional links и запретить ссылку на другой `run_id`/`session_id`.
5. Включить только bounded provider/model response IDs, error class и
   content-addressed artifact references. Полный prompt, headers, tokens,
   credentials и raw tool output не сохраняются; для секретов допустимы
   только presence/classification и keyed digest, пригодный для сравнения,
   но не для offline dictionary lookup.
6. Сохранить signed `receipts_v1` как отдельный integrity/audit layer. Typed
   `ToolReceipt` содержит `receipt_action_id` и `receipt_hash`, ссылается на
   существующую запись и не дублирует её canonical envelope.
7. Для legacy `events` зафиксировать детерминированный mapping: `event_id` —
   domain-separated digest исходных `sequence_id`, `task_id`, `event_type`,
   `payload` и `created_at`; исходная payload не переписывается, а typed
   projection помечается `legacy` и сохраняет исходную sequence.

## Интерфейсы

- Rust-типы и serde contract tests в Core/storage crates;
- canonical schema/fixtures для typed payloads и таблица допустимых state
  transitions;
- additive protobuf `ExecutionEvent` projection внутри `EventEnvelope` без
  смены IPC major; старый клиент обязан безопасно игнорировать новый oneof;
- legacy event mapping для старых записей без потери `sequence_id` и без
  попытки восстановить отсутствующие correlation IDs.

## Проверки

- round-trip и canonical serialization для каждого варианта;
- bounds на IDs, payload, error, metadata и artifact references; лимиты должны
  быть общими для storage, protobuf и Electron adapter;
- отсутствие секретов/PII/raw output после redaction;
- отрицательные тесты на неизвестный тип, неверную связь, illegal transition,
  два terminal outcomes и превышение размера.

## Готово, когда

Любое новое событие однозначно принадлежит run/session, имеет устойчивый ID и
versioned typed body; legacy mapping воспроизводим; terminal action нельзя
представить одновременно с двумя исходами или с outcome, не связанным с его
immutable receipt/failure/recovery decision.
