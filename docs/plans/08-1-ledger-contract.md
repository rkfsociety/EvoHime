# 08-1 — Versioned execution ledger contract

## Цель

Зафиксировать машинно проверяемый контракт событий выполнения и корреляцию
между action, tool call, observation и terminal outcome.

## Зависимости

### Блокирующие

- [08-0](08-0-execution-ledger.md);
- текущий `events` journal и `EventJournal`, канонический `sequence_id`;
- существующий словарь состояний `RunState`/`NodeState`
  (`crates/evohime-local-storage/src/workflow_store.rs`) и CHECK-ограничения
  `workflow_runs.state`/`workflow_run_nodes.state`, с которыми контракт обязан
  совпадать по именам, а не вводить синонимы;
- `receipts_v1` (`receipt_actions`, `receipt_records`) и model-request
  provenance как immutable-слои, на которые ссылается typed event;
- текущий `EventEnvelope` в `crates/desktop-ipc/proto/evohime.desktop.proto`
  как место additive-проекции.

### Опциональные

- `tool/manifest/v1` из 07-1: при наличии typed `ToolCall` несёт
  `tool_id`/`version`/manifest hash. До его появления используется текущая
  identity из `ToolRegistry` (имя инструмента и exact-call hash), поле
  manifest hash остаётся пустым и не проверяется;
- capability snapshot из 09-1: до его появления `ApprovalDecision` ссылается на
  существующий approval intent, а поле snapshot hash отсутствует.

## Что уже есть в коде

- `events` хранит только `sequence_id`, `task_id`, `event_type`, `payload`,
  `created_at`: нет устойчивого `event_id`, `run_id`, `session_id` и typed
  body, а `payload` — произвольный BLOB без versioned схемы;
- состояния и терминальность уже описаны в `RunState`/`NodeState`, включая
  `waiting_approval`, `unknown_outcome` и `dead_letter`;
- `interrupted`/`unknown_outcome` уже используются в model provenance;
- signed receipt уже связывает action и подпись через `receipt_actions`.

Нет единого typed события, нет correlation IDs в глобальном журнале и нет
запрета на два terminal outcomes для одного action.

## Изменения

1. Ввести `ExecutionEventV1` с bounded полями `schema_version`, `event_id`,
   `sequence_id`, `run_id`, `session_id`, `task_id`, `event_type`, timestamp,
   `state_after`, correlation IDs и redaction metadata. Для новых событий
   `run_id`, `session_id` и `event_id` обязательны; для глобальных lifecycle/
   recovery events применяется зарезервированный `system` scope, а не
   фиктивный пользовательский run. Legacy rows получают отдельный `legacy`
   scope и не маскируются под новый run. `session_id` здесь означает логическую
   execution session, а не transport generation из `session_epoch`.
2. Зафиксировать `run_id` как ссылку на существующего владельца, а не как новое
   пространство имён. Поскольку в checkout уже есть два непересечённых по
   смыслу ключа — `runs.id` и `workflow_runs.run_id` — ledger хранит пару
   (`run_scope`, `run_id`), где `run_scope` ∈ {`workflow`, `work_item`,
   `standalone`, `system`, `legacy`}. Для `workflow` значение равно
   `workflow_runs.run_id`, для `work_item` — `runs.id`; уникальность и запрет
   cross-link проверяются по паре, а не по одному `run_id`.
3. Описать typed variants: `ActionRequest`, `ToolCall`, `Observation`,
   `ToolReceipt`, `TypedFailure`, `ApprovalDecision`, `Cancellation` и
   `RecoveryDecision`. `sequence_id` — единственная глобальная durable
   sequence; `workflow_run_events.run_sequence` в неё не переименовывается.
