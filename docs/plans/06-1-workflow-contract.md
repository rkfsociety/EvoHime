# План 06-1 — Контракт workflow и адаптеры agent-узлов

## Цель

Превратить существующий typed graph в канонический контракт workflow, который
может описывать AutoGen-подобные роли, capability-as-tool и маршруты, но
остаётся безопасным для Rust Core.

## Зависимости

### Блокирующие

- [06-0](06-0-workflow-orchestration.md);
- существующие `workflow.rs`, `workflow_runner.rs` и child contracts;
- текущие `run_policy`, capability registry, model gateway и tool registry.

### Опциональные

- локальные embeddings и расширенный RAG. До их появления узел knowledge
  использует FTS5 или возвращает bounded `degraded` с объяснением;
- дополнительные model routes. До их появления используется уже выбранный
  route snapshot и существующий fallback.

## Изменения

1. Зафиксировать `workflow/v1` как versioned serde-контракт с immutable
   `graph_id`, `version`, entry node, nodes, edges и run-level budget snapshot.
2. Сохранить существующие bounded limits и deterministic validation: duplicate
   IDs, unknown ports, type mismatch, cycles, unreachable nodes, invalid retry,
   timeout и loop bounds.
3. Добавить к узлу typed action profile:
   `child`, `tool`, `mcp_tool`, `research`, `transform`, `condition`,
   `approval`, `loop`. `child` получает role, goal, output schema,
   context/artifact allowlist, grants, budget и max revisions. `mcp_tool`
   ссылается только на Core-owned registry entry с server identity и
   разрешённым tool name; URL, command и headers не приходят из model output.
4. Описать routing edges как versioned event/port transitions. Для условий
   поддержать детерминированные `all`/`any`; LLM не выбирает произвольный node ID
   и не рассылает произвольный broadcast-контекст всем узлам.
5. Описать acceptance contract узла: output schema, required evidence,
   allowed statuses и retryable error classes.
6. Добавить canonical hash definition и нормализованный JSON для provenance,
   чтобы graph snapshot можно было связать с model-request и receipts.
7. Удалить или запретить в public contract любые пути к inline script,
   произвольному Python, shell и неразрешённым dynamic refs.

## Проверки

- round-trip serde fixtures для valid/invalid graphs;
- deterministic error ordering и canonical hash;
- тесты на `AND`/`OR`, route allowlist, output schema и parent-subset grants;
- тесты на `AgentTool`-подобную child capability без общего mutable context;
- тесты на MCP registry identity, tool allowlist и запрет model-controlled server
  selection;
- негативные тесты на nested child, capability escalation, unbounded loop,
  unknown action и unknown route;
- `cargo fmt --check` и targeted `cargo test -p evohime-core`.

## Готово, когда

Новый контракт можно принять из Core-owned запроса, проверить до любого
side effect и преобразовать в существующие `TypedChildRequest`, tool call или
approval intent без обхода policy.
