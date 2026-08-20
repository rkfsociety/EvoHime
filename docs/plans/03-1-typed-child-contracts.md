# Этап 03.1: Typed contracts

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: существующие child runtime и IPC/storage.

Этап 01.3 не блокирует этот этап целиком, он блокирует только одно поле:
`correlation.receipt_id`. Task/child/tool-call correlation, contract_version,
grants, budget и provenance (`input_hash`, `evidence_hash`, `tool_version`,
`schema_version`, `model_id`, `parent_sequence`) не зависят от 01.3 и входят в
scope 03.1 целиком. `receipt_id` остаётся `Option::None` до интеграции с
`receipt_action_id` из 01.3; до этого момента корреляция с конкретным receipt
не проверяется и не считается частью критериев готовности этого этапа.

Из зависимостей плана 03 этот этап можно начать раньше остальных: базовые
контракты не требуют готового coordinator.

Разблокирует: все остальные этапы плана 03.

## Что этап отдаёт наружу

Typed input/output контракт child task, сквозные correlation ids (кроме
receipt-привязки, см. выше) и enforcement grant/budget/provenance проверок до
persistence.

## Что уже есть в коде

`crates/evohime-core/src/child_contracts.rs` уже реализует типы этого этапа:

- `ContractVersion{major, minor}` с `is_compatible_with`/`can_accept_additive`;
- `CorrelationId`/`CorrelationContext` (task/child/tool_call/receipt id и
  `parent_sequence: u64`);
- `Grant{grant_type, scope}` и `Grant::is_subset_of`;
- `ChildBudget{max_tokens, max_time_seconds, max_tool_calls}` и
  `ChildBudget::is_within_parent`;
- `Schema{json_schema, content_type, max_bytes}`;
- `Provenance{input_hash, evidence_hash, tool_version, schema_version,
  model_id, created_at, completed_at, parent_sequence}`;
- `TypedChildTaskRequest`/`TypedChildReport` с `validate()`,
  `validate_against_request()`, `grants_are_subset_of()`,
  `budget_is_within_parent()`, `to_deterministic_json()`;
- `accept_typed_report`, `validate_grant_subset`, `validate_budget_subset`,
  типизированный `ContractError`.

Это база для описанного ниже; она не заменяет требования этого документа, а
формализует их as-code. Существующий untyped `ChildTaskRequest` (`role`,
`reduced_context`, `max_output_bytes`, `requested_capabilities`) и отказ
вложенным детям остаются нижним уровнем; `TypedChildTaskRequest` расширяет его
additive-полями.

Нет ещё: подключения `child_contracts` к `child_runtime`/IPC persistence path
(enforcement на реальном create/report flow), проверки grants на каждом Core
tool call, stale-provenance проверки, атомарного генератора
`parent_sequence`, ignore-unknown-fields miграции существующих child-задач и
audit-логирования отклонений.

## Спецификация контракта

Схема input/output — **JSON Schema** (draft 2020-12), сериализованная как
UTF-8 строка в поле `Schema.json_schema` (лимит `MAX_SCHEMA_CHARS = 4096`
символов). `Schema.content_type` описывает формат самого значения (обычно
`application/json`), `Schema.max_bytes` — верхняя граница сериализованного
значения, проверяемая до валидации по JSON Schema. В обычном inline-пути
report проходит две проверки в этом порядке: (1) `max_bytes`, (2) JSON Schema
validation `output_data` против `output_schema.json_schema`, если оно задано;
отсутствие `output_schema` не освобождает report от базовой
structural-валидации `TypedChildReport::validate()`. Этап 03.3 добавляет
явный offload-путь: включённый для конкретного child offload перехватывает
результат до inline-лимита, сохраняет его как разрешённый `ArtifactRef`, а
родителю передаёт только bounded reference/summary; без этого пути превышение
лимита по-прежнему отклоняется как `OutputTooLarge`.

