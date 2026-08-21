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
| 7 | [lavague-ai/LaVague](https://github.com/lavague-ai/LaVague) | Исследовано | Web-agent loop, DOM/XPath grounding, driver abstraction, telemetry и QA fixtures | Адаптировать web-action/evidence/test идеи; runtime и extension не подключать |
| 8 | [microsoft/playwright](https://github.com/microsoft/playwright) | Исследовано | Изолированные BrowserContext, locators/actionability, accessibility snapshots, network policy, trace и cross-browser tests | Рассматривать как возможный isolated browser backend; не подключать до отдельного packaging/security плана |
| 9 | [puppeteer/puppeteer](https://github.com/puppeteer/puppeteer) / [pptr.dev](https://pptr.dev/) | Исследовано | Chromium-first CDP/BiDi client, BrowserContext, locators, interception, tracing и browser manager | Рассматривать как альтернативный isolated browser backend; runtime не подключать до packaging/security плана |
| 10 | [fixie-ai/ultravox](https://github.com/fixie-ai/ultravox) | Исследовано | Audio-to-LLM projector, streaming text inference, conversation KV-cache, voice dataset/evaluation pipeline | Адаптировать контракты, preprocessing и eval-идеи; модельный runtime не подключать в desktop без отдельного GPU/provider плана |
| 11 | [kyutai-labs/moshi](https://github.com/kyutai-labs/moshi) | Исследовано | Full-duplex speech-text model, Mimi streaming codec, Rust/Candle backend, binary WebSocket protocol и audio client | Рассматривать как наиболее близкий voice-runtime кандидат; адаптировать протокол/streaming идеи, runtime не подключать до PoC и security/licensing плана |
| 12 | [pipecat-ai/pipecat](https://github.com/pipecat-ai/pipecat) | Исследовано | Frame-based realtime orchestration, workers/bus/jobs, transports/serializers, RTVI, tool lifecycle, metrics и behavioral evals | Адаптировать frame/event/worker/eval контракты; Python runtime и внешние transport-интеграции не подключать |
| 13 | [openai/whisper](https://github.com/openai/whisper) | Исследовано | Локальный multilingual seq2seq ASR, 16 kHz preprocessing, 30-секундные окна, language detection, timestamps и quality fallback | Адаптировать ASR-контракты, model manifest и evaluation; Python runtime не подключать, текущий listener остаётся на whisper.cpp |

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

### 7. LaVague

- Источник: https://github.com/lavague-ai/LaVague
- Дата проверки: 2026-08-21
- Ревизия/commit: `9024bb832c40291cd012916757f27ef60469b22d`.
- Последний commit в checkout: 2025-01-21, `CI: drop cron schedule run (#633)`.
- Лицензия исходного кода: Apache-2.0.
- Версии в checkout: root package `lavague` `1.1.19`, `lavague-core`
  `0.2.35`; root зависимости допускают `lavague-core ^0.2.31`.
- Состав: Python-монорепозиторий с `lavague-core`, Selenium и Playwright
  drivers, Chrome Extension, Gradio UI, server/integrations, QA/test runner,
  retrievers, contexts и локальными сайтами для тестов.
- Назначение: web-agent framework, в котором World Model получает objective,
  состояние страницы и историю, а Action Engine компилирует инструкции в
  Selenium/Playwright action code и исполняет их.
- Краткий вывод: LaVague полезен как reference для изолированного web-agent
  слоя: DOM/XPath grounding, driver abstraction, наблюдение страницы,
  structured action schema, retriever pipeline и acceptance fixtures. Прямое
  подключение не подходит Еве: код генерируется моделью и исполняется через
  browser driver, Chrome Extension имеет `<all_urls>` и `debugger`, а
  telemetry по умолчанию отправляет подробные данные на внешний endpoint.

#### Что изучено

- `WebAgent` строит цикл с ограничением `n_steps`: получает observation,
  вызывает World Model, выбирает один из `Navigation Engine`, `Python Engine`
  или `Navigation Controls`, обновляет short-term state и снимает новую
  observation. World Model хранит objective, текущий state, предыдущие
  инструкции, последний engine и результат предыдущего шага.
- `WorldModel` передаёт multimodal LLM YAML-состояние, сведения о вкладках,
  objective и изображения из screenshot directory. Контекст и примеры можно
  добавлять через `add_knowledge`; это полезная, но не доверенная модельная
  память.
- `BaseDriver.get_obs()` возвращает HTML, screenshot path, URL, timestamp и
  `tab_info`. Screenshot сохраняется с MD5-именем, а для одной страницы есть
  bounded whole-page scan до 30 кадров. Driver abstraction отделяет
  Selenium/Playwright/Chrome Extension от agent logic.
- Default retriever pipeline последовательно применяет interactive XPath
  extraction, расширение XPath nodes и semantic retrieval. Есть BM25,
  syntax/semantic и trivial retriever варианты. Результат ограничивает HTML
  context перед вызовом action model.
- `NavigationEngine` просит модель вернуть YAML actions, извлекает code,
  подсвечивает target до исполнения и разрешает XPath только из текущего
  authorized context. При ошибке делает до `n_attempts` повторов; для каждого
  шага сохраняет raw response, prompt, retrieved HTML, selected target,
  execution result и error.
- В core JSON Schema описывает массив actions и наличие `name/args`, но не
  ограничивает конкретные action names и их аргументы. Chrome Extension
  дополнительно использует Zod discriminated union для `click`, `enter`,
  `setValue`, `setValueAndEnter`, `scroll`, `wait` и `fail`.
- `ActionResult` содержит instruction, generated code, success, output и
  token/cost fields. Важно: `success` в Navigation Engine означает, что
  driver-код выполнился без ошибки, а не что пользовательская цель доказанно
  достигнута.
- `Python Engine` в этом checkout в основном извлекает информацию из HTML и
  пачки screenshots через RAG/OCR с confidence score и fallback; это не
  sandbox для произвольного Python. Сгенерированный browser action code всё
  равно исполняется через driver (`exec_code`/DOM JavaScript).
- QA runner использует YAML fixtures со свойствами `URL`, `Status`, `Output`,
  `Steps`, `HTML`, `Tabs`, глобальными/task-level max steps и локальными
  static-site fixtures. Это хороший источник deterministic acceptance tests.
- Chrome Extension Manifest V3 содержит `host_permissions: ["<all_urls>"]`
  и permissions `activeTab`, `tabs`, `scripting`, `debugger`, `sidePanel`;
  extension валидирует action schema, но работает в реальном браузерном
  профиле пользователя.
- LaVague включает optional Browserbase remote connection, cloud LLM/context
  integrations, Gradio demo и локальный SQLite logger. Эти integrations не
  рассматривались как зависимости EvoHime.

#### Что можем использовать в Еве

- **World Model → typed web action pipeline.** Сопоставить objective,
  observation, planner decision, action dispatch, result и next observation
  с Core-owned workflow Евы. Реализация должна быть в Rust Core, а не в
  Electron renderer и не в Python framework.
- **Web observation/evidence contract.** Взять состав `url + tabs + html
  snapshot + screenshot hash + timestamp + viewport` и дополнить его
  origin, frame id, DOM hash, active tab/window, redaction status и retention.
  Любое действие должно ссылаться на конкретную observation и терять
  валидность после изменения DOM/navigation.
- **Authorized DOM targets.** Идею `authorized_xpaths` перенести как
  allowlist актуальных DOM nodes, но вместо доверия к XPath использовать
  stable target descriptor: DOM/attribute fingerprint, frame/tab id, bounds,
  role/name и короткоживущий observation id. Перед side effect Core обязан
  заново разрешить target и отказать при stale/ambiguous match.
- **Retrieval before action.** Pipeline interactive elements → bounded HTML
  expansion → lexical/semantic retrieval полезен для уменьшения context и
  стоимости. В Еве применить существующий local RAG/context budget, сохраняя
  provenance source nodes и не позволяя retrieved page text менять policy.
- **Typed action schema из Extension.** Небольшой набор `click`, `set_value`,
  `press_enter`, `scroll`, `wait`, `fail` может быть baseline будущего
  browser capability. В Еве добавить action id, target/evidence reference,
  risk class, redacted input, expected effect, timeout, idempotency key и
  approval requirement; схему держать в Rust/proto и генерировать TypeScript.
- **Target preview.** Подсветка выбранного DOM target перед выполнением —
  хороший UX и debugging pattern. Для Евы preview должен показывать URL/tab,
  target summary, изменяемое поле в redacted виде, risk и expected effect,
  после чего approval относится к конкретному action hash.
- **Bounded retries и unknown outcome.** `n_attempts`, error log и
  `ActionResult` полезны как идеи для retry budget, но после возможного
  частичного browser side effect повтор нельзя выполнять автоматически.
  Нужен typed `unknown`, новая observation и отдельное решение policy.
- **Final-state QA fixtures.** Перенять YAML-подобную декларацию тестов с
  initial URL, objective, max steps и проверками URL/HTML/tabs/status. Для Евы
  тесты должны дополнительно проверять отсутствие неразрешённых network
  requests, receipt/approval, cancellation cleanup и redaction.
- **Driver capability matrix.** Сопоставление Selenium/Playwright/extension
  по headless, iframe, tabs и highlight можно превратить в capability matrix
  браузерного backend Евы. Предпочтителен изолированный Playwright context;
  пользовательский Chrome Extension — только отдельный opt-in режим.
- **Token/cost and execution observability.** Поля inference time, token usage,
  retrieved context, generated action и error полезны для Core events и
  budget accounting. Хранить нужно redacted structured receipt, а не raw
  prompt/chain-of-thought и полный HTML по умолчанию.

#### Ограничения и риски

- **Generated code executes browser effects.** LaVague компилирует model
  output в Selenium/Playwright code и вызывает driver `exec_code`. Ошибка
  модели может отправить форму, перейти по внешней ссылке, изменить данные
  или загрузить файл; нет общей Core policy/approval boundary EvoHime.
- **XPath allowlist не является security boundary.** Проверка, что XPath был
  среди retrieved nodes, защищает от части hallucinated targets, но не от
  prompt injection в HTML, вредного элемента в разрешённом context, stale DOM,
  неправильного tab/frame или опасного значения поля.
- **Telemetry включена по умолчанию.** При `LAVAGUE_TELEMETRY` не равном
  `NONE` код отправляет на `telemetrylavague.mithrilsecurity.io` msgpack с
  version, objectives, generated actions, past actions, observations, model
  names, chain-of-thought, bounding box, viewport, current step, token usage,
  URL, result/error и source HTML chunks согласно README. HTML/screenshots
  частично удаляются в helper, но это не равно privacy guarantee Евы.
- **Chrome Extension имеет широкие права.** `<all_urls>`, `scripting`,
  `tabs` и `debugger` дают большую поверхность доступа к браузеру и
  страницам. Это нельзя добавлять в продукт без explicit install consent,
  host allowlist, profile isolation, permission review и отдельного audit.
- **Cloud/remote egress.** OpenAI default context, другие provider packages и
  optional Browserbase передают страницу, prompts или browser session во
  внешние сервисы. Local-first режим и provider egress должны быть явными
  capability decisions, а не побочным эффектом запуска.
- **Ретри и success semantics небезопасны.** До пяти попыток могут повторить
  side effect; `success=True` означает отсутствие exception, а не проверенный
  final state. Нет общего idempotency key, compensation, per-action approval
  или typed partial/unknown outcome.
- **Контекст содержит недоверенный web input.** HTML, text nodes, URLs и
  retrieved source chunks поступают в prompt; страницы могут содержать
  prompt injection, секреты или инструкции для агента. Нужны untrusted-data
  envelope, redaction и строгая граница между page content и Core policy.
- **Runtime и зависимости не соответствуют EvoHime.** Python 3.10+, LlamaIndex,
  LangChain, Selenium/Playwright, Gradio и provider packages дают большую
  surface area; версии root/core разнесены, а checkout имеет alpha classifier.
  Переносить framework целиком в Rust Core нецелесообразно.
- **Лицензия integrations и browser dependencies требует отдельной проверки.**
  Apache-2.0 относится к repository code; Chrome/Playwright/Selenium,
  provider SDK, Browserbase и модели имеют собственные условия. При
  заимствовании кода сохранить attribution/NOTICE и проверить transitive
  licenses.
- **Тестовые сайты не доказывают безопасность.** URL/HTML/status checks
  полезны для regression, но не покрывают data exfiltration, wrong account,
  duplicate submit, popup/navigation race, prompt injection и secret leakage.

#### Предварительное решение

`адаптировать web observation, authorized-target, typed action, retriever и
QA fixture идеи`; `не подключать LaVague core, Chrome Extension, telemetry,
Selenium/Playwright execution loop или Browserbase как runtime-зависимость
Евы`.

#### Связь с EvoHime

- будущий browser capability должен быть отдельным Core-owned tool с default
  deny, capability/host allowlist, isolated browser profile/context,
  bounded navigation/network policy, per-action approval для mutations,
  redacted provenance и supervisor lifecycle; Electron renderer не должен
  обращаться к WebDriver или provider напрямую.
- существующие в EvoHime model gateway, local RAG/context budget,
  prompt-injection envelope, redaction, named-pipe auth, approval/receipts,
  cancellation и Core state ownership остаются источниками истины; LaVague
  не должен добавлять собственные telemetry, secrets или policy layers.
- возможная будущая работа: описать `browser_observation` и
  `browser_action` в desktop IPC/Core contract, затем реализовать isolated
  backend и deterministic local-site fixtures. Этот журнал не создаёт такой
  план.
- критерии проверки: telemetry и raw HTML/screenshot egress выключены по
  умолчанию, provider opt-in явно отражён в policy, target/frame/DOM hash
  revalidated, action schema строго валидируется, side effects требуют
  approval, stale/ambiguous target fail-closed, unknown outcome не повторяет
  submit, cancellation закрывает browser context, а final-state tests
  проверяют URL/DOM/receipt и отсутствие неразрешённых эффектов.

### 8. Microsoft Playwright

- Источник: https://github.com/microsoft/playwright
- Дата проверки: 2026-08-21.
- Ревизия/commit: `9642f57665db582b12dcfa5d8022808f2402fa2a`.
- Последний commit в checkout: 2026-08-21, `feat(trace): attribute requests
  to service workers and api request contexts (#42328)`.
- Версия текущего monorepo package: `1.63.0-next`; требование Node.js `>=20`.
- Лицензия: Apache-2.0; `NOTICE` отдельно указывает код, происходящий от
  Puppeteer под Apache-2.0.
- Состав: `playwright-core`, Node/TypeScript library, Playwright Test,
  Chromium/Firefox/WebKit packages, Electron/WebView/Android/BiDi support,
  trace viewer, recorder, CLI и встроенный MCP/CLI backend для AI agents.
- Назначение: единый API web automation и E2E testing для Chromium, Firefox и
  WebKit; текущий README также описывает CLI/MCP, которые дают LLM structured
  browser control без обязательного vision model.
- Краткий вывод: это наиболее зрелый кандидат из очереди для будущего
  browser backend Евы. Полезно рассмотреть использование ограниченного
  `playwright-core`/isolated helper, но не добавлять Playwright в renderer и
  не считать BrowserContext, MCP allowlist или Chromium sandbox заменой
  Core policy, supervisor isolation и approval.

#### Что изучено

- **BrowserContext isolation.** Каждый context имеет отдельные cookies,
  local/session storage, IndexedDB, cache, history и tabs. CLI по умолчанию
  держит profile в памяти; persistent profile, custom profile и CDP/extension
  attach включаются отдельно. `storageState` позволяет сохранить/восстановить
  cookies, credentials и origin storage, поэтому его нужно считать секретным
  артефактом.
- **Locator/actionability.** User-facing locators (`getByRole`, `getByLabel`,
  `getByPlaceholder`, `getByTestId`) работают поверх accessibility/semantic
  selectors. Перед pointer action engine ждёт visible/enabled/stable, пытается
  прокрутить target в viewport, проверяет hit target и ждёт navigation signals;
  retry-путь обрабатывает stale/not-visible/not-in-viewport состояния. Это
  значительно надёжнее pixel coordinates и произвольных XPath.
- **Structured accessibility agent surface.** Playwright MCP/CLI строит
  snapshot accessibility tree, выдаёт transient element refs вроде `e5` и
  принимает строго описанные tool inputs. В backend есть отдельные
  capabilities (`core`, navigation, tabs, input, network, storage, testing,
  vision, pdf, devtools), Zod schemas, target resolution и verify-tools для
  visible/text/list/value assertions.
- **Network controls.** Context/page routing позволяет abort/continue/fulfill
  requests, мокать API и работать с HAR. Есть proxy, headers, service-worker
  blocking, APIRequestContext и события request/response/failure. Это полезная
  техническая точка интеграции для bounded network egress и deterministic
  fixtures, но allowlist должен принадлежать policy Евы.
- **Evidence and diagnostics.** Trace records actions, DOM snapshots,
  screenshots, network activity, console messages, timing и resource bodies;
  screenshots, video, HAR, console and network events доступны отдельными
  options. Trace Viewer позволяет воспроизвести состояние шага и причину
  failure.
- **Browser lifecycle and data paths.** Есть headless/headed режимы,
  downloads/uploads, file chooser, PDF/screenshot, permissions, geolocation,
  client certificates, storage state, device emulation, proxy и browser
  attach. `acceptDownloads` для обычных browser contexts нормализуется в
  accept, если явно не переопределён.
- **MCP/CLI guardrails.** Текущий MCP умеет `--isolated`, workspace-root
  ограничения для `file://`, `--allow-unrestricted-file-access`, service
  worker blocking, capability selection, output size/timeouts, browser
  profiles и session cleanup. В исходнике прямо указано, что allowed/blocked
  origins не являются security boundary и не действуют на redirects.
- **Cross-platform test engineering.** Monorepo содержит отдельные suites для
  Chromium, Firefox, WebKit, Electron, WebView, Android, BiDi, MCP, extension,
  tracing, network, storage, downloads и test runner. Есть lint/type/doc/dependency
  checks, generated protocol/types и browser-specific expectations.
- Анализ выполнен по README, package metadata, `playwright-core` context,
  locator/actionability, network, trace, MCP/CLI backend, skills/references,
  tests, `LICENSE`, `NOTICE` и `SECURITY.md`. `npm install`, browser download и
  тестовый suite не запускались: это исследовательская запись, не интеграция.

#### Что можем использовать в Еве

- **Кандидат на browser execution backend.** В отличие от LaVague/Agent S,
  Playwright уже содержит зрелый browser lifecycle, locator engine, network
  hooks, download/upload handling, trace и cross-browser tests. Вариант для
  будущего плана: отдельный helper process с Playwright, запускаемый
  supervisor и доступный Core через authenticated bounded IPC. Это не означает
  добавление Node.js runtime в текущий product package.
- **Task-scoped BrowserContext.** Создавать новый non-persistent context на
  задачу, default deny permissions, explicit proxy/network rules, no inherited
  cookies и обязательное close/cleanup. Persistent auth state только после
  явного user action, DPAPI-protected storage и привязки к конкретному
  workspace/provider policy.
- **Semantic target contract.** Использовать role/name/label/test-id и
  transient snapshot ref вместо координат. Action должен содержать
  `context_id`, `page_id`, `snapshot_id`, `target_ref`, role/name fingerprint,
  expected state и action hash; перед исполнением Core повторно разрешает
  target и fail-closed реагирует на stale/ambiguous/multiple match.
- **Actionability как reusable validator.** Проверки visible, enabled, stable,
  in-viewport и hit-target можно превратить в precondition policy для
  `browser.click`, `browser.fill`, `browser.select` и `browser.press`. Они не
  должны автоматически разрешать mutation; validation и approval остаются
  разными стадиями.
- **Accessibility-first context.** Использовать bounded ARIA snapshot и
  targeted search вместо полного HTML или screenshot для обычных web задач.
  Это снижает token cost и privacy exposure. Screenshot/vision включать только
  как explicit fallback, с crop/redaction/provenance.
- **Network policy adapter.** На уровне BrowserContext блокировать всё, что не
  входит в Core-issued origin/route policy, abort third-party trackers и
  неожиданные downloads, фиксировать redirects/requests/responses в receipt и
  иметь deterministic route/HAR fixtures. Redirect targets, DNS/private IP и
  cross-origin requests дополнительно проверяются supervisor/Core, потому что
  Playwright allowlist не является security boundary.
- **Verification and final state.** Использовать locator assertions,
  URL/DOM snapshots, response predicates, download metadata и page state как
  deterministic evidence после каждой mutation. LLM judge может быть только
  advisory; `success` Core должен означать проверенное expected state, а не
  отсутствие exception.
- **Trace as redacted diagnostic receipt.** Trace-on-failure полезен для
  воспроизведения stale target, redirect, popup, console/network race и
  cancellation. В Еве включать ограниченно, перед записью redacting headers,
  cookies, form values, authorization, page text и response bodies; задавать
  retention/size budget и удалять trace после export.
- **Capability and cleanup matrix.** Перенять раздельные capabilities MCP и
  test matrix для browser features. Каждая capability (`navigation`, `input`,
  `network`, `storage`, `files`, `evaluate`, `devtools`) должна быть отдельным
  Core permission с default deny, explicit audit и cleanup contract.
- **Acceptance fixtures.** Из Playwright Test перенять fresh context per test,
  retry only for test infrastructure, web-first assertions, trace on retry,
  parallel browser projects и local deterministic HTTP fixtures. Для Евы
  добавить policy/approval/receipt/egress assertions поверх обычных UI tests.

#### Ограничения и риски

- **BrowserContext не является sandbox.** Он изолирует browser state, но
  страница и automation process всё ещё имеют сеть, downloads, file chooser,
  JavaScript evaluation и доступ к любым явно переданным credentials/paths.
  OS process, Job Object, filesystem/network limits и secret policy должны
  принадлежать supervisor/Core.
- **`evaluate` и file tools опасны.** Playwright API и MCP имеют evaluate,
  upload/download, storage, devtools и CDP attach surfaces. Model-generated
  JavaScript может читать DOM, менять состояние страницы или использовать
  разрешённый origin для exfiltration; абсолютный путь upload и persistent
  profile могут раскрыть локальные данные.
- **Persistent/CDP/extension modes теряют изоляцию.** Attach к уже запущенному
  Chrome, пользовательский profile или extension relay получают доступ к
  реальным вкладкам, cookies и аккаунтам. В Еве эти режимы запрещать по
  умолчанию; разрешать только отдельным capability с подтверждением и
  понятным ownership/cleanup.
- **Network flags не заменяют egress policy.** Сам Playwright предупреждает,
  что `allowed-origins`/`blocked-origins` MCP не являются security boundary и
  не контролируют redirects. Нужны Core-issued allowlist, redirect
  revalidation, DNS/private-range checks, proxy policy и audit каждого
  request/response.
- **Browser sandbox требует явного контроля.** `chromiumSandbox` существует,
  а MCP конфигурирует его по платформе, но его значение зависит от channel,
  launch mode и runtime. Нельзя считать включённый Chromium sandbox заменой
  Windows supervisor Job Object, restricted token и отдельному filesystem
  boundary; unsupported setup должен fail-closed.
- **Sensitive artifacts по умолчанию возможны.** `storageState` может содержать
  cookies/localStorage/IndexedDB, downloads пишутся на диск, а trace/HAR/video
  могут включать passwords, tokens, request headers/bodies, full DOM и
  screenshots. Нужны redaction, encrypted storage, bounded retention и
  no-raw-export default.
- **Actionability не понимает намерение.** Locator может корректно кликнуть
  опасную кнопку, а auto-wait/retry может сделать side effect после изменения
  страницы. Нужны target semantics, action risk classification, approval,
  idempotency/unknown outcome и final-state verification.
- **MCP/CLI имеет широкую AI surface.** Хотя tool schemas и capability
  selection полезны, MCP предоставляет полноценный browser control; default
  network origins широки, `--allow-unrestricted-file-access` расширяет file
  scope, а `--secrets`/storage/profile создают чувствительные inputs. Не
  подключать внешний MCP server прямо к Core без authenticated IPC и policy
  translation.
- **Packaging и supply chain существенны.** Node `>=20`, browser binaries,
  Chromium/Firefox/WebKit channels, native dependencies и frequent releases
  усложняют Windows installer/update. Playwright package не является Rust
  crate; официальный .NET/Python/Java API всё равно требует отдельного
  runtime bridge. Проверить Apache NOTICE, browser binaries, transitive
  licenses и reproducible pinning.
- **Большой test suite не гарантирует продуктовую безопасность.** Upstream
  тестирует API/browser correctness, но не специфические policy EvoHime:
  prompt injection, secret redaction, approval spoofing, tool-call replay,
  supervisor escape, origin policy bypass и durable receipt integrity.

#### Предварительное решение

`рассматривать Playwright как возможный isolated browser backend и источник
locator/network/trace/test контрактов`; `не подключать Playwright MCP/CLI,
CDP attach, persistent profile или browser automation в renderer до отдельного
packaging, sandbox, egress, approval и secret-storage плана`.

#### Связь с EvoHime

- потенциальная интеграция должна идти через supervisor-managed browser host,
  Core-owned typed IPC и capability registry; UI получает только состояние и
  receipts. Внешний Node.js не добавлять в продукт без отдельного решения о
  bundled helper; текущая архитектура Electron не даёт renderer прямой доступ
  к workspace, provider или browser state.
- использовать существующие EvoHime named-pipe session authentication,
  capability/approval policy, context budget, redaction/provenance,
  cancellation/timeout, supervisor Job Object и SQLite event ownership.
  Playwright trace/HAR/storage не должны становиться параллельным источником
  истины или обходить Core audit.
- возможная будущая работа: сначала design-only `browser_observation`,
  `browser_action`, `browser_receipt` и browser-host protocol; затем tiny
  isolated Chromium fixture на локальном test server, network deny tests,
  storage/download cleanup и real cancellation/restart tests. Этот журнал не
  создаёт implementation plan.
- критерии проверки: non-persistent context по умолчанию, sandbox/Job Object
  health check, default-deny permissions and origins, redirect/DNS/private-IP
  policy, no direct CDP/user-profile attach, no raw trace/storage export,
  strict action schema, snapshot-bound target revalidation, per-mutation
  approval, typed unknown after partial side effect, deterministic final-state
  assertions, bounded artifacts и cleanup после crash/cancel.

### 9. Puppeteer

- Источник: [официальная документация](https://pptr.dev/) и репозиторий
  [puppeteer/puppeteer](https://github.com/puppeteer/puppeteer)
- Дата проверки: 2026-08-21
- Ревизия/commit: `186beb8ae8091d7cce1c23ab6d2fba109c169988`
- Версия документации: `25.8.0`
- Лицензия исходного кода: Apache-2.0; отдельные browser binaries и
  системный Chrome имеют собственные условия распространения.
- Состав: TypeScript/Node.js monorepo с `puppeteer`, тонким
  `puppeteer-core`, `@puppeteer/browsers`, документацией и большой test suite.
- Назначение: управление Chrome/Chromium и Firefox через Chrome DevTools
  Protocol или WebDriver BiDi; по умолчанию браузер запускается headless.
- Краткий вывод: сильный Chromium/CDP-first кандидат для будущего browser
  host Евы. Для EvoHime интереснее `puppeteer-core`, потому что он не скачивает
  браузер автоматически; браузер, helper process, sandbox и lifecycle должны
  оставаться под контролем supervisor/Core.

#### Что изучено

- `BrowserContext` изолирует cookies, localStorage, cache и связанные страницы;
  контекст можно создавать и закрывать на задачу, выдавать разрешения явно и
  не использовать профиль пользователя. Это полезный runtime-механизм, но не
  замена OS sandbox или EvoHime Job Object.
- Locator API и ARIA-селекторы (`::-p-aria(...)`) дают семантическое управление
  с ожиданием состояния, вместо координат мыши. В будущий action contract нужно
  передавать evidence из наблюдения: page/frame/context, роль, имя, ref и
  ожидаемое состояние; locator сам не понимает риск бизнес-действия.
- `puppeteer-core` подключается к явно выбранному браузеру, CDP session и
  remote endpoint; `@puppeteer/browsers` умеет install/list/clear/launch и
  вычислять executable path. Это позволяет отдельно закрепить версию browser
  binary и реализовать health check, rollback и очистку.
- Network interception, request/response events, downloads, screenshots, PDF,
  upload/file chooser, permissions, tracing и CDP дают хорошие точки для
  наблюдаемости и policy adapter. Сама библиотека не является egress policy,
  secret store или журналом действий.
- Полный пакет `puppeteer` скачивает совместимый Chrome при установке;
  `puppeteer-core` этого не делает. Заблокированные package-manager scripts
  требуют отдельной установки browser binary, поэтому supply chain и installer
  должны быть частью решения.
- Репозиторий содержит отдельные browser/API/type tests, проверки документации,
  lint, лицензий и зависимостей; в рамках исследования upstream-сборка и тесты
  не запускались. В документации также упоминается отдельный
  `chrome-devtools-mcp` на базе Puppeteer и экспериментальный WebMCP; это
  исследовательские ориентиры, а не готовая зависимость EvoHime.

#### Что можем использовать в Еве

- Паттерн `puppeteer-core` как тонкого клиента без install-time скачивания;
  только внутри отдельного supervisor-managed browser helper, не в renderer и
  не как прямой runtime Core.
- Одноразовый non-persistent `BrowserContext` на задачу, явное закрытие всех
  страниц и default-deny permissions. Нельзя разрешать attach к реальному
  пользовательскому профилю или принимать произвольный CDP endpoint.
- Locator/ARIA-контракт для `browser_observation` и `browser_action` с
  повторной проверкой target после каждого navigation/DOM change, risk class,
  approval для мутаций и typed `unknown` после частично выполненного действия.
- Browser manager как источник идей для pinned browser build, executable path,
  запуска, проверки версии, восстановления после падения и очистки временного
  профиля.
- CDP session и bounded tracing для диагностики; наружу отдавать только
  редактированные receipts/telemetry, а не raw DOM, cookies, headers, bodies,
  trace, screenshots или storage state.
- Request interception как один слой allowlist/redirect/request-body policy;
  фактические DNS/private-IP, filesystem, proxy и egress-ограничения должны
  проверяться Core/supervisor независимо от Puppeteer.
- Upstream locator, browser-lifecycle, cancellation и deterministic local
  server fixtures можно использовать как образец для собственных contract и
  integration tests без копирования бизнес-логики.

#### Ограничения и риски

- Puppeteer прямо оставляет ответственность за безопасное применение на
  вызывающей стороне. `page.evaluate`, function-based actions и CDP могут
  выполнять произвольную логику страницы; BrowserContext не является
  sandbox-границей операционной системы.
- `--no-sandbox` встречается в troubleshooting/launch-сценариях, но такой путь
  неприемлем как дефолт Евы. Нужны проверка sandbox, restricted helper,
  supervisor Job Object и fail-closed при неподдержанном окружении.
- Downloads и screenshots пишут на диск, upload принимает локальные пути,
  extensions могут загружаться, а storage/trace/network artifacts могут
  содержать cookies, токены и секретные request data. Нужны redaction,
  encrypted storage, bounded retention и запрет raw export.
- Interception и `allowed origins` не дают полной сетевой границы: остаются
  redirects, DNS rebinding/private ranges, service workers, extensions и
  remote browser attach. Нужна собственная Core-issued policy с audit каждого
  запроса и повторной проверкой redirect.
- Нет встроенных EvoHime approval, capability registry, durable receipts,
  prompt-injection defence или модели частичного исхода. Auto-wait повышает
  надёжность UI-синхронизации, но не делает опасную кнопку безопасной.
- Node.js helper и browser binaries конфликтуют с текущим правилом продукта,
  что внешний Node.js runtime не входит в поставку. Нужны отдельное решение о
  bundled helper, Windows packaging, размерe/update/rollback и поддержке
  Chromium/Firefox feature differences.
- Apache-2.0 допускает использование при сохранении условий лицензии и
  attribution; лицензии Chrome/Chromium и прочих binary dependencies требуется
  проверять отдельно.

#### Предварительное решение

`рассматривать puppeteer-core + supervisor-managed pinned browser как
альтернативу Playwright для Chromium-first backend`; `не подключать Puppeteer,
Chrome DevTools MCP, WebMCP, user-profile attach или no-sandbox path до
отдельного packaging/security плана`. Не добавлять одновременно Playwright и
Puppeteer как runtime-зависимости без сравнительного PoC и решения о границах
поддержки.

#### Связь с EvoHime

- Потенциальный browser host должен быть отдельным процессом под supervisor;
  Core владеет capability/approval, network policy, redaction/provenance,
  cancellation/timeout и SQLite audit, а Electron получает только typed state и
  receipts. Puppeteer не должен обходить authenticated desktop IPC.
- Сначала нужен design-only контракт `browser_observation`, `browser_action`,
  `browser_receipt` и lifecycle/error model; затем минимальный Chromium fixture
  на локальном test server. В этот журнал implementation plan не добавляется.
- Критерии будущей проверки: pinned binary и health check, отсутствие
  user-profile/CDP attach, sandbox/Job Object enforcement, context cleanup после
  crash/cancel, default-deny permissions, redirect/DNS/private-IP tests,
  semantic target revalidation, per-mutation approval, bounded/redacted
  artifacts и deterministic final-state assertions.

### 10. Ultravox

- Источник: [репозиторий fixie-ai/ultravox](https://github.com/fixie-ai/ultravox)
  и связанные в нём [Ultravox Realtime](https://ultravox.ai) / модели
  [Hugging Face](https://huggingface.co/fixie-ai)
- Дата проверки: 2026-08-21
- Ревизия/commit: `69ddc63b2d72be5e9a86f818315da72cec55a876`
  (`2025-12-12`, README обновлён для Ultravox 0.7)
- Лицензия исходного кода: MIT, Copyright Fixie.ai 2023. Лицензии и условия
  базовых LLM, audio encoder и опубликованных model weights нужно проверять
  отдельно.
- Состав: Python 3.10+ проект на PyTorch/Transformers с Whisper-подобным audio
  encoder, multimodal projector, open-weight text LLM, локальным inference,
  training, dataset tooling и evaluation.
- Назначение: multimodal LLM для real-time voice research. Модель принимает
  аудио и текст и выдаёт streaming text без отдельного ASR-этапа; полноценный
  voice-to-voice продукт и managed realtime API находятся вне этого checkout.
- Краткий вывод: полезный источник архитектурных идей для voice provider Евы,
  потокового ответа, формата голосового turn, диалогового KV-cache и
  воспроизводимых audio evals. Это не готовый компонент поставки EvoHime:
  default-модель на Llama 3.3 70B, Python/PyTorch/CUDA-зависимости, скачивание
  весов и GPU-профиль требуют отдельного provider/deployment решения.

#### Что изучено

- `UltravoxModel` объединяет audio tower и текстовую модель, а
  `UltravoxProjector` переводит признаки аудио в пространство hidden states
  LLM. Конфигурация позволяет менять audio/text backbone, projector, LoRA и
  latency block size. Это модельная архитектура, а не готовая политика
  инструментов или состояние продукта.
- `UltravoxProcessor` нормализует аудио, использует placeholder `<|audio|>`,
  считает длину audio token span и вставляет replacement tokens в текстовую
  последовательность. Длинные записи режутся на chunks по audio context size;
  тесты проверяют короткие, длинные, многократные и переполняющие контекст
  аудио.
- `LocalInference` имеет обычный, batch и streaming пути. Streaming строит
  KV-cache для входа, затем использует `transformers.TextIteratorStreamer` в
  отдельном thread и отдаёт `InferenceChunk`/`InferenceStats`. Это хороший
  образец API наблюдаемого потока, но в нём нет полноценного cancellation,
  backpressure, аудиовывода или гарантии прерывания native generation.
- `conversation_mode` сохраняет `past_messages` и `past_key_values`, а после
  ответа заменяет audio placeholder на EOS-span и добавляет assistant message.
  Паттерн полезен для latency, но cache должен принадлежать Core-сессии,
  иметь лимит бюджета и удаляться при завершении/отмене.
- `VoiceSample` описывает messages, float32 PCM, sample rate, transcript,
  label и extra evaluation fields; поддерживаются raw buffer, WAV file и
  JSON/base64 WAV. Для EvoHime полезен typed voice-turn envelope, но base64 и
  произвольные пути не должны быть внутренним IPC-форматом по умолчанию.
- Dataset configs задают split, audio field, prompt templates, message history,
  labels и eval metric. `ds_tool` умеет ASR/TTS/text generation, chunked dataset
  transforms, timestamping, mixing, augmentation и кэширование. Registry и
  composable configs — хороший образец для локального набора voice fixtures.
- Evaluation pipeline поддерживает WER/BLEU и instruction/voicebench-подобные
  метрики, ограничение sample count, DDP sharding и отдельный прогон
  audio augmentations. В проекте есть pytest-тесты для preprocessing,
  inference, model config, streaming, datasets и metrics.
- README описывает обучение projector при замороженных backbone, адаптацию к
  новым LLM/audio encoder и RAG-on-the-fly вместо обязательного fine-tuning.
  Указанный масштаб старого training run — 8xH100; это показывает стоимость
  обучения, а не обязательный минимум для каждого inference checkpoint.

#### Что можем использовать в Еве

- Идею отдельного voice provider contract: `audio_input`/`text_context` →
  `text_delta` → `voice_turn_completed`, с `input_tokens`, `output_tokens`,
  latency и terminal reason. В Core это должно быть typed IPC и единый audit,
  а не прямой Python callback в Electron.
- Audio preprocessing contract: явный sample rate, PCM dtype, channel count,
  duration limit, resampling policy, chunk sequence и stable turn id. Ошибки
  формата и превышения длительности должны возвращаться до model invocation.
- Placeholder/span-модель для привязки аудиофрагмента к сообщению и
  conversation turn. Для Евы это может стать частью transcript/provenance:
  какой audio chunk породил наблюдение или ответ, без хранения сырого аудио в
  каждом event.
- Потоковую семантику `chunk`/`stats`/`completed` и раздельное сохранение
  assistant text от внутреннего thinking/debug content. Нужны bounded queues,
  cancellation token и backpressure поверх этой идеи.
- Conversation cache как оптимизацию: Core владеет session state, KV-cache
  привязан к provider/model revision и очищается при смене модели, workspace,
  identity или policy. При cache miss должен существовать детерминированный
  replay из сохранённых typed messages.
- Набор локальных voice fixtures и evaluation registry: 16 kHz/48 kHz,
  тишина, шум, длинный input, clipping, multi-turn, text-only fallback,
  cancellation и partial output. Метрики WER/latency/first-token и
  deterministic final transcript можно включить в provider conformance suite.
- Audio augmentation как тестовый слой, а не runtime-состояние: gain, noise,
  reverb, resampling и compression помогают проверять устойчивость listener и
  provider adapter без изменения исходных пользовательских записей.
- Composable prompt/data configs и explicit labels/transcripts как образец
  разделения test fixture, expected outcome и production conversation. Идеи
  можно реализовать на Rust/SQLite/JSONL без импорта Python pipeline.
- RAG-on-the-fly как архитектурную подсказку: улучшение голосового ответа
  знаниями не требует менять multimodal projector; retrieval должен идти через
  существующий Core-owned workspace RAG и context budget.

#### Ограничения и риски

- Checkout ориентирован на обучение и локальный inference, а не на Windows
  desktop delivery. Python, PyTorch >=2.6, Transformers, librosa, CUDA/GPU,
  Hugging Face/W&B и многочисленные evaluation dependencies не соответствуют
  правилу EvoHime об отсутствии внешнего Python/Node runtime в продукте.
- Модель не является заменой текущему listener/ASR runtime без доказанного
  latency, memory, multilingual и hardware профиля. Прямое audio-to-LLM
  coupling может убрать отдельный ASR этап, но усложняет транскрипт,
  searchable provenance, partial recognition и отладку ошибок.
- Default 70B backbone и скачивание model weights создают большую стоимость,
  supply-chain риск, требования к диску/VRAM и отдельные условия лицензий.
  Нельзя включать загрузку Hugging Face или W&B в пользовательский runtime без
  явного provider policy, pinning, checksum и offline/failure behavior.
- `VoiceSample.to_json()` кодирует аудио в base64 WAV, а dataset tools умеют
  загружать/публиковать данные и обращаться к внешним ASR/TTS/LLM сервисам.
  Сырые записи, transcript, W&B/eval logs и dataset cache могут содержать
  персональные данные; для Евы нужны consent, redaction, retention и
  owner-only storage.
- Streaming сделан через Python thread и `TextIteratorStreamer`; upstream API
  не задаёт EvoHime cancellation, timeout, crash recovery, queue bounds или
  exactly-once receipt. Нельзя переносить эту реализацию в Core без явного
  lifecycle/error contract.
- KV-cache может быть чувствительным производным состоянием и занимать много
  памяти. Нужны session quota, eviction, model revision binding, telemetry без
  сырого содержимого и очистка после аварийного завершения.
- Тренировочный dataset tool документирует append-only, неатомный локальный
  metadata cache; это нельзя принимать как модель долговременного состояния
  EvoHime. SQLite migrations, backups и audit остаются ответственностью Core.
- MIT-лицензия репозитория не распространяется автоматически на Llama/Mistral/
  Gemma, Whisper, Hugging Face datasets и сервисы Deepgram/ElevenLabs/W&B.
  Для любого runtime-включения потребуются отдельные license/attribution и
  privacy проверки.

#### Предварительное решение

`адаптировать идеи и тестовые контракты; наблюдать за модельными checkpoint`
и не подключать Ultravox Python runtime, training stack, внешние realtime API,
Hugging Face/W&B upload или 70B weights в desktop-поставку без отдельного
provider/GPU/privacy плана. Приоритетнее использовать существующий Core-owned
listener + model gateway, а Ultravox рассматривать как сравнительный voice
provider и источник eval fixtures.

#### Связь с EvoHime

- Возможная интеграция должна быть provider adapter за Core-owned model gateway:
  Electron получает только transcript/stream state, Core владеет audio buffer,
  context budget, approval, identity, provenance, SQLite events и cancellation.
  Ultravox не должен получать workspace или provider secrets напрямую.
- Сначала нужен design-only voice provider contract и conformance suite;
  затем локальная deterministic audio fixture, bounded streaming test и
  comparison с текущим listener. Этот журнал implementation plan не создаёт.
- Критерии будущей проверки: 16/48 kHz normalization, duration/chunk limits,
  first-token and end-to-end latency, cancellation during generation, bounded
  memory/cache, text-only fallback, transcript provenance, redacted events,
  offline behavior, model checksum/license manifest и cleanup после crash.

### 11. Moshi

- Источник: [репозиторий kyutai-labs/moshi](https://github.com/kyutai-labs/moshi),
  [протокол Rust backend](https://github.com/kyutai-labs/moshi/blob/main/rust/protocol.md)
  и paper [Moshi: a speech-text foundation model for real-time dialogue](https://arxiv.org/abs/2410.00037)
- Дата проверки: 2026-08-21
- Ревизия/commit: `e6a55d2722a65870ef52a6c9f6ecfc0e90f38362`
  (`2026-05-16`)
- Лицензии: Python и web client — MIT, Rust backend — Apache-2.0; в репозитории
  есть отдельные `LICENSE-MIT`, `LICENSE-APACHE`, `moshi/LICENSE` и
  `client/LICENSE`. Model weights выпущены под CC-BY 4.0; сторонние AudioCraft,
  Mimi и используемые модели/данные требуют отдельной проверки условий.
- Состав: три inference backend — PyTorch для research, MLX для on-device
  inference на macOS/iPhone и Rust/Candle для production; Rust содержит
  `moshi-core`, Mimi implementation, server/backend и CLI, а `client` — web UI
  и binary WebSocket client.
- Назначение: full-duplex spoken dialogue. Moshi моделирует поток пользователя и
  поток собственной речи, а также текстовые токены; Mimi потоково сжимает
  24 kHz audio до 12.5 Hz representation с заявленной задержкой frame 80 ms.
- Краткий вывод: это самый близкий из изученных источников к будущему
  voice-runtime Евы благодаря Rust backend, streaming state, отдельному codec
  layer и явному typed/binary protocol. Использовать как источник контракта и
  сравнительный PoC; не встраивать сейчас: нет официальной Windows-поддержки,
  PyTorch требует GPU около 24 GB без quantization, а production path всё ещё
  требует самостоятельной security/packaging интеграции.

#### Что изучено

- Архитектура делит систему на `Mimi` streaming neural audio codec, multistream
  temporal/depth transformer и server/client transport. Модель работает на
  нескольких audio codebooks одновременно, выдавая audio tokens для собственной
  речи и text token stream; full-duplex не сводится к последовательному
  `ASR -> LLM -> TTS` pipeline.
- Mimi имеет frame size 1920 samples при 24 kHz и frame rate 12.5 Hz. В
  streaming mode вход обязан поступать положительными блоками, кратными frame
  size; код явно предупреждает о необходимости buffer/pad и о фиксированном
  размере входа для CUDA Graphs. Это сильный контракт для bounded audio ring
  buffer.
- `moshi/modules/streaming.py` задаёт context-managed streaming state,
  reset, snapshot/set state и execution mask для рассинхронизированных batch
  streams. В Rust есть аналогичные streaming state, KV cache, reset и
  multistream generation.
- Rust backend реализует production-oriented `moshi-core`/Candle model path,
  отдельный Mimi path и `moshi-backend`. Поддерживаются CUDA/Metal features,
  quantized q8 config и standalone server; Python bindings `rustymimi` дают
  возможность использовать Rust Mimi из Python.
- Binary WebSocket protocol использует one-byte message type и little-endian
  payload: handshake, Opus/Ogg audio, UTF-8 text, control, JSON metadata, error
  и ping. Control содержит Start, EndTurn, Pause и Restart. Неизвестные типы
  должны отбрасываться, а version/model fields позволяют версионировать
  protocol.
- Input audio идёт как Opus 24 kHz mono внутри Ogg pages; output PCM
  буферизуется и кодируется в Opus frames. Web client записывает microphone с
  echo-cancellation path, а Rust server декодирует вход в bounded chunks перед
  Mimi/model loop.
- Session configuration разделяет text/audio temperature, top-k, seed,
  max_steps, repetition penalty и optional ASR delay. Это образец отделения
  deterministic session parameters от global model config; seeds полезны для
  воспроизводимых voice fixtures.
- Сервер отсылает handshake/metadata до потока, разделяет websocket receive,
  decode, model generation и async send loops, имеет backpressure-sensitive
  channels и закрывает сессию по timeout/inactivity. Web client дополнительно
  закрывает socket после 10 секунд отсутствия сообщений.
- Для WebSocket auth backend принимает authorized ID из header или query
  parameter, потому что браузеру неудобно задавать custom headers. Есть
  `room_id` для Mimi send/receive channels. Это пример edge transport auth, но
  не замена authenticated EvoHime named pipe.
- После сессии Rust backend записывает JSON summary и safetensors с text/audio
  token trajectories, включая session config, transcript, client address,
  model file paths и build information. Это полезно для offline debugging, но
  требует жёсткой redaction/retention политики в Еве.
- Тесты и benchmark покрывают streaming modules, SEANet/codec, model generation
  и Mimi streaming; README отдельно требует `mimi_streaming_test` и GPU
  benchmark. В рамках исследования зависимости, сборка Cargo и runtime-тесты
  не запускались.

#### Что можем использовать в Еве

- Архитектурный шаблон отдельного `voice-host`: Rust Core владеет сессией,
  streaming state и policy, а transport adapter обменивается bounded binary
  audio/text events. Это хорошо соответствует Core-first архитектуре Евы, но
  transport должен идти через authenticated desktop IPC, а не через открытый
  WebSocket server.
- Протокол framing/versioning: `protocol_major`, `model_revision`, typed
  `audio_frame`, `text_delta`, `control`, `metadata`, `error`, `ping` и
  terminal event. В EvoHime стоит добавить sequence/correlation/session IDs,
  payload limits, replay rules и explicit unknown/partial outcome.
- Explicit control state machine `start -> streaming -> end_turn/pause/restart`
  с отказом от невалидных переходов. `cancel`, `disconnect`, `model_error` и
  `permission_revoked` должны быть отдельными Core events, а не молчаливым
  закрытием socket.
- Audio ring buffer с обязательным frame alignment, bounded queue, flush/pad
  на завершении и измерением `input_pcm_duration`, `encoded_frames`,
  `dropped_frames`, `decode_lag` и `first_output_latency`.
- Разделение codec/model/transport слоёв: Mimi-like codec может быть заменён
  текущим listener/encoder, provider model может быть удалённым, а IPC
  transport — локальным. Не связывать аудиоформат, модель и UI state в одном
  модуле.
- Streaming state API с `reset`, explicit session scope, bounded cache и
  optional snapshot only for controlled recovery. Не сохранять внутренний
  audio/token state в Electron и не делать его вторым источником истины рядом
  с SQLite.
- Session sampling config и seed как часть auditable provider invocation;
  значения должны проходить allowlist/limits, логироваться в redacted form и
  не позволять модели или пользовательскому prompt менять policy-поля.
- Typed metadata для `model_revision`, `backend`, `build_id`, `codec_revision`
  и latency capabilities, но без раскрытия абсолютных путей, client address,
  секретов и внутренних filesystem details.
- Полезно перенять testing approach: exact frame-size tests, encode/decode
  round-trip, stream reset, desynchronized batch mask, bounded queue,
  reconnect/replay, cancellation и final transcript/audio determinism.
- Rust/Candle backend можно рассматривать как сравнительный PoC для локального
  voice provider, если hardware и Windows support будут подтверждены. Даже без
  включения Moshi его codec framing и state contracts могут быть реализованы
  самостоятельно в существующем Rust Core.

#### Ограничения и риски

- README прямо указывает отсутствие официальной поддержки Windows. PyTorch
  backend не поддерживает quantization и требует GPU с существенной памятью
  (около 24 GB); Rust CUDA path требует корректные CUDA/nvcc/toolchain.
  Нельзя обещать работу на целевых Windows машинах без отдельного hardware/CI
  PoC.
- WebSocket auth ID в query может попадать в URL, proxy/access log, browser
  history и telemetry. Для EvoHime нужен HMAC-authenticated named pipe уже
  существующего IPC, одноразовая session binding и запрет передачи секретов в
  query string.
- Binary protocol из репозитория не задаёт полноценную security boundary:
  payload limits, origin policy, replay protection, per-session authorization,
  backpressure fairness и encrypted transport должны быть добавлены отдельно.
- Сервер сохраняет transcript, client address, model paths и raw text/audio
  token trajectories в JSON/safetensors. Эти артефакты могут восстановить
  разговор или содержать чувствительные данные; в Еве нужен default-deny
  recording, redaction, owner-only ACL, retention/erase и отсутствие raw export.
- `inner monologue` является частью модельного потока и не должен попадать в
  пользовательский UI, durable transcript, model context или telemetry без
  явного redaction. Нужна граница между visible speech text, hidden reasoning
  и diagnostic tokens.
- Full-duplex sampling генерирует audio и text одновременно. Это усложняет
  interruption/barge-in, approval перед внешним действием, idempotency,
  partial speech playback и reconciliation уже проигранного аудио с новым
  Core state.
- Client CLI намеренно barebone: без echo cancellation и компенсации
  растущего lag; web UI выполняет дополнительную обработку. Это не готовая
  microphone UX-гарантия и не заменяет listener permission/quality checks Евы.
- Backend использует fixed max steps/timeout и отдельные threads/channels;
  комментарии и cleanup-path требуют проверки на реальном disconnect/crash.
  Нельзя считать закрытие WebSocket доказательством остановки GPU/native work.
- Rust backend и Python/web части имеют разные лицензии, а weights — CC-BY 4.0;
  model checkpoints, tokenizer, Opus/Ogg, Candle и данные нужно включить в
  license manifest и installer attribution. MIT корневого проекта не покрывает
  всё содержимое поставки.
- Public demo/tunnel может добавлять сотни миллисекунд задержки и переносить
  microphone/audio через внешний маршрут. Для production Евы допустим только
  локальный или явно разрешённый provider endpoint с audit egress.

#### Предварительное решение

`рассматривать Moshi как наиболее близкий сравнительный voice-runtime и
источник Rust/codec/streaming контрактов`; `адаптировать protocol, state,
buffering и evaluation идеи`. Не подключать upstream сервер, public tunnel,
query auth, raw token logging или модельные weights в desktop-поставку до
отдельного Windows/GPU, security, privacy и licensing PoC. Не заменять им
автоматически текущий listener: сначала сравнить full-duplex latency, barge-in,
transcript quality, memory и recovery.

#### Связь с EvoHime

- Возможный host должен запускаться supervisor и быть виден Core как provider,
  используя authenticated desktop-ipc-v1; Electron получает только
  transcript/audio playback state. Workspace, secrets, approvals, identity,
  SQLite audit, cancellation и redaction остаются у Core.
- Сначала нужен design-only `voice_session`/`voice_frame`/`voice_event`
  контракт и deterministic local test fixture; затем Rust/Candle/Mimi PoC с
  локальными weights и без внешнего tunnel. Этот журнал implementation plan не
  создаёт.
- Критерии будущей проверки: frame alignment/flush, bounded queue and memory,
  protocol version mismatch, HMAC auth, replay/sequence rejection, barge-in,
  cancel/disconnect stops work, hidden-output redaction, no raw trajectory
  persistence, model checksum/license manifest, Windows CUDA/CPU fallback и
  supervisor crash/restart cleanup.

### 12. Pipecat

- Источник: [репозиторий pipecat-ai/pipecat](https://github.com/pipecat-ai/pipecat),
  [официальная документация](https://docs.pipecat.ai/)
- Дата проверки: 2026-08-21
- Ревизия/commit: `bd5a2f4cd4da4e7a2c8a7b4051c82de747e42d20`
  (`2026-08-21`, `Make a TTS service that silently returns no audio get marked unusable`)
- Лицензия исходного кода: BSD-2-Clause; copyright Daily, 2024–2026.
- Состав: Python framework для realtime voice agents и multimodal apps,
  `src/pipecat`, transport/provider adapters, examples, behavioral evals и
  клиентские SDK/интеграции. Репозиторий активно развивается; на момент
  проверки история содержала около 11 900 commits.
- Назначение: orchestration-слой между transport, audio/video, STT, LLM, TTS,
  tools и UI. Pipecat не является одной моделью или готовым security boundary:
  он организует поток событий, lifecycle и подключение провайдеров.
- Краткий вывод: это один из наиболее полезных источников для будущего
  voice/multimodal слоя Евы на уровне контрактов и тестов. Переносить весь
  Python/asyncio runtime в desktop не нужно; подходящие идеи следует
  выразить в Rust Core и authenticated desktop IPC.

#### Что изучено

- **Frames и processors.** Основная композиция выглядит как цепочка
  `Processor1 -> Processor2 -> ... -> ProcessorN`. Frame — typed data или
  control unit для audio, text, video, lifecycle, interruption, metrics и
  function calls. `FrameProcessor` принимает, обрабатывает и передаёт frames
  downstream или upstream; transports тоже являются processors.
- **Приоритеты и прерывания.** Внутри processor есть отдельный путь для
  system frames и очередь обычных data frames. System messages имеют высокий
  приоритет, поэтому cancellation/end/interruption не должны ждать длинную
  аудио- или текстовую очередь. `InterruptionFrame` распространяется в обе
  стороны и сбрасывает текущую обработку; `EndFrame`/`StopFrame` относятся к
  uninterruptible terminal frames и переживают interruption.
- **Pipeline lifecycle.** `PipelineWorker` отправляет стартовые frames,
  контролирует setup/start/pipeline/cancel timeouts, heartbeat и idle timeout,
  отслеживает ошибки и гарантирует cleanup observers/tasks. Для непригодного
  processor есть политика `CONTINUE`, `END` или `CANCEL`, а runner умеет
  завершать короткие jobs или держать long-lived host.
- **Workers и bus.** Worker — верхнеуровневый runnable unit с activation,
  ready/end/cancel, job RPC и task cleanup. Typed `WorkerBus` разделяет
  normal data и high-priority system messages, поддерживает lifecycle,
  registry, job request/response/update/cancel и streamed job updates.
  Есть локальная очередь и сетевые варианты на PGMQ/Redis, а `BusBridgeProcessor`
  соединяет bus с pipeline.
- **Tools и function calls.** `@tool` собирает tool functions на LLM worker,
  задаёт `cancel_on_interruption` и timeout. Function-call lifecycle представлен
  отдельными start/in-progress/result/cancel frames, а не одним непрозрачным
  вызовом.
- **Provider и transport abstraction.** Базовые STT/TTS/LLM services скрывают
  различия провайдеров и streaming semantics; отдельные transports обслуживают
  Daily/WebRTC/WebSocket/LiveKit/SmallWebRTC/локальные сценарии. Serializers
  преобразуют frames в wire format, включая protobuf и telephony formats.
- **RTVI.** Processor/observer формируют typed client protocol для lifecycle,
  transcription, bot speaking, VAD, interruption, metrics, audio levels,
  function calls и UI commands/snapshots. В RTVI отдельно задаётся уровень
  раскрытия function-call данных; default observer configuration не должна
  отдавать внутренние аргументы всем клиентам.
- **Observers и metrics.** Наблюдатели читают поток без изменения поведения.
  Есть TTFB/TTFA и turn-level breakdown, usage/latency metrics, heartbeat и
  интеграции OpenTelemetry/Sentry. Это позволяет диагностировать realtime
  pipeline без помещения диагностической логики в каждый provider.
- **Behavioral evals.** YAML-сценарии задают turns с text/audio/DTMF/image и
  ожиданиями `within_ms`, `text_contains`, отсутствием события, function call
  с именем/аргументами и LLM judge. Harness запускает реального бота через
  eval transport, проверяет порядок turns и агрегирует latency; suites могут
  выполняться параллельно.
- **Инженерные гарантии.** Тесты используют `run_test()` для отправки frames и
  assertions, отдельный task manager для отмены/cleanup и typed Pydantic
  models для внешних config/metrics; высокочастотные внутренние frames
  представлены dataclass-подобными структурами.

#### Что можем использовать в Еве

- **Frame/event taxonomy для voice layer.** Спроектировать в Core отдельные
  типы вроде `voice_audio_frame`, `voice_text_delta`, `voice_control`,
  `voice_interruption`, `voice_tool_call`, `voice_metric` и
  `voice_receipt`. Направление upstream/downstream и correlation/session IDs
  должны быть частью контракта, а не соглашением между UI и provider.
- **Срочные control messages.** Перенять разделение system/data: cancel,
  permission-revoked, interrupt, disconnect и terminal events должны иметь
  приоритет над queued audio/text data. Это хорошо сочетается с уже имеющимися
  у EvoHime bounded IPC frames, sequence replay и Core-owned cancellation.
- **Uninterruptible terminal semantics.** Зафиксировать, какие завершения
  нельзя потерять при barge-in: финальный receipt, отказ policy, durable audit
  commit и controlled session end. При этом произнесённый пользователю звук
  не должен автоматически считаться успешным выполнением внешнего действия.
- **Worker lifecycle для provider/child host.** Использовать состояния
  `created -> starting -> ready -> running -> draining -> ended` плюс
  `cancelled`, `failed` и `restarting`. Добавить setup/start/idle/heartbeat/
  cancel deadlines, bounded cleanup и явный результат crash recovery.
  Это может стать общим контрактом Core для listener, voice provider и child
  agent, но supervisor остаётся владельцем процесса и Job Object.
- **Typed job model.** Для будущих child agents и долгих voice operations
  полезны `job_id`, `worker_id`, `request`, `progress`, `result`, `error`,
  `cancel` и streamed updates. Job groups/fan-out можно рассматривать позже,
  не расширяя сейчас desktop IPC без конкретного потребителя.
- **Tool invocation metadata.** У каждого function call должны быть timeout,
  interruption policy, capability/risk class, approval requirement,
  idempotency key и unknown-outcome handling. Pipecat даёт хороший минимум
  cancellation/timeout, но EvoHime обязан добавить свою approval и receipt
  model перед side effect.
- **Transport/serializer boundary.** Сохранить независимость voice pipeline
  от audio codec, provider API и desktop IPC. В wire protocol разрешать только
  versioned serde/protobuf payloads из allow-list; произвольные Python objects,
  callbacks или `Any` из Pipecat нельзя выставлять наружу.
- **RTVI-подобный typed UI protocol.** Использовать идею отдельного слоя
  наблюдаемых событий: bot/user speaking, partial/final transcript, VAD,
  function-call lifecycle, metrics и a11y/UI snapshot. Electron получает
  redacted state через Core, а не сырые provider frames и не внутренний
  reasoning/context.
- **Observer-first observability.** Добавить read-only observers для TTFB,
  TTFA, turn duration, queue depth, dropped audio, provider retries,
  cancellation latency и final receipt. Сырые audio/text payloads должны быть
  исключены из telemetry по умолчанию; события связываются с provenance и
  correlation IDs.
- **Behavioral eval suite.** Перенять сценарии `turn -> expectations`:
  bounded first-token/audio latency, transcript fragments, interruption,
  function-call name/args policy, cancellation, absent forbidden events и
  redaction. Запускать на deterministic local fixtures и fake provider, а
  реальные внешние API использовать только в явно ограниченном smoke suite.
- **Provider conformance matrix.** Базовые STT/TTS/LLM contracts и capability
  metadata можно использовать для сравнения listener, удалённого realtime
  provider и будущего Moshi-подобного host. Это не означает добавление всех
  десятков интеграций Pipecat: для Евы нужна небольшая матрица с offline,
  streaming, interruption, cancellation, cost и privacy свойствами.

#### Ограничения и риски

- **Runtime mismatch.** Pipecat — Python 3.11+ и asyncio framework. EvoHime
  не поставляет внешний Python runtime, а Core уже построен на Rust; прямое
  встраивание создаст второй lifecycle, memory model, packaging и security
  boundary.
- **Frame typing недостаточно для wire security.** Внутренние frames и bus
  могут переносить произвольные объекты, direct callbacks и local-only data.
  Priority задаёт порядок обработки, но не authorization. На границе Евы
  обязательны строгая схема, major version, payload limit, HMAC session,
  sequence/replay rules и capability checks.
- **Bus persistence/network surface.** PGMQ/Redis и внешние transports добавят
  credentials, egress, replay и retention вопросы. Для локальной Евы это не
  должно появиться автоматически; будущий bus обязателен к Core-owned auth,
  bounded queue и явной политике хранения.
- **Tool decorator не заменяет policy.** `cancel_on_interruption` и timeout
  могут остановить обработчик, но не решают approval, idempotency, side-effect
  receipt и ситуацию неизвестного результата после сетевого разрыва.
- **RTVI может раскрыть лишнее.** Function-call names/args, transcripts,
  metrics и UI commands относятся к разным уровням доверия. Нельзя считать
  desktop client или browser transport security boundary; default должен быть
  redacted/no-raw-args, с отдельными разрешениями на диагностическое раскрытие.
- **Provider semantics различаются.** Streaming, reconnect, audio formats,
  VAD, interruption и billing зависят от конкретного STT/TTS/LLM provider.
  Унифицированный interface Pipecat не устраняет необходимость capability
  negotiation, timeout, egress policy и provider-specific tests.
- **Realtime evals не являются security proof.** Behavioral tests проверяют
  пользовательское поведение и latency, но не доказывают отсутствие утечки,
  prompt injection или корректность authorization. Они должны дополнять
  security/permission tests и deterministic Core replay.
- **Лицензия и SDK.** BSD-2-Clause относится к коду Pipecat; provider SDK,
  модели, telephony services, WebRTC stack и записи для eval имеют отдельные
  лицензии/условия. Перед поставкой нужно вести component/license manifest.

#### Предварительное решение

`адаптировать frame/event taxonomy, interruption/cancellation semantics,
worker lifecycle, typed job/tool metadata, observers, RTVI-подобный UI contract
и behavioral evals`; `не подключать Pipecat Python runtime, Pipecat Cloud,
Redis/PGMQ bus или внешние transports как runtime-зависимости EvoHime`.

Pipecat и Moshi следует рассматривать на разных уровнях: Moshi — возможный
низкоуровневый voice runtime/provider, Pipecat — orchestration и evaluation
patterns вокруг такого provider. Они не должны одновременно становиться
двумя конкурирующими runtime-слоями внутри Core.

#### Связь с EvoHime

- Возможный design-only контракт: Core-owned `voice_session`, `voice_frame`,
  `voice_control`, `voice_tool_call`, `voice_event`, `provider_receipt` и
  `voice_metric`. Electron получает только authenticated, redacted state;
  workspace, provider secrets, approvals, SQLite audit и cancellation остаются
  в Core.
- Идеи worker/heartbeat/idle можно сопоставить с supervisor lifecycle и
  существующим Core task cancellation; не дублировать процессный supervisor
  внутри Electron или Python helper.
- Идеи RTVI и serializers проверять против `desktop-ipc-v1`: major version,
  bounded frame size, sequence replay, HMAC proof, typed errors и no raw
  callback/object crossing. Этот журнал implementation plan не создаёт.
- Критерии будущей проверки: system cancellation обгоняет data queue,
  terminal receipt переживает interruption, cancellation реально останавливает
  provider/helper, heartbeat и crash/restart очищают ресурсы, tool approval и
  idempotency сохраняются, события redacted/provenance-linked, latency/TTFB/
  TTFA измеряются, а behavioral evals воспроизводимы offline.

### 13. OpenAI Whisper

- Источник: [репозиторий openai/whisper](https://github.com/openai/whisper),
  [model card](https://github.com/openai/whisper/blob/main/model-card.md),
  [исследовательская статья](https://arxiv.org/abs/2212.04356)
- Дата проверки: 2026-08-21
- Ревизия/commit: `5f86d1d86363843179951550570367b37c5d6f78`
  (`2026-07-28`, `Fix SDPA cross-attention falling back to the math kernel during beam search`)
- Лицензия: MIT для кода и опубликованных model weights согласно README;
  attribution OpenAI должен сохраняться в поставке.
- Состав: Python package `openai-whisper`, PyTorch Transformer, audio/mel
  preprocessing, tokenizer, autoregressive decoder, word timing/DTW, CLI,
  model downloader и тестовые fixtures. Package требует Python `>=3.8`,
  PyTorch, NumPy, tiktoken, numba, tqdm и системный `ffmpeg` для файлового
  декодирования.
- Назначение: general-purpose multilingual ASR, language identification и
  speech-to-English translation. Это модель и reference inference code, а не
  готовый realtime service, supervisor или security boundary.
- Краткий вывод: Whisper полезен Еве как эталон ASR-семантики, наборов
  метрик и source для сравнения с текущим `whisper.cpp` listener. Прямой
  Python/PyTorch runtime в Windows-поставку не добавлять: текущий listener
  уже использует проверенный whisper.cpp DLL/runtime и лестницу моделей.

#### Что изучено

- **Аудиоконтракт.** Reference pipeline приводит вход к mono 16 kHz PCM,
  использует 400-sample FFT, 160-sample hop, 80/128 mel bins и 30-секундные
  окна (`480000` samples, `3000` mel frames). Один audio frame соответствует
  10 ms, один audio token — 20 ms. Короткие входы дополняются нулями, длинные
  окна обрезаются или обрабатываются последовательно.
- **Архитектура.** Это encoder-decoder Transformer: audio encoder строит
  features из log-Mel spectrogram, text decoder авторегрессивно выдаёт tokens.
  Специальные tokens кодируют язык, задачу, timestamps и конец сегмента.
  Поддерживаются language detection, transcription и translation в английский.
- **Сегментация.** `transcribe()` сначала строит общий mel для аудиофайла,
  затем проходит его sliding 30-second windows. Сегменты получают `start`,
  `end`, `text`, tokens, temperature, average log probability,
  compression ratio и no-speech probability. Встроенный API не является
  настоящим incremental streaming API: near-realtime поверх него нужно
  строить отдельным bounded segmenter/transport.
- **Контекст между окнами.** По умолчанию предыдущие output tokens передаются
  как prompt следующему окну. `condition_on_previous_text=false` уменьшает
  риск повторяющегося loop/hallucination, но может ухудшить согласованность
  текста между соседними сегментами. `initial_prompt` позволяет передать
  vocabulary/proper nouns, однако это untrusted hint, а не источник истины.
- **Decode fallback.** При слишком высоком gzip compression ratio или слишком
  низком average log probability decoder пробует следующую temperature из
  расписания. Сочетание no-speech probability и logprob позволяет пропускать
  тишину. Это полезная quality policy, но не замена VAD и не доказательство,
  что распознанный текст действительно произнесён.
- **Timestamps.** Segment timestamps берутся из timestamp tokens. Опциональные
  word timestamps получают alignment через cross-attention heads, median
  filter и Dynamic Time Warping; на CUDA используется Triton path с CPU
  fallback. Это отдельный более дорогой режим, а не бесплатное свойство
  каждого partial transcript.
- **Model ladder.** Доступны tiny/base/small/medium/large и multilingual
  turbo (`large-v3-turbo`), а также English-only варианты первых размеров.
  README указывает приблизительно: tiny/base ~1 GB VRAM, small ~2 GB,
  medium ~5 GB, large ~10 GB, turbo ~6 GB; реальные значения зависят от
  backend, dtype, batch и ОС. Turbo оптимизирован для быстрой транскрипции,
  но не предназначен для speech translation.
- **Model loading и supply chain.** Официальные checkpoints загружаются с
  pinned Azure Edge URLs, проверяются SHA-256 и кэшируются в
  `~/.cache/whisper`; `load_model()` также принимает локальный checkpoint и
  `in_memory`. Автоматическая загрузка удобна для research, но для Евы
  должна быть заменена на installer/runtime manifest и offline failure mode.
- **CLI/output.** CLI пишет txt, vtt, srt, tsv, json и jsonl, принимает
  language/task/model/device, clip timestamps, temperature, beam size,
  word timestamps и thread count. Для Core нужен versioned typed event, а не
  импорт пользовательского JSON-файла как доверенного состояния.
- **Тесты.** `test_transcribe.py` прогоняет все официальные модели на
  `tests/jfk.flac`, проверяет язык, текст, токены, segment/word timestamps и
  ключевые фразы. `test_timing.py` проверяет DTW и median filter на CPU/CUDA;
  отдельные tests покрывают tokenizer и English normalizer. В checkout
  выполнена проверка синтаксиса package через `python -m compileall`; скачивание
  больших моделей и полный model test suite не запускались.

#### Что можем использовать в Еве

- **ASR provider contract.** Взять из Whisper явные поля:
  `audio_sample_rate`, `channels`, `segment_id`, `start_ms`, `end_ms`,
  `text`, `language`, `language_probability`, `no_speech_probability`,
  `avg_logprob`, `model_revision`, `is_final` и provenance. Partial/final
  semantics, sequence и session IDs должны быть определены Core, а не выведены
  из текста.
- **Единый нормализованный аудиовход.** Закрепить 16 kHz mono PCM как
  внутренний формат listener/provider boundary, если это совместимо с уже
  реализованным listener contract. Перенять frame alignment, bounded ring
  buffer и явную длительность; файловый `ffmpeg` subprocess из Whisper в Core
  не переносить.
- **Bounded segmenter вместо обещания streaming.** Использовать 30-секундное
  окно как reference compatibility fixture, но для ambient listener иметь
  VAD/turn-based сегменты, overlap, flush и дедупликацию. Каждое partial
  событие должно иметь revision/segment ID, чтобы Core мог отклонить старый
  результат после interruption или restart.
- **Quality policy и fallback.** Добавить в provider receipt измеряемые
  confidence/quality signals и отдельные причины `silence`, `decode_retry`,
  `low_logprob`, `repetition_detected`, `segment_timeout`. Переключение
  `small -> base -> tiny` и остановка при деградации уже являются частью
  текущего listener; идеи Whisper помогают формализовать причины, но не
  должны создавать вторую независимую ladder policy.
- **Language/task policy.** Language detection и explicit language allow-list
  полезны для русского и multilingual режима. Translation в английский должна
  быть отдельной capability и event type, чтобы transcript исходной речи не
  подменялся переводом. Пользовательский prompt/vocabulary должен проходить
  bounded input и не может менять Core policy.
- **Model manifest.** Перенять имя модели, размер, dtype/backend, commit,
  checksum, license и capabilities в manifest. В текущей Еве это нужно
  сопоставить с уже существующим listener-runtime manifest; нельзя разрешать
  runtime скачивать произвольный checkpoint или DLL по имени.
- **Опциональные word timestamps.** Использовать как evidence для UI и
  alignment-aware transcript, но включать только по capability/latency policy.
  Сохранять word boundaries и confidence можно в redacted provenance; не
  считать timestamp precision доказательством семантической точности.
- **Evaluation fixtures.** Перенять проверку коротких и длинных сегментов,
  языкового определения, шумов/акцентов, silence, repeated hallucination,
  prompt carry/reset, word timestamps, CPU/GPU fallback и final text/token
  consistency. Добавить русские fixtures и сравнивать Python reference,
  текущий whisper.cpp и выбранные model rungs на одинаковом аудио.
- **Deterministic model identity.** Логировать только model revision,
  backend, quantization, sample rate, segment policy и quality thresholds;
  сырые аудио и полные token trajectories не писать в обычный telemetry.
  Это помогает сопоставить ASR receipt с ambient utterance и не раскрывать
  содержимое разговора без retention/consent решения.

#### Ограничения и риски

- **Нет realtime out of the box.** Model card прямо предупреждает, что
  Whisper не предназначен для real-time transcription без дополнительной
  обвязки. Sliding 30-second inference, autoregressive decoding и optional
  word alignment дают задержку и усложняют barge-in/cancellation.
- **Hallucinations и repetition.** Weakly supervised training может выдавать
  текст, которого нет в аудио, особенно на тишине, длинных паузах и языках с
  меньшим объёмом данных. Нельзя передавать transcript напрямую в tool
  arguments или approval evidence без provenance, confidence и policy.
- **Неравномерность языков.** Качество заметно зависит от языка, акцента,
  диалекта и домена; русская речь требует отдельного benchmark, а не вывода
  из англоязычного `jfk.flac` fixture. ASR нельзя использовать для inferred
  attributes, subjective classification или high-risk decisions.
- **Ресурсы.** PyTorch/large models потребляют существенную RAM/VRAM; CPU
  fallback может не удовлетворить latency budget. Model ladder и runtime
  degradation должны быть bounded и observable, иначе listener начнёт
  накапливать audio или partial results.
- **Python/PyTorch/ffmpeg mismatch.** Прямой package добавит внешний Python,
  PyTorch, tiktoken/numba, ffmpeg subprocess и отдельный model cache, что
  противоречит текущей поставке Electron + Rust Core. Даже локальная модель
  не отменяет packaging, crash cleanup, cancellation и supply-chain проверки.
- **Сетевая загрузка checkpoint.** Автоматический download создаёт network
  egress и риск подмены/неожиданного размера; SHA-256 в upstream полезен, но
  для Евы нужен релизный manifest, size limit, owner-only cache и отсутствие
  скачивания во время пользовательского запуска.
- **Audio privacy и consent.** Микрофонные записи и transcript являются
  чувствительными данными. Upstream не задаёт retention, DPAPI/ACL, redaction,
  deletion, audit или approval; эти свойства остаются ответственностью Core.
- **Reference tests не покрывают продуктовые свойства.** Нет полноценной
  проверки Windows packaging, supervisor restart, authenticated IPC,
  long-lived streaming, cancellation, queue overflow, model corruption или
  prompt-injection через transcript. Их нужно добавить в EvoHime conformance
  suite.
- **Лицензия модели и производные сборки.** MIT упрощает использование кода
  и опубликованных weights, но конкретные quantized/converted checkpoints,
  CUDA/Triton/PyTorch/ffmpeg и сторонние model artifacts нужно учитывать в
  component/license manifest. Нельзя считать лицензию Python package лицензией
  всего будущего listener-runtime.

#### Предварительное решение

`адаптировать ASR event contract, audio normalization, quality signals,
manifest/checksum policy и evaluation fixtures`; `наблюдать за upstream
Whisper как reference model`; `не подключать openai-whisper Python/PyTorch
runtime и автоматическую загрузку моделей в desktop-поставку`.

Текущий listener EvoHime уже основан на whisper.cpp с проверенным DLL,
ABI/runtime manifest и ladder `small -> base -> tiny`. Поэтому OpenAI Whisper
здесь не заменяет реализованный runtime: его роль — источник reference
семантики, сравнительных fixtures и проверок качества. С Moshi Whisper
соотносится как offline/segmented ASR baseline, а не как full-duplex voice
runtime; с Pipecat — как provider, который должен быть скрыт за общим
orchestration contract.

#### Связь с EvoHime

- Сопоставить поля Whisper с текущими `ambient_utterances`, listener contract
  и `engine_version`; не менять эти контракты в рамках исследования.
- Сохранить Core/supervisor ownership: Electron получает только transcript
  state, Core владеет microphone permission, segmenter, model manifest,
  cancellation, provenance, SQLite и redaction.
- Для будущего этапа подготовить design-only comparison harness: одинаковый
  PCM fixture прогоняется через текущий whisper.cpp runtime и reference
  Whisper-compatible backend с фиксированными model revision/thresholds.
  Этот журнал implementation plan не создаёт.
- Критерии будущей проверки: 16 kHz mono/frame alignment, bounded audio memory,
  partial/final ordering, cancellation при активном decode, no-speech и
  hallucination rejection, language accuracy на русской речи, word timestamp
  cost, model checksum/license manifest, offline startup, supervisor restart,
  redacted provenance и отсутствие необъявленного network egress.

## Итог для будущего плана

Этот раздел заполняется после завершения набора исследований:

- подтверждённые возможности для интеграции;
- идеи, которые реализуем самостоятельно без заимствования кода;
- внешние компоненты, допустимые после проверки лицензии;
- отклонённые варианты и причины;
- зависимости, порядок этапов и критерии готовности.
