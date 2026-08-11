# План развития Евы по мотивам Mem0 и LangGraph

## Цель

Добавить Еве две связанные возможности:

1. управляемую долговременную память, которая сохраняет полезные факты о пользователе, проектах и рабочих решениях;
2. устойчивое графовое исполнение многошаговых задач с паузами, approval, отменой, восстановлением и повторным запуском.

План адаптирует идеи проектов к EvoHime. Коды и Python-зависимости Mem0/LangGraph в native runtime не переносятся. WinUI 3 остаётся thin client, Rust Core — владельцем состояния и исполнения, SQLite — локальным хранилищем, versioned named-pipe IPC — единственной границей UI/Core.

## Что изучено

Источники:

- [mem0ai/mem0](https://github.com/mem0ai/mem0) — universal memory layer;
- [langchain-ai/langgraph](https://github.com/langchain-ai/langgraph) — low-level orchestration framework for stateful agents.

### Идеи Mem0

- несколько уровней памяти: пользователь, сессия, агент/проект;
- добавление фактов из диалогов и результатов действий, а не сохранение всего transcript целиком;
- извлечение сущностей и связывание фактов;
- поиск по нескольким сигналам: semantic, keyword/BM25 и entity matching;
- временная актуальность: отличать текущую настройку от старого факта;
- явные add/search/delete/history операции и обратная связь о качестве памяти.

В репозитории EvoHime уже есть permission `MemorySearch` и зарегистрированный `memory.search`, но его backend пока оставлен за agent memory backend. Это естественная точка для первого этапа.

### Идеи LangGraph

- граф узлов с типизированным общим состоянием вместо непрозрачного цикла;
- durable execution и checkpoint после границ узлов;
- human-in-the-loop в любой точке графа с возможностью продолжения;
- branching, retry, subgraph и bounded parallel execution;
- короткая рабочая память запуска отдельно от долгой памяти между сессиями;
- replay, поток событий и наблюдаемость переходов состояния.

В EvoHime уже есть task timeline, cancellation, approval round-trip, sequence replay, SQLite event journal и `agent.run`. Их нужно объединить в явную модель `RunGraph`, не ломая текущий ручной coding-agent режим.

## Архитектурные правила

- Не добавлять Python runtime, LangGraph server, Mem0 server, web UI, Redis, PostgreSQL или обязательный Docker-стек.
- Не переносить память, граф, retry или permissions в WinUI. UI только редактирует определения и отображает IPC-события.
- Core должен валидировать граф и memory policy до запуска; каждый tool call проходит существующие permission, timeout, cancellation и approval gates.
- Память не должна незаметно менять prompt: перед отправкой показывать источники и размер добавленного контекста, а в trace хранить memory IDs и scores.
- Секреты, полные ключи, приватные заголовки и чувствительные результаты инструментов не извлекать в память и не писать в trace.
- Незакоммиченные пользовательские изменения, workspace sandbox и существующие delivery-gates сохраняются.
- Каждая новая IPC-команда/событие обновляет Rust и C# стороны, minor/major protocol policy и compatibility tests.
- Каждое изменение EvoHime сначала выполняется самой Евой через штатные Core-инструменты с trace; ручная реализация допустима только после зафиксированной неудачи и исправления контура Евы.

## Приоритеты

| Приоритет | Блок | Результат |
| --- | --- | --- |
| P0 | Durable run state | Задача переживает reconnect, перезапуск UI и паузу approval |
| P0 | Memory v1 | Ева помнит подтверждённые факты с областями видимости и удалением |
| P0 | Graph v1 | Повторяемые узлы `Input`, `LLM`, `Tool`, `Approval`, `Condition`, `Output` |
| P1 | Hybrid memory retrieval | Поиск по SQLite FTS/keywords, embeddings и сущностям с объединённым score |
| P1 | Checkpoint/replay UI | Видны узлы, checkpoint, причина паузы и продолжение запуска |
| P1 | Subgraphs/retry/parallel | Повторяемые подсценарии, ограниченные retry и независимые ветки |
| P2 | Memory curation | Экран памяти, история изменений, feedback и ручное подтверждение фактов |
| P2 | Native graph editor | WinUI-редактор draft/publish/version diff |

## Этап 1. Durable run state и checkpoint foundation

### Реализация

1. Ввести в Core модели `RunId`, `NodeId`, `RunStatus`, `NodeStatus`, `CheckpointId` и версию схемы состояния.
2. Разделить `Task`/chat session и конкретный `Run`: один запрос может иметь несколько попыток и продолжений.
3. Хранить в SQLite definition snapshot, input hash, текущий node, typed state, pending approval, retry counters, cancellation flag и последний checkpoint.
4. Сохранять checkpoint транзакционно перед ожиданием approval, после успешного узла и перед retry.
5. При старте Core находить незавершённые runs, помечать их recoverable и продолжать только после явного policy/approval.
6. Расширить event journal событиями `run.started`, `checkpoint.created`, `run.paused`, `run.resumed`, `run.recovered`, `run.completed`, `run.failed`.

### IPC/UI

- `GetRunState`, `ResumeRun`, `PauseRun`, `CancelRun`, `ReplayRun`;
- UI показывает восстановление и причину остановки, но не редактирует raw state;
- старый `StartTask` продолжает работать как одноузловой legacy run.

### Проверка

- убийство Core после checkpoint не теряет уже завершённый узел;
- reconnect UI получает тот же state через replay без дублей;
- повторное resume идемпотентно;
- отмена не запускает следующий узел и завершает child processes.

## Этап 2. Memory v1: локальные факты вместо полного transcript

### Модель данных

Добавить миграции SQLite с backup перед схемой:

- `memory_entries`: id, scope (`user`, `project`, `session`, `agent`), kind (`preference`, `constraint`, `decision`, `fact`, `playbook`), content, normalized content, source run/message, confidence, importance, valid_from, valid_until, created_at, updated_at, deleted_at;
- `memory_entities` и связь entry/entity для простого entity linking;
- `memory_events`: add, supersede, delete, restore, feedback;
- FTS5-индекс для локального keyword поиска; embedding columns/backend добавлять через adapter, не связывая схему с одним vendor.

### Core API

1. Реализовать backend для `memory.search` с фильтрами scope, kind, project и временной актуальности.
2. Добавить внутренние операции `memory.extract`, `memory.add`, `memory.supersede`, `memory.delete`, `memory.history`.
3. Извлекать только bounded candidate facts после завершённого шага/ответа; не сохранять автоматически секреты, команды с credentials, большие tool outputs и необоснованные предположения.
4. Для конфликтов не перезаписывать старый факт молча: создать новый revision, закрыть validity старого и сохранить причину/источник.
5. Считать итоговый score из keyword, semantic, entity, scope, confidence и recency; сначала реализовать keyword+scope+recency, затем подключить embedding.
6. Передавать в LLM только top-k bounded entries с memory IDs, score и кратким source metadata.

### Управление пользователем

- `/memory search`, `/memory add`, `/memory forget`, `/memory history` как Core-команды, а не как текст для модели;
- в UI — список фактов, scope, источник, дата актуальности, confidence и действия подтвердить/удалить;
- для чувствительных или низкоуверенных фактов — approval перед публикацией в user/project scope;
- экспорт и импорт JSONL с redaction и versioned schema.

### Проверка

- одинаковый факт не создаёт бесконечные дубли;
- новый подтверждённый факт не уничтожает историю старого;
- запрос о текущем состоянии предпочитает актуальную запись старой;
- поиск не выходит за project/user scope;
- удалённая память не попадает в retrieval и может быть восстановлена только явной операцией;
- memory tests проверяют redaction, лимиты, миграцию, FTS ranking и backup/restore.

## Этап 3. Graph v1 поверх Core agent loop

### Модель графа

Ввести версионируемый `GraphDefinition`:

- `graph_id`, name, project, version, draft/published/archived;
- nodes, typed ports, edges, input/output schemas;
- execution limits: wall time, token budget, max nodes, max retries, parallelism;
- required capabilities и approval policy.

Первая версия узлов: `Input`, `Prompt/LLM`, `Tool`, `Condition`, `Approval`, `Output`. После стабилизации добавить `Template`, `Iteration`, `Subgraph` и bounded parallel branches.

### Выполнение

1. Валидировать граф до запуска: циклы, недостижимые узлы, отсутствующие порты, несовместимые типы, capability violations и лимиты.
2. Держать typed state запуска отдельно от long-term memory; обновление состояния каждого узла — атомарная транзакция вместе с checkpoint.
3. Для каждого узла применять timeout, cancellation, retry policy и approval gate; ошибка должна указывать node, category и recoverability.
4. При `Approval` остановить граф на checkpoint и продолжить по тому же `run_id`, не повторяя завершённые side effects.
5. Retry разрешать только для классифицированных transient errors; write/shell/Git operations делать idempotency-aware.
6. Для параллельных веток ввести bounded fan-out/fan-in, лимит child nodes и детерминированное объединение результатов.

### IPC/UI

Добавить команды `CreateGraph`, `ValidateGraph`, `PublishGraph`, `StartGraphRun`, `ResumeRun`, `StopRun`, `GetGraphState` и события `node.started`, `node.waiting_approval`, `node.checkpointed`, `node.retrying`, `node.completed`, `node.failed`.

На первом UI-этапе достаточно timeline и inspector состояния узла. Canvas делать только после стабильности definition/validation/execution API.

### Проверка

- один mock-граф даёт детерминированную последовательность переходов;
- невалидный граф отклоняется до запуска tools;
- checkpoint после approval продолжает следующий узел ровно один раз;
- retry не превышает лимит и сохраняет причины;
- parallel branches не превышают concurrency и корректно отменяются;
- ручной `StartTask` и graph run используют общий permission/trace контур.

## Этап 4. Связать память и граф без скрытой магии

1. Добавить явные узлы `MemorySearch` и `MemoryWrite` с input/output schema и policy scope.
2. Перед `Prompt/LLM` автоматически добавлять память только если graph definition включила memory policy; обычная сессия сохраняет текущую семантику до opt-in.
3. После `Output` запускать bounded extraction кандидатов; запись в долгую память проходит confidence/redaction/policy gate.
4. Сохранять в trace memory IDs, retrieval scores, hash контекста и решение об extraction, но не полное секретное содержимое.
5. При compact контекста сначала сохранять summary/checkpoint и список активных memory IDs, затем удалять только производные prompt fragments.

## Этап 5. Subgraphs, skills и native graph editor

1. Представить `Subgraph` как versioned published graph с typed inputs/outputs и capability manifest.
2. Добавить reusable skills для типовых сценариев: исследование, тестирование, review, backup/rollback.
3. В WinUI сделать редактор декларативной definition: canvas, ports, zoom, validation markers, node inspector, draft/publish, version diff и rollback.
4. Не давать canvas прямого доступа к tools; запуск и изменения идут только через Core IPC.
5. В composer добавить выбор published graph и slash-команду запуска с формой typed inputs.

## Этап 6. Evaluation и диагностика

- расширить trace: graph/version, node transitions, checkpoint IDs, model profile snapshot, latency, tokens, retry, approval, memory IDs и redaction status;
- добавить локальные eval cases для memory recall, conflict resolution, graph resume, approval и citation/source correctness;
- реализовать offline replay на mock provider/tools;
- сравнивать версии prompt/graph/model по schema validity, success rate, resume correctness, latency и token budget;
- добавить bounded retention и JSONL export;
- в UI различать ошибки model, validation, permission, timeout, tool, checkpoint и environment.

## Порядок реализации и коммиты

1. Durable run state/checkpoint и compatibility events.
2. Memory schema/backend и полноценный `memory.search`.
3. Graph definition/validation и одноузловой executor.
4. Graph multi-node execution, approval, retry и resume.
5. Явные memory nodes и memory policy.
6. Subgraphs, skills и editor.
7. Evaluation, replay и диагностика.

Каждый этап — отдельный task-only commit в текущей ветке `main`. Перед каждым commit обязательны Rust tests, нужные WinUI/IPC compatibility tests, migration tests для SQLite, `git diff --check`, native package smoke и очистка ненужных build artifacts. Push выполнять только по прямому запросу хозяина.

## Что не переносить

- Не подключать Mem0 Cloud или LangGraph Platform как обязательную внешнюю зависимость.
- Не хранить память в удалённом сервисе по умолчанию.
- Не добавлять автономные triggers, provider fallback, публикацию графов или внешние webhooks без явного approval и журналирования.
- Не выполнять произвольный Python/code node внутри Core.
- Не считать embedding retrieval заменой FTS, scope-фильтрам, временной актуальности и ручному удалению.
- Не показывать пользователю уверенный ответ, если memory/RAG retrieval не дал проверяемого источника.

## Критерии успеха

- Ева помнит подтверждённые проектные решения между сессиями и умеет показать/удалить источник факта.
- Долгая задача переживает reconnect, approval pause, падение Core после checkpoint и продолжает выполнение без повторного side effect.
- Повторяемый граф объясним через timeline, checkpoint, typed state и trace.
- Обычный coding-agent режим работает независимо от графов и памяти, если пользователь не включил эти возможности.
- UI остаётся thin client, Core сохраняет владение workspace/tools/permissions/SQLite, а все новые функции покрыты тестами и совместимы с native packaging.
