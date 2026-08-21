# План 06-1 — Контракт workflow, CAMEL-роли и MCP-адаптеры

## Цель

Превратить существующий typed graph в канонический контракт workflow, который
может описывать CAMEL-подобные ChatAgent/Workforce-роли, context providers и
capability-as-tool, но остаётся безопасным для Rust Core.

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
   `child`, `tool`, `mcp_tool`, `context_provider`, `research`, `transform`,
   `condition`, `approval`, `loop`. `child` получает role, goal, output schema,
   context/artifact allowlist, grants, budget и max revisions. `mcp_tool`
   ссылается только на Core-owned registry entry с server identity и
   разрешённым tool name; URL, command и headers не приходят из model output.
   `context_provider` допускает только read-only provider с source identity,
   freshness policy, evidence schema и bounded result budget.
4. Описать routing edges как versioned event/port transitions. Для условий
   поддержать детерминированные `all`/`any`; LLM не выбирает произвольный node ID
   и не рассылает произвольный broadcast-контекст всем узлам.
5. Описать acceptance contract узла: output schema, required evidence,
   allowed statuses и retryable error classes.
6. Зафиксировать block/capability identity: стабильный `block_id`, версия
   capability, display metadata, input/output schema, test fixture и bounded
   execution context (`workflow_run_id`, `node_id`, `attempt_id`). Изменение
   схемы или поведения требует новой версии, а не silent mutation.
7. Описать явные failure outputs/ветви. Неподключённая ошибка не должна
   маскироваться как успешный output или разрешать зависимому узлу запуск.
8. Добавить canonical hash definition и нормализованный JSON для provenance,
   чтобы graph snapshot можно было связать с model-request и receipts.
9. Удалить или запретить в public contract любые пути к inline script,
   произвольному Python, shell и неразрешённым dynamic refs.

## Проверки

- round-trip serde fixtures для valid/invalid graphs;
- deterministic error ordering и canonical hash;
- тесты на `AND`/`OR`, route allowlist, output schema и parent-subset grants;
- тесты на Team/Agent-подобную child capability без общего mutable context;
- тесты на MCP registry identity, tool allowlist и запрет model-controlled server
  selection;
- тесты на Context Provider identity, stale evidence и parent-subset budget;
- fixtures на schema validation, test input/output и block-version mismatch;
- тесты на обязательные входы, явную failure-ветвь и запрет silent success;
- негативные тесты на nested child, capability escalation, unbounded loop,
  unknown action и unknown route;
- `cargo fmt --check` и targeted `cargo test -p evohime-core`.

## Готово, когда

Новый контракт можно принять из Core-owned запроса, проверить до любого
side effect и преобразовать в существующие `TypedChildRequest`, tool call или
approval intent без обхода policy.
