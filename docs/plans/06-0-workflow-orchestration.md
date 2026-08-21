# План 06 — CAMEL-inspired Research Workforce и workflow orchestration для Евы

Статус: проект плана, реализация не начата.

## Цель

Дать Еве безопасный и воспроизводимый способ выполнять составные задачи как
граф bounded-этапов: запускать специализированные child workflows, собирать их
результаты, проверять их по схемам, ждать approval и продолжать выполнение после
перезапуска Core.

План использует выбранные идеи CAMEL — ChatAgent/Workforce, task
specification/planning, critic loop, stateful memory, typed structured output,
MCP toolkit и evaluations — но не добавляет CAMEL как зависимость и не
переносит его Python runtime. CAMEL `to_mcp`, внешние web/API services и
отдельный control-plane UI остаются только reference и не входят в
поставляемый Windows runtime.

Исполнительным контуром остаются Rust Core, существующие child contracts,
`run_policy`, receipts, SQLite и authenticated Electron IPC.

## Что именно заимствуем

- ChatAgent/Workforce → роли, маршрутизация, последовательные/параллельные
  этапы и deterministic fan-in в каноническом typed data-flow контракте Core;
- task planner/critic loop → уточнение задачи, synthesis и независимая проверка
  результата с evidence gate;
- stateful memory/context utility → read-only источники актуального контекста с
  provenance, freshness и bounded context budget;
- MCP toolkit → Core-owned каталог доверенных внешних tools с discovery,
  allowlist и ограниченными schemas;
- structured input/output и human-in-the-loop → schema validation и единая
  точка перехвата до side effect поверх текущих approval intent, exact-call
  recheck и signed receipts;
- tracing/evaluation → корреляция workflow/node/tool/model событий,
  сравнение с single-agent baseline и deterministic сценарии проверки.

Не входят в план CAMEL memory/storage/RAG как замена локальным memory/RAG,
локальный code executor, произвольный Python/Node sidecar, внешний HTTP runtime,
публикация Workforce как MCP server или обязательная внешняя telemetry.

## Что уже есть в коде

- `crates/evohime-core/src/workflow.rs` содержит typed graph с портами,
  типами данных, ограничениями размера, проверкой циклов и недостижимых узлов;
- `crates/evohime-core/src/workflow_runner.rs` строит стабильный
  topological plan и планирует retry, timeout, cancellation и approval;
- `crates/evohime-core/src/workflow_execution.rs` выполняет узлы с live
  approval/cancellation, timeout и retry;
- `crates/evohime-core/src/child_contracts.rs` уже содержит role, reduced
  context, output schema, grants, budget, provenance и revision limits;
- `docs/features/task-dependency-graphs.md` фиксирует ожидаемую модель графа,
  bounded concurrency, SQLite state и projection в Electron.

Сейчас workflow-модули не являются полноценным пользовательским workflow
контуром: нет durable graph-run state и recovery, нет адаптера узла к реальному
child/tool/model execution path, нет Electron IPC surface и нет проверенного
end-to-end сценария.

## Решения и границы

1. Workflow definition immutable после запуска. Новая версия графа создаётся
   целиком, а уже начатый run продолжает использовать snapshot.
2. Core остаётся единственным владельцем графа, состояния, планирования и
   запуска. Renderer только создаёт запрос и показывает projection.
3. Узлы workflow не получают произвольный код, shell или динамический импорт.
   Разрешены только известные типы действий и зарегистрированные capabilities.
4. Группа child workflows в терминах продукта — bounded capability graph, а не
   свободная LLM-диспетчеризация или broadcast-чат. Nested child delegation
   остаётся запрещённой.
5. Любой child получает grants, context allowlist, output schema и budget,
   являющиеся подмножеством родительского запуска.
6. При недоступности optional модели или embeddings workflow продолжает работу
   по deterministic fallback либо завершается typed `unknown/degraded`, но не
   подтверждает результат вслепую.

## Этапы

- [06-1 — контракт workflow и адаптеры узлов](06-1-workflow-contract.md)
- [06-2 — durable runtime и интеграция с Core](06-2-workflow-runtime.md)
- [06-3 — IPC и Electron projection](06-3-workflow-desktop.md)
- [06-4 — evaluation, security и закрытие плана](06-4-workflow-acceptance.md)

## Внешние references

- [CAMEL repository](https://github.com/camel-ai/camel) — agents, societies,
  Workforce, memory, tools, RAG и evaluations;
- [CAMEL ChatAgent](https://raw.githubusercontent.com/camel-ai/camel/master/camel/agents/chat_agent.py)
  — stateful agent, structured output, tool calls и context summarization;
- [CAMEL Workforce](https://raw.githubusercontent.com/camel-ai/camel/master/camel/societies/workforce/workforce.py)
  — coordinator, planner, workers, failure handling и fan-in reference;
- [CAMEL MCPToolkit](https://raw.githubusercontent.com/camel-ai/camel/master/camel/toolkits/mcp_toolkit.py)
  — discovery/lifecycle reference для MCP tools.

## Критерий готовности плана

План завершён, когда пользователь запускает составной workflow из Electron,
Core выполняет его через существующие policy/approval/child контуры,
перезапуск не теряет состояние, UI показывает bounded projection, а
deterministic evals подтверждают порядок, fan-out/fan-in, retry, cancellation,
approval, recovery и отсутствие capability escalation.
