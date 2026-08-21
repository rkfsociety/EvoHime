# Task Dependency Graphs

Task Dependency Graphs живут в Core и описывают зависимости между work items
проекта: шагами задачи, которые пользователь видит в task timeline. Граф
валидируется алгоритмом Kahn: отсутствующие зависимости и циклы отклоняются до
запуска.

Это **не** workflow orchestration. Два графа намеренно не объединяются:

| | Task Dependency Graphs | Workflow orchestration |
| --- | --- | --- |
| предмет | work items проекта | составная задача агента |
| контракт | `AddTaskEdge`, `GetTaskGraph`, `next_ready` | `workflow/v1` (`crates/evohime-core/src/workflow.rs`) |
| состояние | таблицы work items в SQLite | `workflow_runs`/`workflow_run_nodes` (схема 29) |
| узел | шаг задачи | action profile: `child`, `tool`, `mcp_tool`, `context_provider`, … |

Канонический контракт workflow orchestration описан в разделе «Workflow
orchestration» файла [`../architecture.md`](../architecture.md). Ссылаться на
этот документ как на описание workflow-контура нельзя.

## Runtime

- независимые шаги объединяются в batch;
- batch выполняется с bounded concurrency;
- состояние каждого шага хранится в SQLite;
- failure strategy ограничивает суммарное число ошибок;
- desktop task timeline получает status events через named pipe.

## UI

Electron отображает граф и состояния как часть task workspace. UI не вычисляет
зависимости и не запускает steps самостоятельно. Составные задачи агента
показываются отдельным разделом «Составные задачи» (`WorkflowPanel`), который
работает с workflow-контуром, а не с этим графом.
