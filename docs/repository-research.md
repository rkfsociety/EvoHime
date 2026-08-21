# Анализ внешних репозиториев для EvoHime

Рабочий журнал исследования репозиториев, которые могут дать Еве полезные
идеи, код, контракты, тесты или инженерные практики. Записи не являются
утверждённым состоянием продукта и не означают автоматического принятия
зависимости или копирования кода.

## Как оцениваем

Для каждого репозитория проверяем:

- назначение и ключевые возможности;
- архитектуру и совместимость с Electron + Rust Core + supervisor;
- лицензию, происхождение и ограничения повторного использования;
- качество кода, тестов, документации и активность проекта;
- безопасность, приватность, sandbox, сеть, хранение секретов и модель угроз;
- что можно перенять как идею, контракт, тестовый подход или код;
- стоимость адаптации, риски и способ изоляции интеграции;
- связь с текущими планами и отсутствие конфликта с уже реализованными
  границами EvoHime.

## Реестр

| № | Репозиторий | Статус | Потенциальная ценность | Решение |
|---|---|---|---|---|
| 3 | [run-llama/llama_index](https://github.com/run-llama/llama_index) | Исследовано | RAG-контракты, ingestion, workflow и evaluation | Адаптировать идеи; runtime не подключать |

## Карточки исследований

Для каждого источника добавляется отдельная карточка:

```markdown
### N. Название репозитория

- Источник:
- Дата проверки:
- Ревизия/commit:
- Лицензия:
- Назначение:
- Краткий вывод:

#### Что изучено

- архитектура и основные модули;
- точки интеграции;
- тесты и проверяемые гарантии;
- документация и история изменений.

#### Что можем использовать в Еве

- идея/паттерн:
- контракт или формат:
- код или библиотека:
- тесты/fixtures:

#### Ограничения и риски

- лицензия и attribution:
- безопасность и приватность:
- несовместимость с архитектурой EvoHime:
- стоимость сопровождения:

#### Предварительное решение

`использовать` / `адаптировать` / `наблюдать` / `отклонить`

#### Связь с EvoHime

- затронутые документы или планы:
- предполагаемый этап интеграции:
- критерии проверки:
```

### 1. Superagent SDK

- Источник: https://github.com/superagent-ai/superagent
- Дата проверки: 2026-08-21
- Ревизия/commit: `aa6c184972fb6fe29d3bf41f12c8f46d7c4262d8`
- Лицензия исходного кода: MIT
- Состав: TypeScript SDK, Python SDK, CLI и stdio MCP-сервер;
  отдельная web-документация и OpenAPI.
- Назначение: внешние LLM-guardrails для классификации prompt injection и
  опасных инструкций, LLM-redaction PII/секретов и сканирования репозиториев.
- Краткий вывод: полезный источник контрактов, эвристик, тестовых идей и
  модели изоляции сканирования; готовой локальной подсистемой для EvoHime не
  является. Код напрямую в Core не переносить.

#### Что изучено

- `sdk/typescript/src/client.ts` и параллельная Python-реализация дают три
  операции: `guard`, `redact`, `scan`;
- `guard` принимает текст, PDF, изображения, Blob/URL, режет большие тексты на
  bounded chunks и агрегирует результат по OR: если заблокирован хотя бы один
  chunk или PDF-page, итог `block`;
- результат `guard` типизирован как `pass|block` с
  `violation_types`, `cwe_codes`, reasoning и token usage;
- провайдеры унифицированы форматом `provider/model`, поддерживают structured
  output там, где провайдер его умеет, а для superagent-моделей есть отдельный
  адаптер Ollama-style response;
- URL-fetcher проверяет только HTTP(S), запрещает credentials и localhost,
  разрешает DNS во все адреса и отклоняет любой private/internal IP, закрепляет
  проверенные адреса для соединения, повторно проверяет redirect targets,
  ограничивает 5 redirects, 30 секунд и 25 MiB;
- `scan` клонирует репозиторий в Daytona sandbox, устанавливает
  `opencode-ai`, запускает security prompt, разбирает JSONL-события и удаляет
  sandbox после выполнения;
- MCP-слой использует строгие Zod-схемы, bounded input до 50 000 символов и
  read-only/idempotent annotations для guard, redact и scan;
- исходный репозиторий содержит MIT `LICENSE`, `SECURITY.md` и тесты для
  guard, chunking, redaction, URL/SSRF и provider fallback.

#### Что можем использовать в Еве

- **Контракт оценки входа как advisory signal.** Взять структуру
  `pass|block + violation_types + cwe_codes + bounded explanation` для
  внутреннего typed-события Core и evaluation fixtures. Это может улучшить
  диагностику prompt-injection и классификацию причин, но не должно заменять
  Core policy, capability checks, approval или hard-deny.
- **Bounded chunking и консервативная агрегация.** Использовать как идею для
  bounded проверки больших внешних evidence/workspace-фрагментов: ограничить
  размер, число параллельных вызовов, бюджет и дедлайн; итог блокировать при
  одном подтверждённом опасном фрагменте, сохраняя объединённые типы нарушений.
  В EvoHime это должно проходить через уже существующий context budget и
  Core-owned execution path.
- **SSRF-safe remote fetch как эталон тестов.** Сопоставить с текущим
  `research_fetch`: полезны проверка всех DNS-адресов, DNS pinning, повторная
  проверка каждого redirect, запрет credentials/private ranges, bounded body
  и timeout. Переносить только после сверки с существующим Rust-контрактом,
  чтобы не создать вторую несовместимую сетевую политику.
- **Capability matrix для model gateway.** Таблица поддержки structured
  output/vision и provider-specific fallback полезна как источник тестовой
  матрицы для `crates/model-gateway`. Реализовывать её нужно в Rust policy
  snapshot и capability metadata Евы, а не добавлением TypeScript SDK.
- **Изолированный repository-security workflow.** Идея отдельного read-only
  sandbox, pinned revision, bounded scan и typed report подходит для будущей
  функции проверки внешнего репозитория перед подключением к Еве. Для Евы
  sandbox должен контролироваться supervisor/Core, не Daytona; результат
  должен быть untrusted evidence с provenance и redaction, а не автоматически
  подтверждённой инструкцией.
- **MCP schema/annotation pattern.** Строгие схемы, максимальная длина входа,
  read-only/idempotent metadata и явное разделение guard/redact/scan полезны
  при проектировании будущего внешнего MCP adapter или tool-manifest. Это не
  меняет текущую границу: внешний MCP требует отдельного permission/approval.
- **Тестовые сценарии.** Перенять набор классов fixtures: override system
  instructions, prompt extraction, data exfiltration, jailbreak, malicious
  repository instructions, redirect-to-private-IP, oversized response и
  malformed structured output.
- **Локальные Guard-модели как предмет оценки.** Упоминание
  `superagent-guard-0.6b/1.7b/4b` и GGUF можно использовать только для
  отдельного benchmark local provider. В репозитории нет весов и полноценного
  локального runtime, поэтому это не готовая зависимость Евы.

#### Ограничения и риски

- **Нарушение local-first при прямом подключении.** Основной SDK отправляет
  контекст во внешние Cloud Run/API endpoints или выбранному внешнему
  provider. Даже `superagentProvider` без model API key не делает обработку
  локальной.
- **Скрытая внешняя телеметрия.** `SafetyClient` требует
  `SUPERAGENT_API_KEY` уже при создании клиента и fire-and-forget отправляет
  token usage на `https://superagent.sh/api/billing/usage`. Это не подходит для
  Core-owned secrets/provenance boundary без отдельного явного opt-in.
- **Fallback расширяет egress.** При timeout SDK повторяет запрос на
  `https://superagent.sh/api/fallback`; fallback нужно рассматривать как
  отдельный provider и отдельное раскрытие данных.
- **LLM-classifier не является security boundary.** `guard` может ошибаться,
  а пустой PDF без извлекаемого текста превращается в `pass`. Поэтому его
  результат допустим только как advisory/evaluation signal; разрешение tool
  effect должно оставаться за Core policy и approval.
- **Сканирование передаёт секреты в sandbox.** `scan` собирает
  `ANTHROPIC_API_KEY` и `OPENAI_API_KEY` и передаёт их в Daytona environment,
  затем устанавливает floating `opencode-ai@latest`. Для EvoHime это
  неприемлемая модель доверия и воспроизводимости.
- **Командная поверхность scan недостаточно близка к контракту Евы.** В
  Python-пути shell-команда строится с `repo` и `branch`; перед адаптацией
  потребовались бы строгая валидация, pinned commit, отсутствие shell
  interpolation и supervisor limits.
- **Redact основан на LLM и rewrite может менять смысл.** Это подходит для
  удобного пользовательского текста, но не для доказательной redaction
  audit/provenance. В Еве сохранять deterministic redaction и typed tombstones.
- **Интеграционная зрелость неоднородна.** TypeScript SDK в ревизии собрался,
  162 теста прошли; у него отсутствует lockfile, а CLI/MCP зависят от
  опубликованного `safety-agent ^0.1.7`, тогда как SDK в checkout имеет
  `0.1.8-rc1`. Python-тесты требуют установки package/runtime dependencies;
  без `daytona_sdk` один credential test падает до проверки credentials.
- **Лицензия моделей не подтверждена лицензией кода.** MIT относится к
  исходному репозиторию; веса Guard и их условия нужно проверять отдельно,
  если benchmark когда-либо превратится в поставку.

#### Предварительное решение

`адаптировать идеи и тестовые сценарии`; `не использовать SDK/MCP/Daytona как
runtime-зависимость Евы`.

#### Связь с EvoHime

- уже покрыто и не дублировать: Core-owned redaction, context budget,
  prompt-injection envelope, model-gateway routing/capabilities, approval,
  provenance и bounded research fetch;
- возможная будущая работа: security-evaluation fixtures для внешнего
  repository/evidence scanning и typed advisory guard result;
- возможная отдельная работа после подтверждения необходимости: локальный
  guard benchmark на модели с проверенными весами и лицензией;
- критерии проверки: отсутствие нового необъявленного network egress,
  сохранение Core-only policy/approval, bounded budget/timeout/cancellation,
  redacted provenance, deterministic replay и negative tests на prompt
  injection/SSRF/secret leakage.

### 2. LangChain

- Источник: https://github.com/langchain-ai/langchain
- Дата проверки: 2026-08-21
- Ревизия/commit: `2a91b2f5841e240ece7f344822cad8f17237ac22`
- Лицензия исходного кода: MIT
- Состав: Python-монорепозиторий с `langchain-core`, активным `langchain`
  v1, legacy `langchain-classic`, `langgraph`-based agent runtime,
  партнёрскими integrations, text splitters, model profiles и
  стандартизированными тестами.
- Назначение: composable framework для LLM applications и agent loops с
  tools, structured output, middleware, checkpoint/store, streaming и
  provider integrations.
- Краткий вывод: очень полезен как каталог проверенных контрактов и
  сценариев тестирования. Целиком подключать к Еве нельзя и не нужно: текущая
  архитектура Евы уже имеет Rust Core, собственный model gateway, workflow,
  SQLite, approvals, context budget и provenance.

#### Что изучено

- `langchain-core` задаёт типизированные messages, `BaseTool`,
  `StructuredTool`, JSON/Pydantic args schemas, `ToolMessage`, обработку
  tool errors и `return_direct`;
- `langchain.agents.create_agent` строит цикл model → tool calls → tool results
  → model до terminal condition, умеет structured response, middleware,
  checkpointing, store, interrupts и streaming;
- `AgentMiddleware` предоставляет хуки `before_agent`, `before_model`,
  `after_model`, `after_agent`, `wrap_model_call` и `wrap_tool_call`. Request
  можно менять через immutable `override`, а middleware может retry, short-circuit,
  менять model/tools/system message и делать bounded state updates;
- `HumanInTheLoopMiddleware` описывает per-tool approval с решениями
  `approve`, `edit`, `reject`, `respond`, динамическим `when`-predicate,
  bounded action request и привязкой решения к `tool_call_id`;
- `ModelCallLimitMiddleware` считает model calls на уровне run и thread и
  завершает выполнение с явным `end` либо поднимает ошибку;
- `ModelRetryMiddleware` задаёт max retries, фильтр retryable errors,
  exponential backoff, jitter, max delay и политику поведения после исчерпания
  попыток;
- `PIIMiddleware` имеет deterministic detectors для email, credit card, IP,
  MAC и URL, custom regex/callable detector и стратегии `block`, `redact`,
  `mask`, `hash`. Он применяет обработку к input, AI output, tool results и
  streaming surfaces, включая tool-call arguments;
- `ContextEditingMiddleware` применяет последовательность bounded edits к
  tool results перед model call, поддерживает approximate/exact/custom token
  counter и immutable replacement списка messages;
- `ModelProfile` описывает lifecycle, context/output limits, modalities,
  reasoning, tool calling, tool choice, streaming tool calls, structured
  output, attachments и temperature. `langchain-model-profiles` генерирует
  профили из `models.dev`, применяет provider augmentations и предупреждает о
  неизвестных ключах/расхождении версий;
- `langchain-tests` задаёт общий набор unit/integration conformance tests для
  chat-model integrations. Unit tests запрещают network, integration tests
  отделены и явно требуют credentials;
- монорепозиторий использует `uv`, отдельные `pyproject.toml`/`uv.lock` для
  пакетов, editable local sources и партнёрские пакеты. `langchain-core` имеет
  обязательную зависимость от `langsmith`, хотя tracing должен быть выключен в
  unit-тестах.

#### Что можем использовать в Еве

- **Middleware как conceptual pipeline.** Сопоставить hook-порядок
  `before_model → model → after_model` и `wrap_tool_call` с Core-owned
  policy pipeline. Полезно для будущих независимых стадий: context assembly,
  policy snapshot, approval check, dispatch, result redaction и audit. В Еве
  это должны быть typed Rust stages, а не Python callbacks.
- **Approval contract для UI/IPC-сравнения.** Использовать набор действий
  `approve/edit/reject/respond`, `tool_call_id`, dynamic predicate и batch
  review как материал для проверки уже существующего EvoHime approval UX.
  Edit должен заново проходить canonicalization, exact-call hash и policy
  recheck; решение не может напрямую менять Core state.
- **Tool schema discipline.** Использовать идею единого tool manifest с
  именем, описанием, JSON schema аргументов, async execution, `return_direct`,
  typed result/error и отдельным artifact. Это хорошо ложится на текущие
  `ToolSpec`, tool loadout, capability registry и receipts Евы.
- **Explicit run/thread budgets.** Разделение лимитов run и durable thread
  полезно для eval fixtures и diagnostics: отдельно показывать исчерпание
  model calls, tool calls, wall clock, tokens и cost. В EvoHime уже есть
  `run_policy`; можно использовать LangChain как сравнительный набор edge
  cases, не дублируя механизм.
- **Retry semantics.** Взять как reference для классификации retryable
  provider failures, capped exponential backoff и jitter. В Еве retry должен
  сохранять request-attempt lineage, policy/route hashes, idempotency и
  `unknown_outcome` semantics; превращать окончательную ошибку в обычный
  успешный AI message нельзя.
- **Полная redaction surface.** Использовать перечень поверхностей из
  `PIIMiddleware` как checklist для Core: user input, model output, tool
  arguments, tool output, stream events, state snapshots и diagnostics. Сами
  detectors и правила Ева должна держать deterministic/core-owned, расширяя
  их только через versioned policy.
- **Context edit strategy.** Идея staged edits и injectable token counter
  полезна для сравнения с `context-budget`: approximate counter как быстрый
  fallback, provider tokenizer как optional exact path, а каждая правка должна
  быть отражена в immutable ledger с причиной и hash.
- **Model capability profiles.** Это наиболее практически полезная часть:
  перенять поля и workflow обновления профилей для `model_context_limits` и
  model gateway — limits, modalities, tool calling, structured output,
  reasoning и model lifecycle. Источник и timestamp должны быть видны,
  stale/missing profile должен вести к conservative fallback, а не к
  безусловному разрешению capability.
- **Provider conformance tests.** Использовать `langchain-tests` как идею для
  общего Rust/Electron compatibility matrix: каждый provider обязан пройти
  одинаковые проверки messages, tool calls, structured output, usage,
  cancellation, timeout, errors и redaction. Network tests держать отдельно
  от deterministic unit suite.
- **State graph and checkpoint vocabulary.** Термины state, checkpoint,
  interrupt, resume, store и structured response полезны для документации и
  acceptance tests workflow runtime Евы. Уже реализованный Rust workflow
  runtime должен оставаться источником истины.

#### Ограничения и риски

- **Архитектурное дублирование.** `langchain` v1 зависит от Python `pydantic`,
  `langchain-core` и `langgraph`; перенос этого стека в Windows desktop Core
  добавит отдельный runtime, packaging, lifecycle и security boundary.
- **Middleware не является самостоятельной policy boundary.** Approval,
  redaction и limits действуют только если конкретный agent собран с нужным
  middleware. Tool без записи в `interrupt_on` auto-approved, а `False` явно
  отключает interrupt. Для Евы security policy не должна зависеть от состава
  пользовательской конфигурации agent.
- **Состояние mutable и графовое.** LangChain/LangGraph допускает state
  updates, commands и повторные tool/model hops; прямое заимствование может
  нарушить EvoHime invariants о Core ownership, exact-call receipts и
  atomic approval redemption.
- **Retry failure может маскироваться.** `ModelRetryMiddleware` допускает
  возврат искусственного `AIMessage` после исчерпания попыток. Для Евы это
  опасно: provider failure, cancellation и unknown outcome должны оставаться
  typed terminal states, а не выглядеть как нормальный ответ модели.
- **PII detection ограничена эвристиками.** Built-in regex/validators полезны
  как deterministic baseline, но не дают полной защиты от secrets, бинарных
  payloads, произвольных credentials или prompt injection. Их нельзя считать
  заменой существующей EvoHime redaction/provenance policy.
- **Streaming redaction сложна.** Потоковые chunks требуют буферизации
  границ токенов и повторной проверки tool-call JSON; частичная обработка
  может пропустить значение, разбитое между chunks. Это только checklist и
  test design, не готовый безопасный компонент.
- **Внешняя телеметрия и ecosystem coupling.** `langchain-core` зависит от
  `langsmith`; provider integrations и tracing могут отправлять данные наружу
  при включённых переменных окружения. Для local-first Евы любой такой egress
  должен быть явным, redacted и Core-policy governed.
- **Model profiles могут устареть.** Данные приходят из внешнего
  `models.dev`, refresh выполняется отдельно и profile format помечен beta.
  Нельзя принимать capability только по имени модели; нужен provider probe,
  timestamp, version и conservative fallback.
- **Масштаб и частота изменений.** Монорепозиторий очень большой и активно
  меняется; прямое копирование отдельных API создаст долг сопровождения и
  риск подтянуть legacy/classic semantics вместо текущего v1.
- **Лицензия не снимает dependency risks.** MIT разрешает использовать код с
  сохранением copyright notice, но лицензии партнёрских SDK, моделей,
  `models.dev`, LangGraph и LangSmith нужно проверять отдельно перед любым
  включением в поставку.

#### Предварительное решение

`адаптировать контракты, поля профилей и тестовые идеи`; `не подключать
LangChain/LangGraph как runtime Евы и не переносить middleware напрямую`.

#### Связь с EvoHime

- уже покрыто и не дублировать: Core-first agent loop, structured tool
  registry, approval/receipts, run budgets, context-budget, model gateway,
  durable workflow/checkpoints, redaction и provenance;
- наиболее ценные кандидаты для будущего плана: расширение model capability
  profiles, conformance matrix для providers, полный redaction-surface audit
  и acceptance tests для approval edit/reject/respond;
- возможное заимствование кода ограничить изолированными MIT utility-идеями
  после отдельной license/security проверки; приоритет — собственная Rust
  реализация по контракту, а не Python dependency;
- критерии проверки: no new runtime/network boundary, deterministic tests без
  network, explicit provider egress, typed terminal failures, approval
  recheck после edit, bounded stream redaction и conservative profile fallback.

### 3. LlamaIndex

- Источник: https://github.com/run-llama/llama_index
- Дата проверки: 2026-08-21
- Ревизия/commit: `d8021225eb7e7b276d5ceb476b0a4650240f27f8`
- Лицензия исходного кода: MIT
- Состав: Python-монорепозиторий с отдельным `llama-index-core`, большим
  набором package-level integrations, workflow runtime и instrumentation;
  starter-пакет `llama-index` добавляет core и OpenAI integrations.
- Версия core на момент проверки: `0.14.24`, Python `>=3.10,<4.0`.
- Назначение: framework для agentic applications, document parsing,
  ingestion, индексов, retrieval, query engines, tools, workflows и
  evaluation; README также выделяет внешний Parse/Extract/Index продукт.
- Краткий вывод: сильный источник архитектурных контрактов и evaluation
  сценариев для локального RAG Евы. Подключать Python framework или внешние
  LlamaCloud/connector services в runtime EvoHime не следует.

#### Что изучено

- `llama-index-core` разделяет `Document`, `BaseNode`, `TextNode` и
  `NodeWithScore`: узлы имеют стабильный `id_`, metadata, hash, embedding и
  явные отношения `SOURCE`, `PREVIOUS`, `NEXT`, `PARENT`, `CHILD`;
  `RelatedNodeInfo` переносит node id, type, metadata и hash без обязательной
  загрузки полного текста.
- Metadata может отдельно исключаться из текста для embedding и для LLM;
  `MetadataMode` даёт режимы `ALL`, `EMBED`, `LLM`, `NONE`. Это полезное
  разделение между поисковым представлением и доказательным контекстом.
- `IngestionPipeline` оформляет ingestion как последовательность
  transformations. В нём есть cache, docstore, стратегии `UPSERTS`,
  `DUPLICATES_ONLY`, `UPSERTS_AND_DELETE`, optional vector store и async
  execution. Кэш и dedup завязаны на hash исходных документов/узлов.
- Vector index/retriever возвращает `NodeWithScore`, а response/query
  engine сохраняет `source_nodes`; citation и multi-step/router/sub-question
  engines умеют объединять источники нескольких подзапросов.
- `StorageContext` группирует docstore, index store, vector stores и graph
  stores и умеет сохранять/восстанавливать их из каталога. По умолчанию
  используются простые локальные хранилища, но plugin surface позволяет
  подключать внешние vector/database backends.
- Workflow слой использует типизированные `Event`, `StartEvent`, `StopEvent`,
  `Context` и `@step`; граф шагов можно визуализировать по accepted events и
  return types. Retry policy вынесена в отдельный workflow package.
- `FunctionTool` связывает имя/описание и schema аргументов с sync/async
  функцией и возвращает typed `ToolOutput` с raw input/output и content
  blocks. Есть адаптеры к другим tool-форматам, но security approval в
  `FunctionTool` не является самостоятельной границей.
- Evaluation API задаёт общий `EvaluationResult` с `query`, `contexts`,
  `response`, `passing`, `feedback`, `score` и invalid-result marker. В core
  есть evaluator-ы faithfulness, context/answer relevancy, correctness,
  semantic similarity, pairwise и retrieval, а также batch runner.
- Core зависит от достаточно широкого Python stack: Pydantic 2, SQLAlchemy,
  fsspec, httpx/requests/aiohttp, numpy, tiktoken, NLTK, Pillow, NetworkX,
  SQLite async bindings и tenacity. Core отдельно исключает workflow и
  instrumentation из coverage-отчёта.
- Внешние integrations и instrumentation выделены в отдельные пакеты;
  observability может подключать сторонние backend-ы. Это полезный список
  событий и spans для сравнения, но не разрешение на передачу prompt/tool
  данных наружу.

#### Что можем использовать в Еве

- **RAG metadata/provenance contract.** Сопоставить текущую схему chunk-а с
  полями stable id, source document id, content hash, source/parent/next
  relationships и score. Идея `RelatedNodeInfo` особенно полезна для
  компактного provenance без копирования полного текста. Не заменять
  текущие generation/hash/citation gates EvoHime.
- **Разделение представлений metadata.** Использовать отдельные правила для
  metadata, разрешённых в embedding-представлении, в LLM-контексте и в UI
  citation. В Еве это должно быть Core-owned и redaction-aware, чтобы
  исключённое из embedding поле не стало случайно доступным модели через
  другой путь.
- **Ingestion как наблюдаемая цепочка трансформаций.** Зафиксировать в
  acceptance tests этапы scan → normalize → chunk → metadata → embedding →
  publication и для каждого этапа hash/version/input-output counts. Это
  помогает диагностировать cache hits, upsert и stale generation, но
  реализацию оставить в существующем Rust workspace RAG pipeline.
- **Dedup/upsert test matrix.** Перенять комбинацию сценариев: новый
  документ, неизменённый документ, изменённый документ, удалённый документ,
  duplicate hash, частично упавшая трансформация и повтор после отмены.
  Результатом должна быть атомарная опубликованная generation, а не
  промежуточно видимый индекс.
- **Retriever/source-node boundary.** Закрепить контракт, где retrieval
  выдаёт scored evidence с node id, score, generation и provenance, а
  synthesis получает только проверенный набор source nodes. Для сложных
  запросов полезны acceptance cases на router, sub-question и merge источников
  с дедупликацией citations.
- **Local storage decomposition как сравнительный reference.** Разделение
  document/index/vector/graph store помогает проверить, что SQLite FTS5,
  optional local embeddings, context ledger и citation metadata Евы не
  смешивают поисковый индекс с источником истины. Новые внешние vector DB для
  этого не нужны.
- **Event-driven workflow vocabulary.** Использовать typed events, step
  inputs/outputs, cancellation, retry policy и визуализируемую схему как
  материал для acceptance tests текущего Rust workflow runtime: особенно
  resume после approval, timeout, retryable provider error и terminal failure.
- **Tool output contract.** Идея отдельного `ToolOutput` с typed content,
  raw input/output и tool name полезна для receipts, artifact references и
  UI-диагностики. В Еве raw output должен проходить redaction/size limits и
  храниться только в разрешённой форме; `FunctionTool` нельзя считать
  заменой capability/approval policy.
- **RAG evaluation fixtures.** Перенять разделение оценок retrieval
  (recall/релевантность контекста) и generation (faithfulness, answer
  relevancy, correctness), сохраняя отдельно query, retrieved contexts,
  response, score, pass/fail, feedback и invalid reason. Это хороший каркас
  для offline acceptance suite Евы.
- **Instrumentation checklist.** Использовать event/span vocabulary для
  измерения latency, cache hit, retrieval scores, token/cost и workflow
  transitions. События должны создаваться Core, быть redacted и иметь
  correlation/run id; внешние exporters — только явный opt-in.
- **Мультимодальный ingestion как дальняя идея.** Поддержка media resources
  и modality-specific embeddings может быть полезна для будущих локальных
  документов/изображений, но не должна расширять текущий scope без отдельной
  модели приватности и bounded storage.

#### Ограничения и риски

- **Несовместимый runtime и размер dependency surface.** LlamaIndex — Python
  framework с Pydantic, сетевыми клиентами, численными и parser-зависимостями;
  добавление его в Windows desktop Core создаст второй runtime, упаковку,
  обновления и отдельную поверхность уязвимостей.
- **Слишком широкий plugin/egress surface.** Интеграции охватывают cloud
  providers, vector stores, readers и observability. Наличие адаптера не
  означает, что данные можно отправлять во внешний сервис; ключи и prompt
  содержимое должны оставаться под текущей Core policy.
- **LlamaParse/Parse — внешняя сервисная зависимость.** OCR, extraction и
  indexing через облачные продукты могут раскрывать документы и требуют
  отдельного consent, network policy, billing, retention и license review.
  По умолчанию для Евы это отклонённый путь.
- **Persistence может обойти текущие гарантии.** Простая сериализация
  `storage`/vector store может сохранить raw text, metadata или embeddings
  вне EvoHime SQLite, не зная о redaction, generation, stale-evidence,
  backup и atomic-publication правилах. Использовать только как comparison
  reference.
- **Generic agent/tool API не является security boundary.** `FunctionTool`,
  workflow и agent loops не заменяют capability checks, approval, exact-call
  hash, cancellation, supervisor limits или receipts Евы. Нельзя давать им
  прямой доступ к workspace и model secrets.
- **Cache/hash semantics недостаточны сами по себе.** Hash документа может не
  учитывать смену parser/chunker/embedding/model policy. В Еве любой reuse
  должен быть привязан к generation, schema/pipeline version и model profile,
  иначе появятся stale citations и несовместимые embeddings.
- **LLM-based evaluation не является доказательством.** Faithfulness,
  correctness и relevancy evaluators требуют модели-судьи и могут зависеть от
  network/provider. Их результат — измерение качества с invalid/uncertain
  состоянием, а не security decision.
- **Лицензирование интеграций неоднородно.** MIT относится к исходному core;
  отдельные readers, provider SDK, модели, LlamaCloud и observability
  backends могут иметь собственные лицензии и условия.
- **Документация и код меняются быстро.** README прямо направляет к текущей
  документации; перед любым заимствованием нужно снова сверять live source,
  package versions, tests и конкретную лицензию. Полный test suite LlamaIndex
  в рамках записи не запускался.

#### Предварительное решение

`адаптировать RAG/ingestion/workflow/evaluation контракты и тестовые идеи`;
`не подключать LlamaIndex runtime, LlamaParse или cloud integrations в Еву`.

#### Связь с EvoHime

- уже покрыто и не дублировать: локальный Agentic RAG, canonical workspace
  scan, versioned chunking, SQLite FTS5, optional local embeddings, RRF,
  citations с re-read verification, context ledger, redaction и stale-evidence
  gates;
- наиболее ценные кандидаты для будущего плана: единый RAG evaluation suite,
  dedup/upsert failure matrix, metadata/provenance contract audit,
  retriever/source-node contract и workflow observability checklist;
- реализацию держать в Rust Core и существующих SQLite/IPC контрактах; Python
  использовать только во внешнем offline benchmark, если это когда-нибудь
  понадобится, без включения в поставку Евы;
- критерии проверки: atomic generation publication, parser/chunker/model
  version in reuse key, no raw-text egress, redacted Core-owned events,
  deterministic retrieval fixtures, explicit invalid evaluation state,
  cancellation/timeout/retry tests и сохранение approval/receipt invariants.

## Итог для будущего плана

Этот раздел заполняется после завершения набора исследований:

- подтверждённые возможности для интеграции;
- идеи, которые реализуем самостоятельно без заимствования кода;
- внешние компоненты, допустимые после проверки лицензии;
- отклонённые варианты и причины;
- зависимости, порядок этапов и критерии готовности.