`acceptance_criteria` в 03.1 — свободный текст, который формулирует
coordinator при создании child (см. [03-0](03-0-specialized-child-workflows.md#контракт-child-task)
для семантики revise/accept). Проверяемое structured-представление
acceptance criteria (JSON-path assertions) не входит в 03.1 и остаётся за
03.2, где coordinator интерпретирует `revise`/`Accepted`.

`contract_version` — `ContractVersion{major, minor}`, текущее значение
`CONTRACT_VERSION = 1.0`. Правила совместимости:

- **major-изменение** — добавление обязательного поля, удаление поля или
  изменение семантики существующего обязательного поля. Parent отклоняет
  contract с другим major с ошибкой `ContractError::VersionMismatch` (major
  добавляется к enum ниже); существующий child должен быть явно
  мигрирован, автоматической конвертации нет.
- **minor-изменение** — добавление optional-поля с safe default. Parent с
  `contract_version.minor >= child.contract_version.minor` и тем же major
  принимает contract (`can_accept_additive`); неизвестные для читателя
  optional-поля игнорируются, а не отклоняются (serde default для новых
  optional-полей, `#[serde(skip_serializing_if = "Option::is_none")]` уже
  используется во всех optional-полях `TypedChildTaskRequest`/
  `TypedChildReport`).
- Существующий `is_compatible_with`/`can_accept_additive` в коде уже
  реализует эти два правила; 03.1 добавляет только enforcement в
  create/accept path (сейчас `contract_version` заполняется, но не
  сравнивается на границе).

## Содержание

- Подключить `child_contracts::TypedChildTaskRequest`/`TypedChildReport` к
  `child_runtime` create/report path вместо прямой работы с untyped
  `ChildTaskRequest`/`ChildReport` (additive: untyped поля остаются
  подмножеством typed структуры).
- Проверять `contract_version` на границе create/accept по правилам выше;
  major mismatch отклоняется до persistence.
- Валидировать report schema (`Schema.max_bytes` + JSON Schema) до
  persistence и fan-in.
- Заполнять correlation ids для task, child и tool call; `receipt_id`
  заполняется `None` до 01.3 и не проверяется этим этапом.
- Ввести атомарный генератор `parent_sequence` на parent task (monotonic
  counter per `parent_task_id`, не per process), чтобы конкурентные children
  не получали повторяющийся или невозрастающий `parent_sequence`.
- Определить и проверять stale provenance (см. ниже) перед persistence.
- Проверять `grants_are_subset_of`/`validate_grant_subset` при создании
  child (создание отклоняется при эскалации) и повторно передавать grants в
  Core tool policy на каждом вызове tool (повторная проверка — не
  единоразовая, см. [03-0](03-0-specialized-child-workflows.md)).
- Логировать каждое отклонение report/contract (`ContractError` variant,
  `parent_task_id`, `child_task_id`, `parent_sequence`, timestamp) через
  существующий diagnostic/audit sink; raw input/output в лог не попадает.

### Stale provenance

Provenance считается stale, если выполняется любое из условий (проверка —
до persistence report):

- `provenance.completed_at` отсутствует, хотя report имеет terminal `status`
  (`Complete`/`Rejected`/`Failed`) — provenance не закрыта;
- `provenance.parent_sequence` не совпадает с `correlation.parent_sequence`
  того же report — provenance и correlation рассинхронизированы;
- `provenance.schema_version` (если задан) не совпадает по major с текущим
  `output_schema` request'а, к которому привязан report;
- `provenance.created_at > provenance.completed_at`, если оба заданы.

Обнаружение любого условия даёт `ContractError::StaleProvenance` (новый
вариант) и report отклоняется до persistence и до fan-in; child может
переотправить report с обновлённой provenance в пределах `max_revisions`.

### Grant enforcement

Проверка выполняется в двух точках:

1. **Создание child** — `validate_grant_subset(child.grants, parent.grants)`
   вызывается до persistence `TypedChildTaskRequest`; нарушение даёт
   `ContractError::GrantEscalation` (уже реализовано) и создание
   отклоняется.
2. **Каждый tool call** — Core повторно проверяет actual grant child'а
   против actual grant parent'а на момент вызова (не кэшированный на
   создании), как описано в [03-0](03-0-specialized-child-workflows.md); это
   защищает от grant drift, если parent grant сузился после создания child.

`Grant::is_subset_of` уже определяет subset как: тот же `grant_type`, и
(a) child без `scope` — всегда subset, (b) child со `scope` — subset только
если parent `scope` содержит child `scope` как prefix или равен ему.

## Проверки

- malformed report, oversized report (`max_bytes`/`MAX_OUTPUT_BYTES`) и
  wrong parent id отклоняются до persistence в inline-пути;
- role permission matrix и negative tests;
- child cannot commit/push without parent policy and approval (approval
  state machine — 03.2; здесь проверяется, что typed contract не содержит
  implicit approval);
- grant/path/capability escalation отклоняется на создании и на каждом tool
  call (drift test: parent grant сужается после создания child);
- stale provenance по каждому из четырёх условий выше отклоняется отдельным
  тестом;
- contract_version: major mismatch отклоняется, minor forward-compat
  (unknown additive-поле) принимается и игнорируется;
- parent sequence монотонен и уникален в пределах parent task под
  конкурентной нагрузкой (N children, каждый получает свой sequence без
  gaps/repeats) и однозначно упорядочивает fan-in по значению
  `parent_sequence`, а не по arrival time;
- budget escalation (`BudgetExceedsParent`) отклоняется, включая case
  child budget без parent budget;
- каждое отклонение (grant/budget/version/provenance/schema) создаёт
  audit-запись с `ContractError` variant и без raw payload.

## Критерии готовности

- каждый child имеет typed input/output (JSON Schema + `max_bytes`) и
  отдельный budget, проверяемые до persistence;
- `contract_version` enforced на границе create/accept с задокументированными
  major/minor правилами;
- child не расширяет права родителя ни на создании, ни на повторной проверке
  при каждом tool call, и не обходит approval;
- stale provenance имеет закрытый список проверяемых условий и отклоняется
  до persistence/fan-in;
- `parent_sequence` генерируется атомарным per-parent-task счётчиком и
  детерминированно упорядочивает fan-in;
- `receipt_id` в correlation остаётся `None`/pending до 01.3 без блокировки
  остальных критериев этого этапа;
- отклонения contract/report логируются с типом ошибки и correlation ids, без
  raw input/output.
