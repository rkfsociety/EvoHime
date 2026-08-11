# План развития Евы по мотивам Dify

## Цель

Использовать лучшие продуктовые идеи Dify для развития локального Windows-agent EvoHime, не перенося его веб-серверную архитектуру. Ева остаётся native-приложением: WinUI 3 отображает состояние, Rust Core владеет агентным циклом, инструментами, разрешениями, SQLite и выполнением, а versioned named-pipe IPC остаётся единственной границей UI/Core.

Этот документ — план адаптации идей, а не предложение копировать код Dify. Перед реализацией каждой функции нужно отдельно проверить лицензирование зависимостей и совместимость с правилами безопасности EvoHime.

## Что изучено в Dify

По официальному репозиторию и документации Dify выделены следующие сильные стороны:

1. Визуальные Workflow/Chatflow-графы с узлами ввода, LLM, кода, шаблонов, условий, итераций, параллельных ветвей и финального ответа.
2. Agent-режимы с Function Calling/ReAct и каталогом встроенных и пользовательских инструментов.
3. RAG: загрузка документов, извлечение текста, индексация, retrieval и rerank.
4. Prompt IDE: шаблоны, переменные, параметры модели и быстрый тест prompt-ов.
5. Модель расширений через плагины: tools, models, agent strategies, endpoints, datasources и triggers.
6. Наблюдаемость: трассировка шагов, логи запусков, метрики, аннотации и оценка качества.
7. API/BaaS и self-hosted deployment как отдельные поверхности продукта.

Источники:

