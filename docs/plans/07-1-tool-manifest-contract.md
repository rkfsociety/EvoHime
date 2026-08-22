# План 07-1 — Tool manifest и capability contract

## Цель

Зафиксировать единый versioned manifest для встроенных, workflow и MCP-backed
инструментов. Manifest должен быть достаточным для schema validation,
permission/approval decision, loadout selection, telemetry и provenance без
чтения произвольного кода из renderer или model output.

## Зависимости

### Блокирующие

- [07-0](07-0-superagi-inspired-tooling.md);
- текущий `ToolRegistry` (`crates/tool-runtime/src/registry.rs`), capability
  registry, `run_policy` и approval intent;
- реализованный workflow-контракт: versioned tool identity в workflow описана
  в разделе «Workflow orchestration» [`../architecture.md`](../architecture.md).

### Опциональные

- remote signed catalog из 07-2. Без него builtin и уже зарегистрированные
  Core-owned MCP identity получают manifest из статического реестра; внешние
  package metadata не нужны для проверки контракта.

## Что уже есть в коде

- `ToolDefinition` в `crates/tool-runtime/src/registry.rs` хранит только
  `name`, `description`, статический список `Permission` и timeout;
- input schema инструментов живёт отдельно, в хардкодной таблице
  `tool_parameters` в `crates/evohime-core/src/lib.rs`. У неё два потребителя:
  сборка `ToolSpec` для модели и `recovery::recovery_hint`, который по той же
  схеме объясняет модели неудачный вызов;
- таблица покрывает 27 имён из 52 зарегистрированных в registry инструментов.
  Остальные попадают в default-ветку
  `{"type":"object","additionalProperties":true}`: модель видит инструмент без
  описания аргументов, а расхождение двух источников ничем не проверяется;
- preflight (`ToolPreflightDecision`), one-shot approval id, exact-call recheck
  и permission engine уже работают;
- `crates/evohime-core/src/workflow_registry.rs` уже разрешает workflow
  identity по `server_id`, transport, endpoint и tool allowlist и возвращает
  bounded `transport_unavailable`/host/allowlist errors. Этот реестр не
  содержит input/output schema tool-а и не заменяет `tool/manifest/v1`;
- `capability_registry::CapabilityManifest` уже является подписанным
  manifest-ом ролей и skills. Его trust root и storage нельзя смешивать с
  execution manifest tool-а, но их hash/provenance-проверки должны быть
  переиспользованы там, где это применимо;
- `crates/tool-runtime/src/tools/mcp.rs` реализует `mcp.call`, принимающий URL
  из аргументов вызова под env-allowlist `EVOHIME_MCP_ALLOWED_HOSTS`.

Нет версии инструмента, output schema, canonical hash, side-effect class,
объявленных network domains/secret references и связи описания инструмента с
model request и receipt.

## Изменения

1. Ввести `tool/manifest/v1` с immutable полями:
   `tool_id`, `version`, display name, description, input schema, output schema,
   capability class, side-effect class, provider identity и canonical hash.
2. Описать policy-поля:
   required permission, approval mode, workspace scope, network domains,
   secret references, timeout, output-size limit, retry class и cancellation
   support.
3. Описать provenance:
   origin (`builtin`, `mcp`, `catalog`), source reference, package/hash
   metadata, license и compatible Core/protocol versions.
4. Сделать canonical serialization manifest-а и связать его hash с tool intent,
   model request (`request_id`/`logical_request_id`) и receipt. Изменение schema
   или capability обязано менять версию/hash.
5. Сделать manifest единственным источником описания инструмента: `ToolSpec`
   для модели и schema-подсказка в `recovery::recovery_hint` генерируются из
   него, а таблица `tool_parameters` в `crates/evohime-core/src/lib.rs`
   удаляется вместе с дублирующимся описанием. Оба потребителя переводятся на
   manifest одним изменением: оставить один из них на старой таблице значит
   сохранить прежнее расхождение.
6. Сделать отсутствие схемы ошибкой регистрации, а не permissive fallback:
   `{"type":"object","additionalProperties":true}` перестаёт быть значением по
   умолчанию. Инструменты без явной input schema (`archive.*`, `cargo.*`,
   `filesystem_advanced.*`, `git_advanced.*`, `logs.*`, `process.*` и прочие
   из default-ветки) получают её в рамках этого этапа, иначе не попадают в
   loadout.
7. Добавить adapter для существующих tools, чтобы старые builtin registrations
   получили manifest без изменения поведения.
8. Ввести Core validation до loadout и до execution:
   unknown capability, missing schema, invalid scope, unsupported version,
   undeclared secret/network access и parent-subset violation должны давать
   bounded typed error.
9. Привести `mcp.call` к манифесту: объявить network domains и связать новый
   model/workflow loadout с `WorkflowRegistry` по `server_id` и `tool_name`,
   чтобы endpoint не выбирался из аргументов модели. Для уже существующего
   прямого IPC/tool payload временно оставить compatibility adapter только в
   Core: он обязан разрешить endpoint через registry/allowlist, пометить
   вызов как legacy-compatible и не допустить его в новый model loadout.
   После миграции клиентов поле произвольного `url` удаляется из canonical
   model schema; до этого legacy-путь остаётся явно ограниченным и покрытым
   negative tests.

## Проверки

- serde round-trip и canonical-hash fixtures;
- valid/invalid input/output schema fixtures;
- тесты на parent-subset permission, budget и workspace scope;
- negative tests на undeclared network/secret, capability escalation,
  unknown tool version и manifest hash mismatch;
- тест, что каждый зарегистрированный tool имеет manifest с непустой input
  schema и что `ToolSpec` и recovery-подсказка собираются только из manifest
  (регрессия на расхождение двух источников);
- тест, что ни один инструмент не отдаётся модели с permissive
  `additionalProperties: true` вместо объявленной схемы;
- compatibility tests для существующих builtin tools;
- `cargo fmt --check` и targeted `cargo test -p evohime-tool-runtime -p evohime-core`.

## Готово, когда

Любой tool call в Core ссылается на проверенный manifest snapshot, а модель,
renderer и workflow не могут расширить его capability, schema, scope или
budget.
