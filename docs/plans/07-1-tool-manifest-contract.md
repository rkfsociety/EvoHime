# План 07-1 — Tool manifest и capability contract

## Цель

Зафиксировать единый versioned manifest для встроенных, workflow и MCP-backed
инструментов. Manifest должен быть достаточным для schema validation,
permission/approval decision, loadout selection, telemetry и provenance без
чтения произвольного кода из renderer или model output.

## Зависимости

### Блокирующие

- [07-0](07-0-superagi-inspired-tooling.md);
- текущий Core `ToolRegistry`, capability registry, tool schemas,
  `run_policy` и approval intent;
- [06-1](06-1-workflow-contract.md) для versioned tool identity в workflow.

### Опциональные

- MCP adapter из 06-1. До его готовности manifest поддерживает только builtin
  tools и возвращает typed `adapter_unavailable` для MCP entry.

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
   model request и receipt. Изменение schema или capability обязано менять
   версию/hash.
5. Добавить adapter для существующих tools, чтобы старые builtin registrations
   получили manifest без изменения поведения.
6. Ввести Core validation до loadout и до execution:
   unknown capability, missing schema, invalid scope, unsupported version,
   undeclared secret/network access и parent-subset violation должны давать
   bounded typed error.

## Проверки

- serde round-trip и canonical-hash fixtures;
- valid/invalid input/output schema fixtures;
- тесты на parent-subset permission, budget и workspace scope;
- negative tests на undeclared network/secret, capability escalation,
  unknown tool version и manifest hash mismatch;
- compatibility tests для существующих builtin tools;
- `cargo fmt --check` и targeted `cargo test` для Core/tool registry.

## Готово, когда

Любой tool call в Core ссылается на проверенный manifest snapshot, а модель,
renderer и workflow не могут расширить его capability, schema, scope или
budget.