- [репозиторий Dify](https://github.com/langgenius/dify) — общий состав продукта и каталоги `api`, `dify-agent-runtime`, `dify-agent`, `packages`, `web`;
- [Dify plugin overview](https://docs.dify.ai/en/develop-plugin/getting-started/getting-started-dify-plugin) — типы расширений;
- [выбор типа плагина](https://docs.dify.ai/en/develop-plugin/getting-started/choose-plugin-type) — границы Tool/Model/Agent Strategy/Endpoint/Datasource/Trigger;
- [Tool Plugin](https://docs.dify.ai/en/develop-plugin/dev-guides-and-walkthroughs/tool-plugin) — жизненный цикл tool-плагина;
- [Model API Interface](https://docs.dify.ai/en/develop-plugin/features-and-specs/plugin-types/model-schema) — LLM, embedding, rerank, STT/TTS и moderation;
- [30-minute workflow](https://docs.dify.ai/en/guides/application-orchestrate/creating-an-application) — User Input, переменные, Iteration и Template nodes.

## Архитектурные ограничения EvoHime

- Не возвращать web UI, HTTP-сервер, PostgreSQL, Redis или обязательный Docker-стек.
- Не переносить бизнес-логику workflow в WinUI: UI редактирует/отображает модель через IPC, Core валидирует и исполняет.
- Любая новая команда или событие требует изменения Rust и C# сторон, обновления major/minor protocol и compatibility-тестов.
- Запись файлов, shell, Git commit, внешние запросы и публикация требуют существующих permissions, timeout, cancellation и approval gate.
- Секреты хранятся в Credential Manager/DPAPI; в trace нельзя писать API keys, полные секретные заголовки и чувствительный контекст.
- Данные workflow, запусков, узлов, документов и оценок хранятся в SQLite с транзакционными миграциями и backup перед изменением схемы.
- Плагин не получает произвольный доступ к workspace: только capability-based API Core с manifest, allowlist и журналом.

## Приоритеты

| Приоритет | Возможность | Польза для Евы | Основание в Dify |
| --- | --- | --- | --- |
| P0 | Workflow graph v1 | Предсказуемые многошаговые задачи вместо одного непрозрачного цикла | Workflow, узлы и переменные |
| P0 | Улучшенный trace/evals | Понимание ошибок и измеримое улучшение prompt-ов | LLMOps, logs, annotations |
| P0 | Provider/model profiles | Быстрое и безопасное переключение моделей | Model management/plugins |
| P1 | RAG для workspace | Ответы по локальным документам с цитатами | Knowledge/RAG pipeline |
| P1 | Tool/trigger extension SDK | Расширение Евы без переписывания Core | Plugin types |
| P1 | Structured output и typed context | Надёжная передача результатов между узлами | Variables, Template, node outputs |
| P2 | Native workflow editor | Визуальная сборка повторяемых сценариев | Canvas/workflow IDE |
| P2 | Local API/trigger bridge | Интеграция с локальными приложениями | BaaS/endpoints/triggers |

## Этап 1. Workflow graph v1 в Rust Core

### Что сделать

1. Ввести версионируемую модель `WorkflowDefinition`:
   - `workflow_id`, имя, версия, проект и статус draft/published/archived;
   - узлы, порты, edges, входные/выходные переменные;
   - лимиты времени, токенов, стоимости, итераций и параллелизма;
   - режим выполнения и требуемые capabilities.
2. Начать с узлов `Input`, `Prompt/LLM`, `Tool`, `Condition`, `Template`, `Approval`, `Output`.
3. Добавить `Iteration` и bounded parallel branches после стабилизации базовой модели.
4. Валидировать граф до запуска: отсутствующие порты, несовместимые типы, циклы, недостижимые узлы, превышение лимитов и запрещённые capabilities.
5. Выполнять граф в Core через существующие cancellation, timeout, approval, dependency graph и task timeline.
6. Сохранять definition, запуск, состояние каждого узла, retry policy и результат в SQLite.
7. Передавать в UI события `workflow.started`, `node.started`, `node.waiting_approval`, `node.completed`, `node.failed`, `workflow.completed`.

### IPC и UI

- Добавить команды создания/обновления/валидации/запуска/остановки workflow и replay событий.
- В первой версии UI показать список узлов и timeline; визуальный canvas отложить до этапа 6.
- Редактирование workflow не должно позволять UI самостоятельно запускать tools или менять состояние Core.

### Проверка

- Один и тот же workflow даёт одинаковую структуру запуска при одинаковых входах и mock-провайдерах.
- Цикл и несовместимое соединение отклоняются до выполнения.
- Stop завершает активный узел и дочерние процессы.
- Перезапуск UI восстанавливает definition и timeline через replay.

## Этап 2. Typed context и structured output

1. Расширить `ContextItem` из текущего плана: workspace-файл, фрагмент, документ, tool result, diagnostic, user input.
2. Добавить JSON Schema для входов/выходов узлов и проверять результат Core до передачи следующему узлу.
3. Поддержать переменные workflow с явными типами, scope и redaction policy.
4. Добавить Template node на безопасном ограниченном шаблонизаторе; запретить произвольный shell/code execution.
5. Реализовать output parser с bounded repair: при невалидном JSON повторный запрос ограничен budget и не обходит approval.
6. В UI отображать форму входов, тип результата, размер контекста и причину ошибки валидации.

## Этап 3. Provider/model profiles

1. Нормализовать каталог моделей: provider, model, capabilities, context window, streaming, tool calling, vision, embeddings, rerank, цены/лимиты.
2. Разделить provider credential и model profile; секреты читать только через Core из Credential Manager/DPAPI.
3. Добавить fallback policy: явный список моделей, max retries, circuit breaker и запрет несанкционированного перехода на другой provider.
4. Ввести единый adapter contract для chat LLM, embedding, rerank, speech и moderation; начать с chat LLM и embedding.
5. Добавить model probe без раскрытия ключа: доступность, лимит контекста, tool-call и structured-output capability.
6. Связать каждый запуск с profile snapshot, чтобы исторический trace не менялся при последующей настройке модели.

## Этап 4. RAG для локального workspace

1. Ввести локальные knowledge collections с путями-источниками, include/exclude rules и версией индекса.
2. Реализовать ingestion через Core: text/Markdown/PDF/DOCX, размерные лимиты, хеш файла и инкрементальное обновление.
3. Хранить исходный текст, chunks, metadata и ссылки на workspace-relative source; бинарные данные не помещать безлимитно в SQLite.
4. Сначала использовать подключаемый embedding backend и локальное хранилище векторов; конкретный vector engine выбирать после benchmark на целевых Windows-машинах.
5. Добавить hybrid retrieval, top-k, metadata filters и опциональный rerank.
6. Возвращать citations: файл, заголовок/страница, диапазон chunk и score; цитаты должны попасть в trace и ответ.
7. Для изменённых документов обновлять только затронутые chunks; `.git`, секреты, `target`, `bin`, `obj` и исключённые пути не индексировать.

### Проверка

- Один документ не индексируется повторно без изменения хеша.
- Ответ показывает источники, а отсутствие релевантных chunks не маскируется уверенным текстом.
- Удаление документа удаляет его chunks и citations из будущих retrieval.
- Индексация ограничена workspace sandbox и корректно отменяется.

## Этап 5. Extension SDK: tools, models и triggers

1. Описать signed/validated manifest с id, версией, capabilities, permissions, входной/выходной schema, privacy и supported platform.
2. Первым реализовать Tool extension через out-of-process worker или контролируемый child process, а не загрузку непроверенного DLL в Core.
3. Передавать worker только typed request, scoped credentials и ограниченный capability token; добавить timeout, memory/output limit и cancellation.
4. Добавить локальный каталог установленных extensions, version pinning, checksum и rollback.
5. После Tool сделать Model adapter, затем Datasource/Trigger: локальная папка, scheduled trigger и localhost webhook только по явному включению.
6. Добавить UI для install/disable/update/revoke и журналировать permission decisions.
7. Не создавать публичный marketplace в первом релизе; достаточно локального signed package и trusted local directory.

## Этап 6. Native workflow editor и reusable skills

1. Сделать WinUI canvas только как редактор декларативной definition: перемещение узлов, порты, validation markers, zoom и keyboard navigation.
2. Добавить node inspector для prompt, model, tool, input schema, retry и approval policy.
3. Реализовать draft/publish, version diff, duplicate и rollback workflow definition.
4. Добавить reusable subworkflow/skill с typed inputs/outputs и capability manifest.
5. Разрешить запуск из task composer через slash command и быстрый выбор published workflow.
6. Сохранить ручной режим coding-agent независимо от workflow editor: workflow — дополнительный orchestration layer, а не замена обычной сессии.

## Этап 7. Observability, evaluation и безопасное улучшение

1. Расширить trace: prompt hash, model profile snapshot, token/cost counters, latency, tool args/result redaction, node inputs/outputs и approval decisions.
2. Добавить локальные datasets/eval cases: вход, ожидаемая структура, expected citations и assertion rules.
3. Реализовать offline replay на mock tools и выбранном model profile; не отправлять dataset во внешний сервис без approval.
4. Добавить сравнение двух prompt/model versions по success rate, schema validity, latency, token budget и human rating.
5. Ввести retention и export JSONL с явным redaction; trace нельзя использовать как скрытый канал хранения секретов.
6. Показать в UI причины неудачи: model, validation, permission, timeout, tool или environment, а не только общий `failed`.

## Порядок реализации

1. Workflow graph v1 и typed node state.
2. Structured context/output и schema validation.
3. Provider/model profiles и безопасные credentials.
4. RAG для workspace с citations.
5. Tool extension SDK с signed manifest и sandbox.
6. Native workflow editor и reusable subworkflows.
7. Trace, offline evals и prompt/model comparison.
8. Datasource/triggers и локальный API bridge после threat-model review.

Каждый этап должен быть отдельным task-only коммитом. Для IPC-изменений обязательны Rust tests и C# compatibility tests; для storage — migration/backup tests; для UI — WinUI smoke; перед готовностью — `git diff --check`, native package smoke и очистка не нужных build artifacts.

## Что сознательно не переносить из Dify

- Docker Compose, Kubernetes, PostgreSQL, Redis и многоарендный server control plane.
- Веб-canvas как единственный интерфейс продукта.
- Неконтролируемое выполнение Python/code nodes внутри Core.
- Публичный marketplace до появления устойчивой подписи, проверки пакетов и revoke-механизма.
- Автоматическое переключение provider, публикацию workflow или внешние triggers без явного approval и журналирования.

## Критерии успеха

- Ева может выполнить повторяемый многошаговый сценарий с ограниченными типами, лимитами и approval.
- Любой результат workflow объясним через trace, citations и состояние узлов.
- Локальные документы используются как проверяемый контекст, а не как невидимое добавление prompt-а.
- Новые model/tool adapters устанавливаются без изменения основного Core и не получают лишних прав.
- UI остаётся thin client, Core переживает reconnect/replay, а существующие Plan/Build, snapshots, permissions и rollback продолжают работать.
