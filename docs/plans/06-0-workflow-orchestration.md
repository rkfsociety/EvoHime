# План 06 — Workflow orchestration и внешние agent-инструменты для Евы

Статус: проект плана, реализация не начата.

## Цель

Дать Еве безопасный и воспроизводимый способ выполнять составные задачи как
граф bounded-этапов: запускать специализированные child workflows, собирать их
результаты, проверять их по схемам, ждать approval и продолжать выполнение после
перезапуска Core.

План использует проверенные идеи AutoGen — typed message routing, GraphFlow,
AgentTool, MCP Workbench, intervention handlers и tracing — но не добавляет
AutoGen как зависимость и не переносит его Python runtime. AutoGen находится в
режиме сопровождения, поэтому Microsoft рекомендует для новых проектов
Microsoft Agent Framework; его typed data-flow и checkpoint-подход используются
только как дополнительный reference.

Исполнительным контуром остаются Rust Core, существующие child contracts,
`run_policy`, receipts, SQLite и authenticated Electron IPC.

## Что именно заимствуем

- GraphFlow → условные переходы, bounded fan-out/fan-in и циклы с обязательным
  ограничителем, но в каноническом typed data-flow контракте Core;
- AgentTool → child workflow выступает capability coordinator-а, а не свободным
  агентом с доступом к общему контексту;
- MCP Workbench → Core-owned адаптер доверенных внешних tools;
- intervention handler → единая точка перехвата до side effect, поверх текущих
  approval intent, exact-call recheck и signed receipts;
- tracing/evaluation → корреляция workflow/node/tool/model событий и
  deterministic сценарии проверки.

Не входят в план AutoGen Studio, AutoGen Memory/RAG, локальный code executor,
внутренний AgentChat runtime или произвольный Python/Node sidecar.

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

- [AutoGen repository](https://github.com/microsoft/autogen) — статус проекта,
  слои Core/AgentChat/Extensions;
- [AutoGen GraphFlow](https://microsoft.github.io/autogen/dev/user-guide/agentchat-user-guide/graph-flow.html)
  — graph patterns, используемые только как reference;
- [AutoGen MCP Workbench](https://microsoft.github.io/autogen/stable/reference/python/autogen_ext.tools.mcp.html)
  — MCP capability surface и ограничения доверия;
- [AutoGen migration guide](https://learn.microsoft.com/en-us/agent-framework/migration-guide/from-autogen/)
  — typed Workflow/data-flow и checkpoint reference для архитектурных решений.

## Критерий готовности плана

План завершён, когда пользователь запускает составной workflow из Electron,
Core выполняет его через существующие policy/approval/child контуры,
перезапуск не теряет состояние, UI показывает bounded projection, а
deterministic evals подтверждают порядок, fan-out/fan-in, retry, cancellation,
approval, recovery и отсутствие capability escalation.
