# План 07 — SuperAGI-inspired tool manifests, Action Console и telemetry

Статус: предложен, реализация не начата.

## Цель

Перенять из SuperAGI лучшие продуктовые идеи вокруг toolkit-ов, явных схем
инструментов, human-in-the-loop и telemetry, не добавляя в Еву Python runtime,
Celery, Redis, PostgreSQL, Docker, внешний web-GUI или marketplace с
непроверенным динамическим кодом.

Исполнительным контуром остаются Rust Core, существующие `ToolRegistry`,
permission/approval policy, `run_policy`, provenance receipts, SQLite и
authenticated Electron IPC.

## Что изучено в SuperAGI

- README описывает toolkit marketplace, Action Console, vector memory,
  telemetry, token control и ReAct workflows:
  [README](https://github.com/TransformerOptimus/SuperAGI/blob/main/README.MD);
- `BaseTool` объединяет описание инструмента, Pydantic-схему аргументов,
  permission flag и toolkit configuration:
  [base_tool.py](https://raw.githubusercontent.com/TransformerOptimus/SuperAGI/main/superagi/tools/base_tool.py);
- цикл агента строится вокруг model call → structured tool choice → tool
  execution → feed/event → следующего шага:
  [agent_iteration_step_handler.py](https://raw.githubusercontent.com/TransformerOptimus/SuperAGI/main/superagi/agent/agent_iteration_step_handler.py);
- фоновые workflow используют Celery/Redis/PostgreSQL:
  [docker-compose.yaml](https://raw.githubusercontent.com/TransformerOptimus/SuperAGI/main/docker-compose.yaml);
- загрузка toolkit-ов скачивает GitHub zipball и распаковывает код, а очередь
  и обработчики используют небезопасно переносимые для Евы `eval`/строковые
  parser-подходы:
  [tool_manager.py](https://github.com/TransformerOptimus/SuperAGI/blob/main/superagi/tool_manager.py),
  [task_queue.py](https://raw.githubusercontent.com/TransformerOptimus/SuperAGI/main/superagi/agent/task_queue.py).

SuperAGI используется только как reference. Его runtime и код загрузчика не
являются зависимостями плана.

## Что уже есть в Еве

- Core-owned `ToolRegistry`, capability loadout, permission и approval policy;
- одноразовые approval-токены, exact-call recheck и signed receipts;
- bounded `run_policy` с лимитами итераций, времени, tool calls, токенов и
  стоимости;
- Context Budget Manager, scratchpad, artifact store, Local Agentic RAG и
  provenance для evidence;
- typed workflow/child-workflow контракты и Electron Operations Panel.

Эти компоненты не заменяются. План добавляет поверх них недостающий единый
контракт описания toolkit-а и понятную пользовательскую проекцию действия.

## Решения и границы

1. Tool manifest immutable внутри snapshot запуска. Изменение инструмента
   создаёт новую версию и не меняет уже начатый run.
2. Core проверяет manifest, capability, schema, policy, budget и provenance до
   любого эффекта. Renderer не выбирает права и не исполняет инструмент.
3. На первом этапе поддерживаются встроенные Rust tools и уже разрешённые
   MCP/внешние adapters. Произвольный Python/Node plugin runtime, shell,
   inline script и model-controlled server selection запрещены.
4. Каталог toolkit-ов хранит metadata, версии, hash, license и capability
   declaration. Установка не означает автоматическое разрешение на запуск.
5. UI получает только bounded action projection: имя, цель, безопасный preview
   аргументов, affected resources, side effects, budget и status. Secrets,
   raw prompt и raw tool output в renderer не передаются.
6. Telemetry хранит корреляцию и bounded metadata, а не секреты и не полный
   prompt. Полные данные остаются в существующих Core-owned storage/policy
   контурах.

## Зависимости

### Блокирующие

- существующие `ToolRegistry`, permission/approval policy, `run_policy` и
  authenticated desktop IPC;
- реализованные provenance receipts, SQLite event journal и Context Budget;
- план 06-1 для versioned workflow/tool identity;
- план 06-3 для существующей Electron workflow/approval projection.

### Опциональные

- план 06-4 evaluation workflow. До его появления проверки 07 выполняются
  отдельным deterministic test suite;
- optional embeddings. Без них tool discovery и telemetry работают на
  metadata/FTS5 и не требуют vector backend;
- подписанный внешний каталог. До появления signing pipeline доверие строится
  на release-channel hash manifest, allowlist и явном пользовательском
  подтверждении установки.

## Этапы

1. [07-1 — Tool manifest и capability contract](07-1-tool-manifest-contract.md)
2. [07-2 — Toolkit catalog, provenance и безопасный lifecycle](07-2-toolkit-catalog-lifecycle.md)
3. [07-3 — Action Console и approval projection](07-3-action-console.md)
4. [07-4 — Tool telemetry, cost view и evaluation](07-4-tool-telemetry-evaluation.md)

## Не входит в план

- перенос SuperAGI целиком или добавление его как Git/npm/Cargo dependency;
- Docker Compose, Redis, Celery, PostgreSQL и отдельный HTTP control plane;
- замена существующих memory/RAG/context-budget механизмов vector memory;
- автоматическая загрузка и выполнение кода из GitHub marketplace;
- свободная multi-agent сеть или новый workflow executor;
- ослабление sandbox, approval, privacy, provenance или run budgets.

## Критерий готовности плана

Каждый доступный инструмент имеет versioned manifest и проверяемую capability
модель; пользователь видит bounded preview опасного действия и может принять,
отклонить или отменить его; каждый запуск даёт replayable telemetry по model/tool
calls, budget, retries и approval; deterministic tests подтверждают отсутствие
capability escalation, утечки secrets и обхода Core policy.
