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
| 4 | [openinterpreter/openinterpreter](https://github.com/openinterpreter/openinterpreter) | Исследовано | Execution policy, sandbox, approvals, ACP и typed protocol | Адаптировать security/protocol идеи; runtime не подключать |
| 5 | [OthersideAI/self-operating-computer](https://github.com/OthersideAI/self-operating-computer) | Исследовано | Computer-use loop, screenshot/OCR и action schema | Адаптировать идеи; прямое управление desktop не включать по умолчанию |
| 6 | [simular-ai/Agent-S](https://github.com/simular-ai/Agent-S) | Исследовано | Разделение worker/grounding, reflection, bounded trajectory и UI evaluation | Адаптировать computer-use/evaluation идеи; runtime не подключать |

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

### 4. Open Interpreter

- Источник: https://github.com/openinterpreter/openinterpreter
- Дата проверки: 2026-08-21
- Ревизия/commit: `5b07159c477920c159d8892d112b480e7307f257`
- Лицензия исходного кода: Apache-2.0; в репозитории также есть `NOTICE` и
  унаследованные upstream-компоненты, поэтому attribution нужно сохранять при
  любом переносе кода.
- Состав: текущий Rust-монорепозиторий, основанный на OpenAI Codex, с CLI/TUI,
  app-server JSON-RPC, ACP server, MCP, SDK, harness-ами, skills/hooks,
  execution policy, sandboxing и отдельным native Windows sandbox crate.
- Назначение: coding agent для open/low-cost моделей, совместимый с Codex
  exec/app-server surfaces и ACP-клиентами. README отдельно указывает, что
  старый Python-проект теперь живёт в community fork.
- Краткий вывод: один из самых полезных источников для security-контрактов
  выполнения команд в Еве: разделение approval и sandbox, typed permission
  profiles, fail-closed поведение, Windows ACL/restricted-token/Job Object
  механизмы, streaming protocol и conformance tests. Целиком переносить
  Codex runtime в EvoHime не нужно.

#### Что изучено

- В текущем репозитории основной runtime — Rust, а не исторический Python
  Open Interpreter. Пространство разделено на `core`, `protocol`, `sandboxing`,
  `windows-sandbox-rs`, `app-server`, `acp-server`, SDK и отдельные crates для
  tools, skills, MCP, network proxy и process hardening.
- Модель безопасности разделяет две независимые оси: sandbox задаёт
  техническую границу локального выполнения, approval policy определяет, когда
  нужно остановить agent и запросить решение пользователя.
- Sandbox-профили включают `read-only`, `workspace-write`,
  `danger-full-access` и `external-sandbox`; filesystem и network policy
  сериализуются отдельно. `workspace-write` задаёт writable roots, умеет
  отключить tmp roots и по умолчанию не включает сеть.
- Approval policy имеет `untrusted`, `on-request`, granular и `never`.
  Granular-конфигурация отдельно управляет sandbox escalation, execpolicy
  rules, skill approval, request_permissions и MCP elicitations.
- Writable roots защищают control directories: `.git`, `.codex` и agent
  configuration paths должны оставаться read-only либо deny-write даже внутри
  разрешённого workspace. В policy есть read-only subpaths и protected
  metadata names.
- Sandbox manager выбирает platform backend и преобразует typed policy в
  spawn parameters. Для macOS используется Seatbelt, для Linux/WSL —
  Bubblewrap/Landlock и связанные kernel limits, для Windows — native
  restricted-token/elevated backends с ACL-подготовкой.
- Windows crate содержит Job Object для дерева процессов, ACL deny-read и
  deny-write, restricted token/elevation paths, ConPTY/stdio bridge, helper
  materialization, filesystem permission resolution и smoke/integration tests.
  Для неподдерживаемой комбинации filesystem policy runtime не должен молча
  запускать команду без sandbox; код и тесты явно проверяют отказ.
- Network policy может работать через ограниченный local proxy; sandbox
  преобразует network access в разрешённые proxy endpoints/ports и имеет
  fail-closed ветки при невозможности применить managed network requirements.
- App-server предоставляет typed JSON-RPC поверх stdio: initialize,
  thread/turn lifecycle, streaming agent deltas, command/file-change output,
  tool progress, approval requests, interrupt/steer, configuration и
  generated stable/experimental JSON/TypeScript schemas.
- ACP server даёт отдельный stdio-протокол для editor/UI: клиент владеет UI,
  а agent — model/provider, tools, approvals, sandbox и session state. Через
  ACP передаются streaming messages, tool progress и permission requests без
  scraping терминального UI.
- TypeScript и Python SDK запускают внешний agent process, маршрутизируют
  JSON-RPC responses/notifications по thread/turn и умеют stream текста. В
  compatibility script есть provider-free smoke test для запуска реального
  binary через Codex SDK.
- Python SDK по умолчанию принимает command/file approval, если caller не
  передал свой `approval_handler`. Это удобный пример API, но опасное default
  поведение для desktop-продукта.
- README/документация рекомендуют `read-only + on-request` для незнакомого
  кода, `workspace-write + on-request` для обычной работы и разрешают
  full-access/never только во внешней disposable isolation. Отдельно
  предупреждается, что `--yolo` отключает сразу approval и sandbox.
- Репозиторий содержит крупные Rust unit/integration/snapshot suites,
  generated protocol schemas, Windows sandbox smoke tests и тесты policy
  transforms, protected paths, network proxy, approval routing и unsupported
  backend behavior. Полный test suite в рамках записи не запускался.

#### Что можем использовать в Еве

- **Разделение sandbox и approval как двух контрактов.** Сверить текущие
  `Permission`, approval redemption и tool policy Евы с явной моделью:
  permission profile отвечает за технические возможности процесса, approval —
  за пользовательское решение. Ни один из них не должен подменять другой.
- **Typed permission profile.** Использовать структуру filesystem roots,
  deny/read-only carve-outs, network mode, escalation capability и source/hash
  policy snapshot для receipts. Profile должен входить в execution context и
  быть повторно проверен перед запуском, а не жить только в Electron UI.
- **Protected control paths.** Добавить в acceptance/security matrix Евы
  проверки, что `.git`, hooks, `.codex`/EvoHime runtime state, secrets,
  supervisor/session files и policy/config directories не становятся
  writable вследствие общего writable root. Отдельно проверять symlink,
  junction, rename и create-child обходы.
- **Windows-native executor checklist.** Использовать Windows-часть как
  reference для ревью supervisor/Core execution: Job Object для всего дерева,
  restricted token, ACL deny rules, ConPTY isolation, bounded stdio и явный
  cleanup. Код напрямую не переносить без проверки совместимости с текущим
  supervisor и ownership модели EvoHime.
- **Fail-closed capability negotiation.** Если Windows backend не может
  выразить requested policy, возвращать typed unsupported/denied result и не
  запускать команду unsandboxed. Это особенно важно для split read/write
  roots, deny-read paths и network restrictions.
- **Approval request protocol.** Перенять typed request/response shape с
  tool/command id, requested filesystem/network delta, policy snapshot, human
  explanation, expiry и decision. Любое edit/escalation решение должно
  canonicalize command, пересчитать hash и пройти Core policy ещё раз.
- **Granular approval matrix.** Разделить approval для shell escalation,
  policy-rule exception, skill script, permission request и MCP elicitation.
  Это даст пользователю точное управление, не превращая все опасные действия
  в один общий prompt.
- **Transport/schema discipline.** Использовать идеи generated stable vs
  experimental schemas, major/version negotiation, typed notifications,
  request correlation, streaming deltas и explicit interrupt. Для Евы это
  нужно адаптировать к каноническому `desktop-ipc-v1` через named pipe, не
  заменять существующий IPC на stdio.
- **ACP как внешний compatibility adapter.** Рассмотреть ACP только как
  будущий opt-in adapter для editor/automation interoperability. Внутри Евы
  Core остаётся владельцем состояния, tools, approvals и secrets; ACP-клиент
  не получает прямого доступа к workspace/SQLite.
- **Binary compatibility smoke tests.** Перенять проверку реального binary
  через SDK/app-server: initialize, thread start/resume, stream, approval,
  interrupt и terminal result. Это полезно для Electron/Core IPC E2E и
  supervisor recovery tests.
- **Portable instruction/skills boundary.** Идея shared `AGENTS.md`,
  `.agents/skills`, MCP и protocol-neutral directories полезна для
  interoperability. В Еве содержимое таких файлов должно считаться
  untrusted context и проходить prompt-injection/policy handling; оно не может
  менять Core permissions или approval state.
- **Execution observability.** Перенять события для command start/output/
  completion, sandbox violation, approval requested/decided, process exit,
  timeout, cancellation и network denial. События должны быть redacted,
  correlation-id based и записываться Core/supervisor, а не только UI.
- **Windows negative-test fixtures.** Добавить сценарии: запись за пределами
  root, изменение `.git/hooks`, чтение deny-read secret, rename через parent,
  child-process escape, network access при disabled policy, unsupported split
  policy и cleanup после crash.

#### Ограничения и риски

- **Это не независимый маленький Open Interpreter runtime.** Текущий проект —
  большой fork Codex с сотнями Rust crates, upstream protocol assumptions,
  provider/auth surfaces и быстро меняющимся app-server API. Прямая зависимость
  добавила бы второй agent runtime и усложнила существующий EvoHime Core.
- **Apache-2.0 требует соблюдения attribution/NOTICE.** Даже при допустимом
  reuse нужно сохранять license/notice, отмечать изменённые файлы и отдельно
  проверять лицензии зависимостей, bundled providers, моделей и внешних tools.
  Название Open Interpreter/OpenAI и trademarks не становятся свободными для
  использования вместе с кодом.
- **Full-access и never опасны по дизайну.** Документация прямо допускает
  режимы, где sandbox или approval отсутствуют, но только во внешней
  disposable isolation. Еве нельзя выставлять такие комбинации как обычный
  пользовательский профиль или скрытый fallback.
- **Approval SDK может auto-accept.** Python SDK default handler принимает
  approvals без caller-supplied handler. При заимствовании протокола это нужно
  запретить: отсутствие обработчика должно быть deny/blocked, а решение —
  только из доверенного UI/Core path.
- **Sandbox не равен полной безопасности agent.** Даже хороший OS boundary не
  предотвращает prompt injection внутри разрешённого workspace, утечку через
  разрешённую сеть, опасную команду в `danger-full-access`, заражённый MCP или
  ошибку policy translation. Нужны отдельные Core policy, redaction и audit.
- **Windows backend имеет ограничения покрытия.** ACL/restricted-token paths
  различаются по правам, elevation, deny-read и writable-root форме. Нельзя
  считать одну успешную подготовку helper-а доказательством защиты всех
  комбинаций; сохранять negative tests и fail-closed behavior.
- **External protocol увеличивает поверхность.** ACP/app-server/stdin JSON-RPC
  требуют bounded frames, authentication/ownership, correlation, cancellation
  и защиты от spoofed approval responses. Для локального IPC Евы named pipe с
  session authentication остаётся более подходящей границей.
- **Shared instructions могут быть атакующим входом.** `AGENTS.md`, skills,
  MCP и подключённые editors могут передать инструкции, которые выглядят как
  policy. Их нельзя принимать как trusted authority или разрешать им менять
  secrets, permission profile и approval.
- **Кодовая совместимость не означает продуктовую совместимость.** Runtime
  рассчитан на Codex process layout, его config/home, provider auth и rollout
  storage. Для EvoHime можно использовать контракты и тесты, но не копировать
  пути, env names, state ownership или UI semantics без отдельного решения.

#### Предварительное решение

`адаптировать permission/sandbox/approval/protocol контракты и Windows
negative-test идеи`; `не подключать Open Interpreter/Codex runtime как
внутреннюю зависимость Евы`.

#### Связь с EvoHime

- уже покрыто и не дублировать: Rust Core, supervisor/Job Object lifecycle,
  named-pipe desktop IPC с session authentication, capability registry,
  approval/receipts, tool sandbox/timeout/cancellation, provider gateway,
  redaction и Core-owned state;
- наиболее ценные кандидаты для будущего плана: Windows sandbox capability
  matrix, protected-path negative tests, explicit fail-closed unsupported
  policy, granular approval schema, execution-event audit и real-binary IPC
  compatibility suite;
- ACP/app-server рассматривать только как внешний interoperability layer после
  стабилизации собственного `desktop-ipc-v1`; не переносить stdio transport
  внутрь продукта и не давать editor/client прямого доступа к Core storage;
- критерии проверки: no unsandboxed fallback, policy/approval hash recheck,
  deny-read/write and child-process escape tests, bounded protocol frames,
  authenticated approval responses, cancellation/cleanup after crash,
  redacted audit events и сохранение Core/supervisor ownership invariants.

### 5. Self-Operating Computer

- Источник: https://github.com/OthersideAI/self-operating-computer
- Дата проверки: 2026-08-21
- Ревизия/commit: `fac568eea7da5e24f8bc91bfc1211b65679177eb`
- Последний commit в checkout: 2025-09-19, `Fix typo in README description`.
- Версия пакета: `1.5.8`.
- Лицензия исходного кода: MIT, copyright OthersideAI 2023.
- Состав: Python CLI `operate`, `pyautogui` desktop control, screenshots,
  OCR через EasyOCR, YOLOv8 Set-of-Mark labels, OpenAI/Anthropic/Google/Qwen
  adapters, Ollama/LLaVA local path, optional voice mode и простой `evaluate.py`.
- Назначение: multimodal model получает screenshot и objective, возвращает
  JSON-последовательность действий `click`, `write`, `press` или `done`, после
  чего framework исполняет их на локальном desktop. README заявляет поддержку
  Windows, macOS и Linux с X server.
- Краткий вывод: полезный ранний reference для computer-use action contract,
  coordinate normalization, OCR/SoM grounding, screenshot compression и
  goal-based evaluation. Это не безопасный execution runtime для Евы:
  действия выполняются напрямую через `pyautogui`, без sandbox, per-action
  approval, capability policy, provenance или redaction.

#### Что изучено

- Основной цикл в `operate/operate.py`: получает objective, строит system
  prompt, до десяти раз запрашивает у модели следующий action batch, исполняет
  его и завершает работу по `done` либо после лимита loop count.
- Канонический action schema предельно мала: `click` с координатами или OCR
  text, `write` с текстом, `press`/hotkey со списком клавиш и `done` с summary.
  Модель может вернуть несколько действий за один ответ.
- Координаты могут передаваться как доля ширины/высоты экрана. Runtime
  переводит проценты в текущие pixel dimensions, перемещает мышь и кликает.
  Для `press` клавиши нажимаются и отпускаются группой; `write` печатает
  посимвольно через `pyautogui`.
- Перед каждым vision request делается screenshot с курсором. Для разных
  providers изображение отправляется как base64 data URL, JPEG сжатие либо
  resize до ограниченного размера. В history сохраняются system prompt,
  user image message и JSON ответа assistant.
- OCR path получает текст кнопки от модели, запускает EasyOCR по screenshot,
  ищет совпадение, вычисляет центр bounding box и подставляет координаты.
  Set-of-Mark path использует YOLOv8 для красных `~N` labels и таблицу
  bounding-box coordinates.
- Prompt templates специально ограничивают модель четырьмя операциями и
  содержат platform-specific советы для открытия браузера, поиска приложений,
  ввода URL и повторного осмотра screenshot после действия.
- Provider adapters поддерживают GPT-4o/GPT-4.1/o1 vision, Claude 3, Gemini,
  Qwen-VL и Ollama/LLaVA. При ошибке JSON/provider/OCR код меняет prompt либо
  откатывается к GPT-4o; некоторые fallback-пути вызываются рекурсивно.
- Конфигурация загружает `.env`, принимает несколько API keys и при первом
  запуске дописывает введённый ключ в локальный `.env`. Ollama может работать
  через локальный host, но базовый cloud flow отправляет полные screenshots
  выбранному provider.
- `evaluate.py` содержит только два сценария (`Go to Github.com` и `Go to
  Youtube.com and play a video`), после выполнения проверяет финальный
  screenshot отдельным GPT-4o judge и ожидает строгое JSON `guideline_met` и
  `reason`.
- В репозитории нет отдельного sandbox, permission profile, action approval,
  window/app allowlist, secret redaction, signed receipt или typed durable
  session state. Ошибки низкоуровневого keyboard/mouse вызова печатаются и
  подавляются.
- Полный test suite не запускался; анализ выполнен по исходникам, setup,
  requirements, README и evaluation harness. Последняя ревизия заметно
  старше текущей даты, поэтому provider APIs и инструкции требуют повторной
  проверки перед любым использованием.

#### Что можем использовать в Еве

- **Минимальный computer-use action contract.** Взять как основу отдельного
  будущего typed tool: `click`, `type`, `key_press`, `scroll`, `wait`, `finish`,
  где каждый action имеет id, display/window target, coordinate frame, reason,
  timeout, expected effect и policy classification. Четыре операции проекта —
  хороший baseline, но для Евы их нужно расширить только после capability
  design.
- **Screenshot provenance.** Каждый кадр должен иметь capture id, timestamp,
  display/window bounds, DPI/scale, cursor state, image hash и redaction
  status. Action обязан ссылаться на конкретный кадр; после изменения окна
  координатный план устаревает и требует нового observation.
- **Coordinate normalization.** Нормализация в относительные координаты
  переносима между разрешениями, но должна включать display id, viewport,
  scale factor и bounds validation. Перед кликом Core должен проверить, что
  target остаётся в разрешённом окне и не изменился после screenshot.
- **OCR/visual grounding fallback.** Сочетание vision model, OCR text lookup и
  visual labels можно использовать как альтернативные способы grounding:
  text → bounding box → center и label → box → click. В Еве результат должен
  быть evidence с confidence, source frame и bounded ambiguity; низкая
  уверенность ведёт к запросу пользователя, а не к повторному клику.
- **Screenshot compression policy.** Сжатие/resize перед cloud vision уменьшает
  cost и latency. В Еве сначала применять deterministic redaction/region crop,
  затем bounded JPEG/PNG encoding; оригинал хранить локально только при
  явной необходимости и с lifecycle/retention policy.
- **Observation-action loop.** Полезен общий контракт: capture → model plan →
  validate → approval/policy → execute one bounded action → capture again →
  verify expected state. Нельзя исполнять произвольный batch действий из одного
  ответа без повторной проверки каждого side effect.
- **Goal-based UI evaluation.** Идея objective + observable final-state
  guideline полезна для offline fixtures: «страница открыта», «диалог закрыт»,
  «файл сохранён». В Еве judge должен дополняться deterministic UI/API/state
  checks, а LLM-оценка оставаться advisory и иметь `unknown`/invalid result.
- **Provider capability matrix.** Набор разных vision/local providers может
  стать fixture matrix для model gateway: image input, OCR, structured JSON,
  max image size, latency, local/cloud egress и fallback behavior.
- **Voice-to-objective boundary.** Voice mode можно рассматривать как отдельный
  input adapter: audio → transcript → user confirmation → objective. Голос не
  должен напрямую превращаться в desktop side effect без тех же approval,
  preview и audit правил.
- **Human-visible action preview.** Текстовая печать `thought/action` в CLI
  показывает полезный UX-паттерн, но в Еве превью должно показывать нормализованный
  target, actual key sequence, redacted text, risk и ожидаемый эффект, а не
  доверять свободному model thought.
- **Computer-use evaluation fixtures.** Перенять классы сценариев для будущего
  benchmark: browser navigation, login form без отправки секрета, file picker,
  dialog confirmation, wrong-window focus, DPI scaling, OCR ambiguity,
  disappearing target, timeout и cancel during action.

#### Ограничения и риски

- **Нет технической sandbox boundary.** `pyautogui` управляет реальным
  пользовательским desktop; модель может открыть сайты, ввести текст, нажать
  подтверждение или отправить данные. В проекте нет capability allowlist,
  restricted desktop/session, window isolation или Core-owned approval.
- **Cloud egress screenshots.** Полный экран может содержать пароли,
  переписку, документы, токены и уведомления. Код отправляет base64 screenshot
  внешнему provider без обязательной redaction, region crop, consent или
  provenance ledger.
- **Действия не подтверждаются поштучно.** Ответ может содержать несколько
  кликов/вводов, и `operate()` исполняет их последовательно после одного ответа
  модели. Ошибка после частичного исполнения не даёт безопасного rollback.
- **Координаты хрупки и гоняются с UI.** Изменение окна, DPI, scaling,
  multi-monitor layout, cursor position, animation или modal dialog может
  направить клик в другой target. OCR match по substring также может выбрать
  неоднозначный элемент.
- **Низкоуровневые ошибки подавляются.** `OperatingSystem.write/press/mouse`
  печатают exception и продолжают цикл; это затрудняет точный terminal state,
  receipt и безопасную компенсацию.
- **JSON-контракт не валидируется схемой.** `clean_json` обрезает markdown
  fences и затем вызывает `json.loads`; нет строгой Pydantic/JSON Schema
  валидации диапазонов координат, допустимых клавиш, размера текста и числа
  действий.
- **Fallback может повторить side effect.** После ошибки provider/OCR код
  возвращается к другому model path, а уже изменённая history может повторно
  запланировать действие. Нужны idempotency/action hash и explicit unknown
  outcome, которых здесь нет.
- **Секреты сохраняются небезопасно.** Введённые API keys дописываются в
  plaintext `.env`; это несовместимо с DPAPI/safeStorage и Core-owned secret
  boundary EvoHime.
- **Старые и тяжёлые зависимости.** Requirements содержат жёстко pinned
  версии 2023 года, одновременно тянут OpenAI/Anthropic/Google/Ollama,
  EasyOCR, Ultralytics, PyTorch-related stack и platform GUI dependencies.
  Это повышает packaging, supply-chain и update risks, а лицензии моделей и
  weights нужно проверять отдельно.
- **Evaluation слабая и зависит от judge.** Два сценария и один финальный
  screenshot не обнаруживают промежуточный вред, утечку данных, неправильное
  окно или ошибочное действие; GPT judge сам может ошибиться.
- **Проект, вероятно, не является текущим источником computer-use API.** При
  последнем commit в 2025 году его prompts/providers отражают старые модели и
  форматы. Использовать как исторический reference и test-idea source, а не
  как актуальный provider abstraction.

#### Предварительное решение

`адаптировать action/screenshot/grounding/evaluation идеи для отдельного
computer-use capability`; `не подключать PyAutoGUI framework и не разрешать
прямое desktop управление без отдельного sandbox, approval и audit дизайна`.

#### Связь с EvoHime

- уже покрыто и не дублировать: Core-owned tools, capability/approval policy,
  supervisor lifecycle, bounded execution, model gateway, redaction,
  provenance, cancellation и receipts;
- наиболее ценные кандидаты для будущего плана: отдельный computer-use
  action schema, screenshot provenance/redaction, OCR/vision grounding,
  observation-action verification loop и UI evaluation fixtures;
- desktop control должен быть отдельным opt-in capability с default deny,
  ограниченными окнами/приложениями, подтверждением опасных actions и
  локальным/изолированным vision path; renderer не должен вызывать pyautogui
  или provider напрямую;
- критерии проверки: no raw-screen egress by default, bounded/cropped/redacted
  screenshots, frame-bound action hashes, target revalidation, per-action
  approval for side effects, typed unknown outcomes, cancel/timeout cleanup,
  deterministic UI fixtures и сохранение Core/supervisor ownership.

### 6. Agent S

- Источник: https://github.com/simular-ai/Agent-S
- Дата проверки: 2026-08-21
- Ревизия/commit: `bffdb59c60cbbb38c3a190b2e91da12039e4063c`
- Последний commit в checkout: 2026-07-31, `Update README.md`.
- Лицензия исходного кода: Apache-2.0.
- Версия Python-пакета в `setup.py`: `0.3.2`; заявлен Python `>=3.9, <=3.12`.
- Состав: поколения `s1`, `s2`, `s2_5`, `s3`, Python CLI/API, visual
  grounding через UI-TARS-подобную модель, OCR/Tesseract, optional local
  code environment, BBoN/comparative judge и evaluation sets для OSWorld,
  WindowsAgentArena и AndroidWorld.
- Назначение: framework для автономного управления GUI через Agent-Computer
  Interface; README позиционирует Agent S3 как исследовательскую систему для
  сложных desktop-задач и приводит benchmark-результаты.
- Краткий вывод: это самый содержательный из изученных computer-use
  reference по разделению планирования и grounding, bounded visual history,
  reflection и offline evaluation. Его runtime нельзя переносить в Еву:
  модель генерирует исполняемый Python-код для `pyautogui`, а включаемый
  `LocalEnv` запускает произвольный Python/Bash с правами текущего пользователя.

#### Что изучено

- `AgentS3` создаёт `Worker`, который получает инструкцию и screenshot,
  хранит ограниченную историю визуальной траектории (по умолчанию 8 image
  turns), при необходимости запускает отдельный reflection agent и передаёт
  результат следующему шагу генерации.
- `OSWorldACI` отделяет worker-модель от grounding-модели: UI-TARS-подобный
  endpoint получает screenshot и описание элемента, возвращает координату,
  после чего ACI масштабирует её из `grounding_width/height` в фактический
  размер экрана. Есть отдельный OCR-путь через Tesseract для поиска слов и
  bounding boxes.
- ACI-примитивы (`click`, `type`, `drag_and_drop`, `scroll`, `hotkey`, `open`,
  `done` и другие) не исполняют действие сами, а строят Python-строку с
  `pyautogui`; README показывает финальный `exec(action[0])` на стороне
  вызывающего кода.
- `enable_local_env` подключает `LocalEnv`: Python выполняется через
  `sys.executable -c`, Bash — через `/bin/bash -lc` с timeout 30 секунд для
  shell-пути. В CLI есть явное предупреждение, но нет отдельной sandbox,
  capability policy или per-action approval.
- Worker передаёт в reflection не только текущий screenshot, но и историю
  последнего действия; reflection классифицирует прогресс/циклы и возвращает
  совет worker-модели. История изображений обрезается отдельным flush-путём
  для long-context и non-long-context моделей.
- В дереве есть BBoN/comparative judge для выбора лучшей из нескольких
  траекторий и evaluation-наборы. Это полезно для offline benchmark, но не
  должно означать автоматический запуск нескольких desktop-траекторий на
  пользовательской машине.
- README описывает Linux/macOS/Windows, single-monitor ограничение,
  обязательное согласование grounding-разрешения с координатами, Tesseract,
  несколько cloud providers и локальные/vLLM/Hugging Face endpoints.
- Исследование выполнено по checkout, README, setup/requirements, S3 agents,
  grounding, worker, local environment, memory prompts и evaluation-коду;
  зависимости не устанавливались, GUI и внешние model endpoints не
  запускались.

#### Что можем использовать в Еве

- **Разделение planner и visual grounding.** В будущей computer-use
  capability можно оставить сильную worker-модель для намерения/плана и
  отдельный grounding adapter для OCR/визуального поиска target. Это лучше
  оформлять как typed evidence, а не как доверенную координату: Core должен
  проверить confidence, frame id, bounds и допустимое окно перед действием.
- **Bounded trajectory и reflection.** Ограниченная история кадров, действий,
  наблюдаемого результата и короткая reflection-проверка полезны для
  обнаружения циклов, отсутствия прогресса и устаревшего плана. В Еве
  reflection остаётся advisory signal и не получает права менять policy,
  approval, secrets или capability profile.
- **Координатный контракт.** Идею явных `grounding_width/height` можно
  перенять для контракта `display_id + viewport + DPI/scale + coordinate
  frame`. Перед исполнением нужно повторно проверить bounds, активное окно,
  frame hash и соответствие target текущему screenshot.
- **Комбинация visual/OCR grounding.** UI-TARS/vision, OCR word boxes и
  текстовый поиск можно использовать как независимые grounding strategies с
  confidence, ambiguity и provenance. Низкая уверенность должна приводить к
  запросу уточнения или отказу, а не к повторным кликам наугад.
- **Offline behavior selection.** BBoN/comparative judge полезен для
  оценки нескольких гипотез плана в изолированном benchmark или dry-run.
  Для live desktop разрешать только один выбранный план, с policy/approval и
  проверкой каждого side effect.
- **UI benchmark fixtures.** Сценарии OSWorld/WAA/AndroidWorld и локальные
  evaluation sets можно использовать как источник классов тестов: DPI и
  single/multi-monitor, wrong-window focus, OCR ambiguity, disappearing
  target, modal dialog, cancel/timeout, observable final state и recovery.
  Результаты Agent S фиксировать как внешние research claims, а не как
  гарантию качества EvoHime.
- **Модельная матрица.** Раздельные параметры main provider и grounding
  provider дают хороший шаблон для capability matrix model gateway: vision,
  structured output, максимальный размер изображения, latency, local/cloud
  egress и fallback. Локальный endpoint должен быть явным opt-in с
  проверкой происхождения модели и лицензии весов.
- **Action preview и наблюдаемость.** Информация `plan`, `plan_code`,
  `reflection`, frame и execution result показывает, какие поля пригодны для
  audit/UX. В Еве нужно показывать нормализованный target, риск, redacted
  input и ожидаемый эффект, а не свободный model thought и не сырой Python.

#### Ограничения и риски

- **Произвольное выполнение кода.** ACI формирует Python-строки, а пример
  README исполняет их через `exec`. `LocalEnv` дополнительно допускает
  произвольный Python/Bash от имени текущего пользователя; это несовместимо
  с Core-owned tool policy, supervisor sandbox и approval boundary Евы.
- **Нет доказанной границы desktop-безопасности.** В checkout нет
  permission profile, allowlist окон/приложений, per-action approval,
  signed receipt, target revalidation или rollback после частично выполненной
  траектории. Предупреждение CLI снижает риск только организационно.
- **Скриншоты могут уйти провайдеру.** Полный экран способен содержать
  пароли, документы, переписку и токены. До любого будущего computer-use
  egress нужны deterministic redaction, crop, retention и provenance policy;
  raw screen нельзя отправлять по умолчанию.
- **Координаты хрупки.** Single-monitor assumption, DPI/scaling, изменение
  окна, анимации и modal dialog могут сделать координату неверной. OCR и
  visual model также могут выбрать неоднозначный target; требуется fail-closed
  ambiguity handling.
- **Широкая Python-зависимость.** Requirements включают GUI automation,
  OCR, provider SDK и тяжёлые ML-пакеты; часть зависимостей не pinned. Это
  повышает supply-chain, packaging и maintenance cost и не соответствует
  Rust Core runtime EvoHime.
- **Лицензии моделей отдельны.** Apache-2.0 относится к коду репозитория;
  UI-TARS/другие модели, веса, inference endpoints и provider terms нужно
  проверять отдельно до любого распространения или сетевой интеграции.
- **Benchmark claims не перепроверялись.** Проценты OSWorld и других
  наборов взяты из README Agent S; в журнале они не считаются подтверждённым
  продуктовым SLA.

#### Предварительное решение

`адаптировать идеи planner/grounding, trajectory, reflection и evaluation`;
`не подключать Agent S, PyAutoGUI, LocalEnv и выполнение model-generated
Python/Bash как runtime-зависимость Евы`.

#### Связь с EvoHime

- будущий computer-use слой должен быть отдельной capability с default deny,
  Core-owned typed actions, supervisor-isolated execution, screenshot
  redaction/provenance, cancellation, timeout, receipts и per-action approval;
  Electron renderer не должен получать прямой доступ к экрану, provider или
  desktop automation.
- уже существующие в EvoHime model gateway, capability/approval policy,
  redaction, provenance, Core state ownership и supervisor lifecycle нужно
  использовать как источники истины, а не дублировать Python-слоями Agent S.
- возможная будущая работа: спроектировать computer-use action/evidence
  schema, безопасный grounding adapter и offline UI fixtures после закрытия
  базовых policy/IPC зависимостей; этот журнал не создаёт такой план.
- критерии проверки: отсутствие raw-screen egress по умолчанию, bounded и
  redacted frames, frame-bound action hash, повторная проверка target/window,
  typed unknown outcome после частичного действия, per-action approval,
  cancel/timeout cleanup и deterministic final-state tests.

## Итог для будущего плана

Этот раздел заполняется после завершения набора исследований:

- подтверждённые возможности для интеграции;
- идеи, которые реализуем самостоятельно без заимствования кода;
- внешние компоненты, допустимые после проверки лицензии;
- отклонённые варианты и причины;
- зависимости, порядок этапов и критерии готовности.
