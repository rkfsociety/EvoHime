# План развития Евы по мотивам LangChain

Дата анализа: 2026-08-11  
Исходный репозиторий: [langchain-ai/langchain](https://github.com/langchain-ai/langchain)  
Проверенная ревизия checkout: `f78df6d` (`master`)

## Цель

Перенести в Еву наиболее полезные инженерные идеи актуального LangChain v1: единый контур агента, расширяемые middleware, типизированные результаты, устойчивое состояние, потоковые события, управляемые инструменты, human-in-the-loop, compact-контекст и наблюдаемость.

Это план продуктовых возможностей и native-архитектуры, а не предложение добавить Python или прямую зависимость от LangChain. Ева должна сохранить утверждённую границу: WinUI 3 остаётся thin client, Rust Core владеет моделью, инструментами, разрешениями, workspace, памятью и SQLite, supervisor — жизненным циклом процессов.

## Что именно изучено в LangChain

- `langchain.agents.create_agent` — сборка агента из модели, инструментов, состояния, checkpoint/store и middleware; внутреннее исполнение графовое и допускает повторяемые узлы.
- `langchain.agents.middleware` — точки `before_agent`, `before_model`, `after_model`, `after_agent`, обёртки вызова модели и инструмента, динамические инструменты и изменения состояния.
- `HumanInTheLoopMiddleware` — пакетный запрос на проверку действий с решениями `approve`, `edit`, `reject`, `respond`.
- `structured_output` — provider-native или tool-based structured response с валидацией, ошибками и повтором.
- middleware для retry/fallback, ограничения числа вызовов, обработки ошибок инструментов, redaction/PII, выбора инструментов, shell-политик, file search, todo и summarization.
- потоковая модель сообщений и content blocks — возможность показывать части ответа, tool calls, результаты и метаданные до завершения запуска.
- короткая память через checkpointing и долгоживущая память через отдельный Store; классическая «магическая» memory-абстракция считается устаревшей.

Параллельно LangChain позиционирует LangGraph как низкоуровневую оркестрацию управляемых workflow, Deep Agents — как более высокий слой с планированием, subagents и filesystem, а LangSmith — как наблюдаемость, отладку и evals. Для Евы это отдельные идеи и интеграционные направления, не обязательные зависимости.

## Ключевое решение для EvoHime

Сделать небольшой собственный native agent runtime с типизированными Rust-контрактами и теми же полезными свойствами:

```text
Task request
    -> Context builder
    -> Policy/middleware pipeline
    -> Model call
    -> Structured decision
    -> Tool scheduler
    -> Approval gate (если требуется)
    -> Tool result / stream event
    -> Checkpoint + journal
    -> Next step или final response
```

Каждый переход должен быть отменяемым, ограниченным таймаутом, записываться в EventJournal и отражаться через совместимые IPC-события. UI не получает право самостоятельно запускать middleware, инструменты или работу с памятью.

## Приоритеты

### P0 — надёжный контур исполнения агента

#### 1. Typed agent state и единый scheduler

В Core ввести явные сущности `AgentRun`, `AgentState`, `ModelDecision`, `ToolCall`, `ToolResult`, `RunCheckpoint` и `RunOutcome`. Убрать зависимость от нескольких эвристических форматов вызова инструментов там, где провайдер поддерживает native tool calling; legacy-парсеры оставить как совместимый fallback.

Состояние должно включать `task_id`, `session_id`, режим, project/workspace, выбранную модель, сообщения, активный контекст, запланированные действия, попытки, budget, approvals и последний checkpoint. Переходы scheduler покрыть детерминированными unit-тестами.

#### 2. Middleware/policy pipeline в Rust

Добавить регистрируемые хуки с фиксированным порядком и trace-id:

- `before_agent` — загрузка сессии, контекста и политики;
- `before_model` — лимиты, redaction, выбор модели и доступных инструментов;
- `after_model` — валидация решения и structured output;
- `before_tool` — классификация риска, preview и approval;
- `after_tool` — нормализация результата, лимиты вывода и запись checkpoint;
- `after_agent` — сохранение итогов, метрик и summary.

Первый набор middleware: cancellation/deadline, token/tool-call budget, retry с backoff для transient ошибок, model fallback, redaction секретов, tool error normalization и ограничение размера результата. Middleware не должен обходить `permissions` и не должен выдавать UI прямой доступ к runtime.

#### 3. Расширить approval до HITL-решений

Текущий approval-контур развить до типизированного `ApprovalRequest` с несколькими действиями и решениями `approve`, `edit`, `reject`, `respond`. Для каждого инструмента объявить разрешённые решения и JSON Schema аргументов. В UI показывать фактический tool name, аргументы, preview diff/команды, риск и источник запроса.

`edit` должен возвращать изменённые аргументы в Core на повторную валидацию; `reject` — понятный `ToolMessage` модели; `respond` — ответ пользователя без запуска инструмента. Все решения и повторные попытки журналировать.

### P1 — управляемый контекст и устойчивые результаты

#### 4. Structured output для планов и финальных результатов

Ввести схемы `TaskPlan`, `PlanStep`, `VerificationReport` и `FinalTaskResult`. Для моделей с native structured output использовать provider strategy, иначе — tool strategy с JSON Schema, строгой валидацией и ограниченным retry. Невалидный ответ не должен считаться успешным завершением.

Сохранять отдельно человекочитаемый текст и типизированный результат. IPC должен передавать версию схемы и сериализованный payload; неизвестные дополнительные поля должны быть безопасно проигнорированы, а несовместимая major-версия — явно отклонена.

#### 5. Context editing и compact-сессии

Добавить в Core `ContextItem`/`ContextPack`: сообщения, выбранные файлы, выделения, диагностика, tool outputs, summaries и пользовательские инструкции. Перед вызовом модели применять правила размера, приоритета, свежести и redaction.

При приближении к лимиту:

- сохранить полный trace и checkpoint;
- сжать старые сообщения в структурированный summary с intent, решениями, изменёнными файлами, ошибками и next steps;
- оставить активные tool calls, approval и текущий plan;
- показать UI событие `context.compacted` с причиной и оценкой размера.

Пути workspace-relative, `.git` и секретные файлы фильтруются до передачи модели; неизменившийся контент в рамках запуска не дублируется.

#### 6. Streaming v2 без потери семантики

Разделить события на стабильные типы: `run.started`, `model.text.delta`, `model.tool_call`, `tool.started`, `tool.progress`, `tool.result`, `approval.required`, `checkpoint.saved`, `context.compacted`, `run.completed`, `run.failed`, `run.stopped`.

Событие должно иметь `run_id`, `step_id`, `sequence_id`, timestamp, redacted payload и статус. Сохранить replay через текущий journal/IPC; позднее подключение UI должно восстановить timeline без повторного запуска работы. Raw provider payload хранить только по диагностической политике и с redaction.

### P2 — память, поиск и планирование

#### 7. Разделить short-term и long-term memory

Short-term memory — checkpoint текущего `session_id` в SQLite: сообщения, plan, tool events и продолжение после перезапуска. Long-term memory — отдельные записи с namespace, ключом, содержимым, источником, confidence, created/updated timestamps и TTL/архивированием.

Нужны команды Core: поиск, просмотр источника, подтверждение, исправление и удаление памяти. Ева не должна молча превращать любую фразу в постоянную память: запись требует явного правила/approval, а в prompt попадают только релевантные и разрешённые записи.

#### 8. File search/RAG для workspace

Добавить индексатор текста и кода с chunk metadata: workspace-relative path, range, content hash, language, indexed_at. Сначала реализовать lexical search и точные ссылки на строки; embeddings/vector store оставить отдельным адаптером после измерения пользы.

Поиск должен возвращать цитируемые фрагменты, а не только текст. Индексирование выполняется Core с отменой, лимитами и исключением секретов/`.git`; изменения файлов инвалидируют только затронутые chunks.

#### 9. Todo/plan как проверяемый объект

Сделать `TaskPlan` частью состояния, а не только текстом prompt: шаги имеют id, зависимости, статус, verification criteria и фактический результат. UI отображает план, но меняет его только IPC-командой; Core проверяет переходы статусов.

Добавить read-only исследовательские child-runs для параллельного сбора контекста. Child-run не может писать файлы, запускать опасный shell или создавать commit; результат возвращается родительскому run как typed artifact.

### P3 — провайдеры, разработчик и качество

#### 10. Единый model profile и fallback policy

Расширить `model-gateway` профилем возможностей: streaming, tool calling, structured output, vision, embeddings, context limit, rate limits и стоимость. Выбор модели становится middleware-решением по задаче и бюджету.

Fallback должен быть явной цепочкой с причиной, retry budget и событием `model.fallback`; нельзя незаметно менять модель посреди операции, если это влияет на безопасность или формат результата.

#### 11. Инструменты как self-describing contracts

Для каждого `ToolSpec` хранить schema аргументов, capability, risk, side effects, idempotency, timeout, output limit и поддерживаемые режимы `plan/build`. Добавить registry validation: уникальное имя, валидная schema, отсутствие неизвестных capability и тестовый dry-run.

Динамический выбор инструментов допускается только через Core policy. Удалённые/MCP-инструменты ввести позднее как sandboxed adapter с отдельным approval scope, а не как прямой импорт внешнего SDK.

#### 12. Наблюдаемость и локальные evals

Расширить redacted JSONL trace: model/provider, latency, token estimates, retries, tool success/failure, approval latency, context size, fallback и итог проверки. Секреты и полный prompt не писать по умолчанию.

Добавить локальный eval harness без внешнего SaaS: фиксированные задачи, fake model gateway, fake tools, ожидаемый event trace, policy violations, structured-output cases и cancellation. Позднее можно добавить экспорт совместимого trace для внешней системы наблюдаемости, но runtime не должен от неё зависеть.

## Порядок реализации

1. Typed state/scheduler и middleware contracts; сохранить текущие legacy tool-call форматы как fallback.
2. HITL approve/edit/reject/respond и единый approval preview.
3. Structured output, verification report и строгая ошибка невалидного результата.
4. Streaming v2, checkpoint после шага и replay/reconnect тесты.
5. Context pack, лимиты и compact с миграцией SQLite.
6. Short-term/long-term memory и точечный lexical file search.
7. Typed todo/plan и безопасные child-runs.
8. Model profiles/fallback, расширенный ToolSpec и локальные evals.

Каждый этап — отдельный task-only commit в текущей ветке `main`. Изменения IPC делаются одновременно в Rust и C# с compatibility tests. Для SQLite заранее создать backup и транзакционную миграцию.

## Что не следует переносить

- Не добавлять Python/LangChain runtime в установочный native-пакет.
- Не переносить классические неявные memory-абстракции без namespace, TTL, источника и правил записи.
- Не разрешать middleware обходить `permissions`, approval, sandbox, timeout или cancellation.
- Не считать LangSmith, LangGraph, Deep Agents или MCP обязательными сервисами для локальной работы Евы.
- Не превращать WinUI в orchestrator и не возвращать web UI, HTTP server или PostgreSQL как пользовательские границы продукта.

## Общие критерии готовности

- UI остаётся thin client и восстанавливает состояние только через IPC/replay.
- Любой side-effect имеет typed tool contract, risk policy, preview и журналируемое решение.
- После перезапуска можно продолжить run с последнего checkpoint без дублирования успешного tool call.
- Невалидный structured output, превышение budget, timeout и отмена видимы как отдельные состояния, а не как ложный успех.
- Secrets, credentials и чувствительный контекст отсутствуют в trace по умолчанию.
- Для новых Rust-функций есть unit/integration tests; для IPC — compatibility tests; перед готовностью запускаются свежие native-проверки и `git diff --check`.

## Источники

- [LangChain README](https://github.com/langchain-ai/langchain/blob/master/README.md) — позиционирование, model/tool interoperability, LangGraph, Deep Agents и LangSmith.
- [Agent factory](https://github.com/langchain-ai/langchain/blob/master/libs/langchain_v1/langchain/agents/factory.py) — сборка агента, state graph, tools, middleware и structured response.
- [Middleware types](https://github.com/langchain-ai/langchain/blob/master/libs/langchain_v1/langchain/agents/middleware/types.py) — request/state/hook contracts.
- [Human-in-the-loop middleware](https://github.com/langchain-ai/langchain/blob/master/libs/langchain_v1/langchain/agents/middleware/human_in_the_loop.py) — approve/edit/reject/respond.
- [Summarization middleware](https://github.com/langchain-ai/langchain/blob/master/libs/langchain_v1/langchain/agents/middleware/summarization.py) — compact-контекст и сохранение наиболее важного состояния.
- Локальные ограничения EvoHime: `docs/architecture.md`, `docs/current-state.md`, `docs/superpowers/plans/2026-08-11-eva-opencode-inspired-improvements.md`, `crates/desktop-ipc/proto/evohime.desktop.proto`.
