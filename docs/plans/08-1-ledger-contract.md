# 08-1 — Versioned execution ledger contract

## Цель

Зафиксировать машинно проверяемый контракт событий выполнения и корреляцию
между action, tool call, observation и terminal outcome.

## Изменения

1. Ввести `ExecutionEventV1` с полями `schema_version`, `event_id`, `run_id`,
   `session_id`, `task_id`, `sequence`, `event_type`, `state`, timestamp,
   correlation IDs и bounded redaction metadata.
2. Описать typed variants: `ActionRequest`, `ToolCall`, `Observation`,
   `ToolReceipt`, `TypedFailure` и state transition.
3. Зафиксировать состояния `running`, `paused`,
   `waiting_for_confirmation`, `finished`, `error` и `stuck`.
4. Связать `action_id ↔ tool_call_id ↔ observation_id ↔ receipt/failure_id`.
5. Включить provider/model response ID, error class и artifact references без
   сохранения raw secret, полного prompt или неограниченного tool output.
6. Сохранить signed `receipts_v1` как отдельный integrity/audit layer; typed
   execution receipt только ссылается на него и не дублирует его контракт.

## Интерфейсы

- Rust-типы и serde contract tests в Core/storage crates;
- canonical schema/fixtures для typed payloads;
- additive protobuf `ExecutionEvent` projection без смены IPC major;
- legacy event mapping для старых записей без потери sequence.

## Проверки

- round-trip и canonical serialization для каждого варианта;
- bounds на IDs, payload, error и artifact references;
- отсутствие секретов/PII/raw output после redaction;
- отрицательные тесты на неизвестный тип, неверную связь и превышение размера.

## Готово, когда

Любое событие однозначно принадлежит run/session, имеет устойчивый ID и
versioned typed body, а terminal action нельзя представить одновременно
успешным и ошибочным.
