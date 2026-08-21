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

- Core-owned MCP registry entry уже существует
  (`crates/evohime-core/src/workflow_registry.rs`: server identity, tool
  allowlist, transport, host allowlist). Manifest 07-1 может ссылаться на него
  напрямую; запись с неподдержанным транспортом возвращает typed
  `transport_unavailable` и не попадает в loadout.

## Что уже есть в коде

- `ToolDefinition` в `crates/tool-runtime/src/registry.rs` хранит только
  `name`, `description`, статический список `Permission` и timeout;
- input schema инструментов живёт отдельно, в хардкодной таблице
  `tool_parameters` в `crates/evohime-core/src/lib.rs`, и используется при
  сборке `ToolSpec` для модели;
- preflight (`ToolPreflightDecision`), one-shot approval id, exact-call recheck
  и permission engine уже работают;
- `crates/tool-runtime/src/tools/mcp.rs` реализует `mcp.call`, принимающий URL
  из аргументов вызова под env-allowlist `EVOHIME_MCP_ALLOWED_HOSTS`.

Нет версии инструмента, output schema, canonical hash, side-effect class,
объявленных network domains/secret references и связи описания инструмента с
model request и receipt. Два источника описания (`ToolDefinition` и
`tool_parameters`) могут разойтись молча — сейчас это ничем не проверяется.

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
   для модели генерируется из него, а таблица `tool_parameters` в
   `crates/evohime-core/src/lib.rs` удаляется вместе с дублирующимся описанием.
6. Добавить adapter для существующих tools, чтобы старые builtin registrations
   получили manifest без изменения поведения.
7. Ввести Core validation до loadout и до execution:
   unknown capability, missing schema, invalid scope, unsupported version,
   undeclared secret/network access и parent-subset violation должны давать
   bounded typed error.
8. Привести `mcp.call` к манифесту: объявить его network domains и подготовить
   переход на registry-owned server identity, чтобы URL перестал приходить из
   аргументов модели. До готовности registry поведение не меняется, но
   ограничение фиксируется в manifest и проверяется тестом.

## Проверки

- serde round-trip и canonical-hash fixtures;
- valid/invalid input/output schema fixtures;
- тесты на parent-subset permission, budget и workspace scope;
- negative tests на undeclared network/secret, capability escalation,
  unknown tool version и manifest hash mismatch;
- тест, что каждый зарегистрированный tool имеет manifest и что `ToolSpec`
  собирается только из manifest (регрессия на расхождение двух источников);
- compatibility tests для существующих builtin tools;
- `cargo fmt --check` и targeted `cargo test -p evohime-tool-runtime -p evohime-core`.

## Готово, когда

Любой tool call в Core ссылается на проверенный manifest snapshot, а модель,
renderer и workflow не могут расширить его capability, schema, scope или
budget.