4. Зафиксировать state machine отдельно от event type, переиспользуя имена
   существующего `NodeState`; `state_after` в ledger — именно action-level
   состояние. Нетерминальные: `pending`, `ready`, `running`,
   `waiting_approval` и новое `cancelling`. Терминальные: `succeeded`,
   `failed`, `timed_out`, `cancelled`, `denied`, `blocked`, `skipped`,
   `degraded`, `unknown_outcome` и `dead_letter`. `unknown_outcome` запрещает
   автоматический повтор, но остаётся reconciliation-required; `dead_letter`
   назначается только явным bounded recovery rule и не считается скрытым
   success/failure. Новое состояние `cancelling` добавляется в `NodeState`, в
   таблицу допустимых переходов и в CHECK-ограничение
   `workflow_run_nodes.state` (миграция — в 08-2), а не живёт только в ledger.
   Run-level `RunState` (`completed`, `interrupted` и остальные) остаётся
   отдельным множеством: ledger не смешивает его с action-level состояниями, а
   фиксирует явный mapping action → run (например `succeeded` → `completed`,
   pre-dispatch restart → `interrupted` на уровне run).
5. Связать `action_id ↔ tool_call_id ↔ observation_id ↔ receipt_id` или
   `failure_id`; для approval, model request и workflow attempt определить
   отдельные optional links (`workflow_run_id`, `node_id`, `attempt_id`,
   `effect_id`, `model_request_id`) и запретить ссылку на другой
   `run_id`/`session_id`.
6. Включить только bounded provider/model response IDs, error class и
   content-addressed artifact references. Полный prompt, headers, tokens,
   credentials и raw tool output не сохраняются; для секретов допустимы
   только presence/classification и keyed digest, пригодный для сравнения,
   но не для offline dictionary lookup.
7. Сохранить signed `receipts_v1` как отдельный integrity/audit layer. Typed
   `ToolReceipt` содержит `receipt_action_id` и `receipt_hash`, ссылается на
   существующую запись и не дублирует её canonical envelope.
8. Для legacy `events` зафиксировать детерминированный mapping: `event_id` —
   domain-separated digest исходных `sequence_id`, `task_id`, `event_type`,
   `payload` и `created_at`; исходная payload не переписывается, а typed
   projection помечается `legacy` и сохраняет исходную sequence.

## Интерфейсы

- Rust-типы и serde contract tests в Core/storage crates;
- canonical schema/fixtures для typed payloads и таблица допустимых state
  transitions, общая для ledger и `workflow_store`;
- additive protobuf `ExecutionEvent` projection внутри `EventEnvelope` без
  смены IPC major; новый oneof arm занимает свободный номер (текущие 10–13
  заняты `ready`/`replay_gap`/`full_snapshot`/`auth_challenge`), старый клиент
  обязан безопасно игнорировать неизвестный вариант;
- legacy event mapping для старых записей без потери `sequence_id` и без
  попытки восстановить отсутствующие correlation IDs.

## Проверки

- round-trip и canonical serialization для каждого варианта;
- bounds на IDs, payload, error, metadata и artifact references; лимиты должны
  быть общими для storage, protobuf и Electron adapter;
- отсутствие секретов/PII/raw output после redaction;
- совпадение множества action-состояний ledger и `NodeState` (и их CHECK-списка
  в SQLite): тест падает, если появился state без mapping в обе стороны;
- полнота mapping action-состояний в `RunState` и обратного покрытия run-level
  состояний;
- отказ при ссылке на `run_id` с несовместимым `run_scope`;
- отрицательные тесты на неизвестный тип, неверную связь, illegal transition,
  два terminal outcomes и превышение размера.

## Готово, когда

Любое новое событие однозначно принадлежит run/session, имеет устойчивый ID и
versioned typed body; словарь состояний един с workflow storage; legacy mapping
воспроизводим; terminal action нельзя представить одновременно с двумя исходами
или с outcome, не связанным с его immutable receipt/failure/recovery decision;
пара (`run_scope`, `run_id`) однозначно указывает на существующего владельца.
