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
| 14 | [jianfch/stable-ts](https://github.com/jianfch/stable-ts) | Исследовано | Whisper post-processing: silence suppression/VAD, stable word timestamps, alignment/refinement, deterministic regrouping и structured result | Адаптировать post-processing/evaluation идеи; archived Python package и Silero/PyTorch dependencies не подключать |
| 15 | [pyannote/speaker-diarization-3.1](https://huggingface.co/pyannote/speaker-diarization-3.1) | Исследовано | Speaker diarization pipeline, speaker segmentation/embeddings, overlap-aware Annotation, VAD и RTTM export | Рассматривать как optional offline enrichment; не подключать gated Python/PyTorch runtime и не трактовать кластеры как identity |
| 16 | [vocodedev/vocode-core](https://github.com/vocodedev/vocode-core) | Исследовано | Streaming voice-agent loop, typed STT/LLM/TTS/action contracts, barge-in, endpointing, transcript events и telephony adapters | Адаптировать lifecycle/interrupt/tool/evaluation идеи; Python SDK, cloud actions и telephony runtime не подключать |
| 17 | [saharmor/voice-lab](https://github.com/saharmor/voice-lab) | Исследовано | Evaluation framework: JSON-сценарии, personas, model/prompt matrix, LLM-as-a-Judge, cost/quality comparison и экспериментальные speech metrics | Адаптировать evaluation contracts, fixtures и report provenance; Python scripts, cloud eval agent и pyannote/stable-ts pipeline не подключать |
| 18 | [Qwen/Qwen2-VL collection](https://huggingface.co/collections/Qwen/qwen2-vl) | Исследовано | Vision-language models для image/video understanding, OCR, multilingual visual QA, dynamic resolution и локальных quantized variants | Рассматривать как optional vision backend/PoC; не подключать к базовому runtime до GPU/memory/licensing/privacy плана |
| 19 | [mPLUG/DocOwl2](https://huggingface.co/mPLUG/DocOwl2) | Исследовано | OCR-free multi-page document understanding, high-resolution DocCompressor, page evidence, cross-page QA и document benchmark suite | Рассматривать как optional document-worker PoC; адаптировать page/evidence/evaluation contracts, custom Python/CUDA runtime не подключать напрямую |
| 20 | [mem0ai/mem0](https://github.com/mem0ai/mem0) | Исследовано | Long-term memory API, scoped user/agent/run memory, additive extraction, SQLite history, hybrid retrieval, expiration и entity linking | Адаптировать memory contracts, provenance, hybrid retrieval и forget semantics; Python SDK, cloud service, default telemetry и raw-memory storage не подключать напрямую |
| 21 | [letta-ai/letta](https://github.com/letta-ai/letta) | Исследовано | Stateful agent, memory blocks, recall, git-backed MemFS, context lifecycle, memory tools, sandbox confinement и persistent agent identity | Адаптировать layered-memory/context/approval contracts и историю изменений; `letta-code`, Cloud, App Server и архивную V1 напрямую не подключать |
| 22 | [AgentOps-AI/agentops](https://github.com/AgentOps-AI/agentops) | Исследовано | OpenTelemetry traces/spans, LLM/tool/workflow semantic conventions, token/cost/latency metrics, session replay и evaluation validation | Адаптировать event/trace/metrics contracts и local observability; Python SDK, облачный OTLP и self-hosted dashboard напрямую не подключать |
| 23 | [THUDM/AgentBench](https://github.com/THUDM/AgentBench) | Исследовано | Multitask agent evaluation, isolated task workers, function-calling protocol, deterministic environments, trajectory/reward scoring и resource budgets | Адаптировать evaluation/scenario/trajectory contracts и bounded fixtures; benchmark runtime, Docker task stack, внешние datasets/services и production host actions не подключать |
| 24 | [traceloop/openllmetry](https://github.com/traceloop/openllmetry) | Исследовано | OpenTelemetry GenAI semantic conventions, provider/vector DB instrumentation, manual spans, content policy, trajectory capture, prompt provenance и guardrail/evaluator hooks | Адаптировать typed local telemetry, usage/latency schemas, capture modes и evaluation fixtures; Python SDK, monkey-patching, OTLP/cloud export и remote guardrails напрямую не подключать |
| 25 | [OpenBMB/AgentVerse](https://github.com/OpenBMB/AgentVerse) | Исследовано | Multi-agent task-solving pipeline, simulation environments, typed messages, role assignment, decision/execution/evaluation stages, visibility/order rules и pluggable registries | Адаптировать environment/step/reset, role/task/evaluator separation, scoped message routing и deterministic simulation fixtures; Python runtime, direct LangChain/BMTools/XAgent и uncontrolled multi-agent autonomy не подключать |
| 26 | [sierra-research/tau-bench](https://github.com/sierra-research/tau-bench) | Исследовано | Stateful benchmark tool-agent-user interaction, policy-guided domain tools, simulated users, deterministic state-hash rewards, trajectories, Pass^k и error attribution | Адаптировать environment/tool/task/evaluator contracts, explicit confirmation fixtures, state predicates, user/adversarial scenarios и reliability metrics; устаревший Python benchmark, внешний LLM user, LiteLLM/providers и production side effects не подключать |

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

### 14. stable-ts

- Источник: [репозиторий jianfch/stable-ts](https://github.com/jianfch/stable-ts),
  [README с API и параметрами](https://github.com/jianfch/stable-ts/blob/main/README.md)
- Дата проверки: 2026-08-21
- Ревизия/commit: `e312072cc024ae9fceb25b057d7d18524873a02b`
  (`2026-05-30`, `Add note about paused development`)
- Статус: репозиторий архивирован владельцем 2026-05-30 и read-only; README
  сообщает, что разработка indefinitely paused.
- Версия: `2.19.1`.
- Лицензия: MIT, copyright Jian 2022; лицензии Whisper, Silero VAD,
  faster-whisper, Hugging Face, MLX, Demucs и других optional components
  проверяются отдельно.
- Состав: Python package `stable_whisper`, Whisper word-level wrapper,
  `WhisperResult`/`Segment`/`WordTiming`, silence stabilization, VAD/audio
  helpers, alignment/refinement, regroup/split/merge operations, output to
  SRT/VTT/ASS/TSV/JSON и adapters для Faster-Whisper, Hugging Face и MLX.
- Зависимости: Python `>=3.8`, NumPy, PyTorch, torchaudio, tqdm и
  `openai-whisper>=20230314,<=20250625`; optional extras тянут
  faster-whisper, Transformers/Optimum/Accelerate или mlx-whisper.
- Назначение: повысить стабильность Whisper timestamps и сделать результат
  пригодным для субтитров, поиска, alignment и дальнейшей обработки. Это
  post-processing library, а не отдельная ASR-модель, realtime service или
  security boundary.
- Краткий вывод: полезен для алгоритмов обработки word/segment timestamps и
  для тестовых сценариев ambient transcript. Как runtime-зависимость Евы не
  подходит из-за archived статуса, Python/PyTorch stack, mutable result model,
  optional model downloads и отсутствия product-level lifecycle/security.

#### Что изучено

- **Позиционирование над Whisper.** `transcribe()` модифицирует Whisper decode,
  добавляет preprocessing (voice isolation/noise removal, low/high-pass), а
  затем корректирует timestamps по тишине и regrouping. `transcribe_minimal()`
  сохраняет более близкую к upstream Whisper decode path, но всё равно
  применяет post-processing.
- **Silence suppression.** Алгоритм строит silent intervals по waveform
  loudness/threshold mask или получает их от Silero VAD. Затем он двигает
  start/end word timestamps к границам речи, учитывая `min_word_dur`,
  `min_silence_dur`, `nonspeech_error`, положение слова и разрешение оставить
  начало либо конец. Текст и token sequence при этом не должны заменяться
  молчащим post-processing.
- **VAD modes.** `vad=True` подключает Silero VAD с настраиваемым threshold,
  sample-rate/chunk constraints и cached model instance. Без VAD stable-ts
  использует собственную energy/silence suppression mask с `q_levels` и
  `k_size`. README отдельно отмечает, что `suppress_silence=True` не
  поддерживается для некоторых `AudioLoader` streaming paths.
- **Word-level timing.** Word timings строятся через cross-attention
  alignment/DTW и затем используются как основа для suppression и regrouping.
  Для отдельного `align()`/`align_words()` можно передать уже известный текст
  или segment result и получить новые временные границы без замены текста.
  `refine()` использует другой model pass для уточнения существующих word
  timestamps; это увеличивает latency и memory.
- **Regrouping.** `WhisperResult.regroup()` принимает детерминированную строку
  операций либо default algorithm `da`. Операции делят/объединяют слова и
  segments по gap, punctuation, длине, duration, sentence boundaries и
  instant words. История операций сохраняется в `regroup_history`, что даёт
  полезный audit trail для воспроизводимого post-processing.
- **Result model.** `WhisperResult` содержит language, segments и words;
  `Segment` и `WordTiming` поддерживают offset, split, merge, pad, clamp,
  remove, search/find, lock и conversion между word/segment level. Это
  удобная in-memory representation, но она mutable и не является durable
  SQLite schema или authenticated IPC payload.
- **Streaming/file handling.** `AudioLoader` умеет читать source кусками,
  ограничивать portions и передавать progress callback; `stream=True` в
  основном означает chunked loading 30-second windows. Это не делает
  Whisper decode true low-latency streaming и не задаёт cancel/restart,
  queue/backpressure или partial transcript protocol.
- **Alternative backends.** Adapters позволяют прогонять похожий
  post-processing поверх Faster-Whisper, batched inference, Hugging Face и
  MLX. Для alignment/refinement есть предупреждение о дополнительной цене,
  особенно на Faster-Whisper. Это показывает полезную границу: стабилизация
  результата может быть отделена от конкретного inference engine.
- **Long-form and recovery helpers.** Есть `clip_timestamps`, progress
  callback, `resume` из незавершённого result и операции locate/find. Для
  локальных файлов это полезно, но resume-файл нельзя автоматически считать
  trusted Core state: его нужно связывать с audio hash, model revision и
  policy snapshot.
- **Outputs.** Package умеет SRT/VTT/ASS word-level output, TSV, JSON и
  karaoke-like formats. Для Евы это источник export/view model ideas, а не
  внутренний storage contract: Core должен хранить typed provenance и schema
  version, а экспорт строить отдельно.
- **Тесты.** `test_transcribe.py` проверяет text/word timestamps на JFK
  fixture, `test_align.py` сравнивает align/align_words, `test_refine.py`
  проверяет изменение временных границ. В checkout выполнен `compileall`;
  синтаксис package прошёл, но выдан SyntaxWarning об escape sequence в
  docstring. Model-dependent tests и VAD/русские/noise fixtures не запускались.

#### Что можем использовать в Еве

- **Отдельный transcript post-processing stage.** После engine decode ввести
  Core-owned этап `raw_segments -> stabilized_segments -> ambient_utterance`.
  Engine только сообщает text/timestamps/confidence, а стабилизатор может
  корректировать границы, split/merge и silence gaps с отдельной версией
  алгоритма.
- **Безопасная коррекция границ.** Перенять принцип: suppression меняет
  temporal boundaries, но не silently rewrites recognized text/tokens. В
  receipt хранить `raw_start/end`, `stable_start/end`, `stabilizer_revision`,
  `silence_source` и reason, чтобы UI и audit могли отличить inference от
  post-processing.
- **Два режима silence detection.** Описать capability `energy_mask` и
  `vad_model` с разными quality/cost/privacy профилями. Текущий listener уже
  владеет segmentation и model ladder, поэтому stable-ts не должен создавать
  второй VAD state machine; его правила можно использовать для сравнения и
  уточнения существующего этапа.
- **Deterministic regroup DSL.** Идея строкового `regroup_algo` полезна как
  design pattern, но в Еве команды должны быть typed enum/serde schema, а не
  свободная строка. Каждая операция получает bounded args, revision и
  before/after counts; invalid chain отклоняется до изменения результата.
- **Immutable-ish result snapshots.** `WhisperResult` показывает удобный
  набор word/segment operations. Для Core лучше сделать immutable versioned
  snapshots или event-based transform: `raw -> stabilized -> grouped`, с
  сохранением raw result и возможностью deterministic replay.
- **Alignment/refinement как опциональная команда.** Пользовательский
  transcript editor или offline export может запросить дорогой refine/alignment
  job. Для ambient real-time path по умолчанию отключить его, использовать
  deadline/cancellation и не блокировать microphone/agent loop.
- **Resume contract.** Для длинных аудиозаписей полезен checkpoint с
  `source_hash`, source portion, last stable segment ID, model revision,
  decoder options и post-processing revision. Это можно сопоставить с
  existing transcript storage, но checkpoint должен быть authenticated и
  безопасно удаляемым.
- **Search/evidence helpers.** `find`/`locate` и сохранение segment/word
  boundaries могут улучшить поиск по локальному transcript и citation
  evidence. Результат поиска должен ссылаться на `episode_id`/utterance ID,
  timestamp и provenance, а не превращаться в новую копию текста без связи.
- **Backend conformance.** Stable-ts показывает, что один стабилизатор можно
  проверять поверх whisper.cpp, reference Whisper и Faster-Whisper. Это
  хороший будущий comparison harness для EvoHime, но не причина тянуть все
  backend adapters в поставку.
- **Regression fixtures.** Перенять классы тестов: слово пересекает
  timestamp boundary, длинная тишина до/после слова, короткая межсловная
  пауза, punctuation split/merge, repeated/hallucinated segment, align result
  с другим текстом, resume после обрыва и монотонность timestamps.

#### Ограничения и риски

- **Проект архивирован.** Upstream development indefinitely paused, поэтому
  новые версии Whisper/PyTorch/Transformers и Windows packaging могут сломать
  compatibility. Нельзя добавлять stable-ts как floating dependency или
  строить на нём долгосрочный runtime contract.
- **Python/PyTorch dependency surface.** Package требует внешний Python,
  PyTorch, torchaudio и FFmpeg; optional VAD тянет Silero model, а alternative
  backends добавляют собственные runtimes. Это несовместимо с правилом
  EvoHime о Rust Core и отсутствующем внешнем Python в поставке.
- **Silence adjustment может скрыть речь.** Threshold/VAD ошибается на шуме,
  шёпоте, музыке, overlap и русском акценте. Слишком агрессивная suppression
  может двигать timestamp, удалять segment или ломать короткие слова. Нужны
  confidence, raw boundaries и возможность отключить correction.
- **Post-processing не лечит hallucination полностью.** Suppress silence и
  instant-word filters уменьшают артефакты времени/тишины, но не доказывают,
  что текст произнесён. Transcript нельзя использовать как permission,
  approval или tool argument без Core policy и user confirmation.
- **Mutable result и provenance gap.** Методы `remove`, `split`, `merge`,
  `regroup` изменяют in-memory result; `regroup_history` полезен, но не
  заменяет signed/auditable event chain. В Еве каждый transform должен быть
  versioned, bounded и воспроизводимым.
- **External URL/audio surface.** Поддержка URL, yt-dlp и FFmpeg удобна для
  research, но создаёт SSRF, shell/process, egress, oversized input и privacy
  риски. Core должен принимать только approved local/provider audio stream;
  network fetching остаётся отдельным permissioned tool.
- **Extra model and memory cost.** VAD, second alignment/refinement model,
  denoiser/Demucs и dynamic attention heads могут удвоить latency/memory.
  Нельзя включать их в microphone path без budget, cancellation и fallback.
- **Compatibility pins.** `openai-whisper<=20250625` и optional upper bounds
  на Transformers отражают реальную fragility monkey-patching/adapter layer.
  Для текущего whisper.cpp ABI эти Python compatibility rules напрямую не
  переносятся.
- **Test coverage is narrow.** Основные fixtures — короткая англоязычная
  речь JFK; нет доказанной точности VAD на русском, speaker overlap, noisy
  ambient audio, long-running Windows listener, crash/restart и bounded queue.
- **Лицензии компонентов.** MIT stable-ts не покрывает Silero VAD, Demucs,
  PyTorch, FFmpeg, Faster-Whisper, MLX или downloaded checkpoints. Нужен
  component/license manifest, если какие-либо алгоритмы или assets попадут в
  поставку.

#### Предварительное решение

`адаптировать silence-boundary correction, deterministic regrouping,
versioned transcript transforms, alignment/refinement job semantics и
regression fixtures`; `наблюдать за stable-ts как archived reference`;
`не подключать Python package, Silero VAD, yt-dlp/FFmpeg URL path и optional
backends в desktop runtime`.

Stable-ts логически дополняет Whisper, но не заменяет текущий whisper.cpp:
Whisper задаёт ASR reference, stable-ts — post-processing reference, а
EvoHime listener остаётся Core-owned native runtime с уже существующими
manifest, segmentation, deduplication и model-ladder policy.

#### Связь с EvoHime

- Сопоставить `WordTiming`/`Segment` с текущими `ambient_utterances` и
  listener contract только на уровне design/evaluation; исходные Rust
  контракты в рамках исследования не менять.
- Возможный будущий этап: отдельный deterministic `transcript-stabilizer`
  в Rust Core, который получает raw listener segments и возвращает новую
  versioned snapshot, сохраняя raw boundaries и provenance.
- Для VAD/silence сравнить stable-ts rules с уже реализованной segmentation;
  не допускать двух конкурирующих источников истины для utterance boundaries.
- Критерии будущей проверки: монотонность и bounded timestamps, raw/stable
  provenance, no text mutation during boundary correction, deterministic
  regroup replay, русская речь/noise/silence fixtures, cancellation of refine,
  no URL egress, no unbounded audio retention и корректное восстановление
  после restart.

### 15. pyannote/speaker-diarization-3.1

- Источник: [модельная страница Hugging Face](https://huggingface.co/pyannote/speaker-diarization-3.1),
  [pyannote.audio toolkit](https://github.com/pyannote/pyannote-audio),
  [зависимая segmentation-3.0](https://huggingface.co/pyannote/segmentation-3.0)
- Дата проверки: 2026-08-21
- Ревизия модели: `84fd25912480287da0247647c3d2b4853cb3ee5d`
  (`lastModified: 2024-05-10` по Hugging Face model API).
- Статус: gated model. Репозиторий виден публично, но для файлов нужно
  принять условия модели, принять условия `pyannote/segmentation-3.0` и
  создать Hugging Face access token; gated form запрашивает контактные поля.
- Лицензия модели: MIT по model card. Лицензии pyannote.audio, PyTorch,
  segmentation/embedding checkpoints, обучающих датасетов и любых
  quantized/converted artifacts проверяются отдельно.
- Состав: `pyannote.audio` Pipeline для speaker diarization, pure-PyTorch
  speaker segmentation и speaker embedding stages. Версия 3.1 повторяет
  pipeline 3.0, но убирает problematic `onnxruntime`; оба stages выполняются
  в PyTorch. Требуется `pyannote.audio>=3.1`.
- Вход/выход: mono audio at 16 kHz; stereo/multichannel автоматически
  downmix-ится усреднением, другие sample rates resample-ятся. Pipeline
  возвращает `pyannote.core.Annotation` с временными интервалами и
  anonymous speaker labels; результат можно экспортировать в RTTM.
- Назначение: определить, когда какой анонимный speaker cluster говорит,
  включая overlapped speech. Это speaker diarization, а не ASR, voiceprint
  identification или подтверждение личности.
- Краткий вывод: модель потенциально полезна для разделения ambient
  transcript по говорящим, но только как отдельный opt-in/offline enrichment.
  Gated access, Python/PyTorch runtime, дополнительный audio retention и
  privacy-риск голосовых признаков не позволяют включать её в базовый
  microphone path Евы сейчас.

#### Что изучено

- **Pipeline contract.** `Pipeline.from_pretrained()` загружает конфигурацию
  и связанные model artifacts, после чего `pipeline(audio)` возвращает
  `Annotation`. Можно обрабатывать файл целиком или передавать waveform и
  sample rate из памяти/конкретного excerpt; это удобная граница для
  отдельного Core job, но не готовый desktop IPC contract.
- **Segmentation stage.** Зависимая `segmentation-3.0` принимает 10 секунд
  mono 16 kHz и выдаёт frame-by-class powerset matrix. Классы включают
  non-speech, одиночных speakers и пары одновременно говорящих speakers.
  Таким образом overlap моделируется явно, а не теряется при выборе одного
  максимального speaker.
- **Embedding/clustering stage.** Полный diarization pipeline добавляет
  speaker embedding и связывает локальные speech turns в anonymous labels.
  Метка `SPEAKER_00`/`SPEAKER_01` стабильна только в пределах конкретного
  inference context; она не является глобальным именем человека и не должна
  переноситься между episodes без отдельного consented identity layer.
- **VAD и overlap.** Pipeline работает автоматически без ручного VAD и без
  обязательного `num_speakers`; при этом можно задать `num_speakers`,
  `min_speakers` и `max_speakers`. Benchmark использует строгий DER setup
  без forgiveness collar и с учётом overlapped speech; это полезнее для
  оценки ambient-сценариев, чем обычная только-один-speaker accuracy.
- **Input normalization.** Автоматический downmix/resample удобен, но может
  скрыть ошибку в capture contract. Для Евы нужно до вызова pipeline явно
  записывать исходный sample rate/channel layout и результат normalization;
  model input не должен silently менять provenance audio.
- **Compute modes.** Pipeline работает на CPU по умолчанию и может быть
  отправлен на CUDA. Model card 3.0 указывает около 1.5 минут на час записи
  для neural inference + clustering на V100/Cascade Lake; это benchmark
  reference, а не гарантия на Windows CPU/GPU Евы. Для 3.1 собственный
  hardware/latency profile в рамках исследования не запускался.
- **Progress и bounds.** `ProgressHook` позволяет наблюдать processing
  progress; speaker-count bounds уменьшают неопределённость, если контекст
  заранее известен. Оба параметра должны стать Core-owned job settings с
  bounded values, а не управляться transcript или model output.
- **Model access.** Загрузка из Hugging Face требует token и принятия условий
  двух gated repositories. На странице нет доступного Inference Provider;
  типичный путь — локальный Python/PyTorch inference. Runtime HF token нельзя
  помещать в Electron, SQLite, логи или обычный Core environment.
- **Current upstream direction.** В актуальном README pyannote.audio legacy
  pipeline 3.1 сравнивается с более новым `speaker-diarization-community-1`,
  который улучшает speaker counting/assignment. Поэтому 3.1 следует считать
  reference/legacy candidate, а не автоматически выбирать для нового плана.
- **Telemetry.** Текущий pyannote.audio toolkit содержит optional telemetry:
  при включении он может отправлять anonymous pipeline origin, class, audio
  duration и speaker-count parameters. Для local-first Евы telemetry должна
  быть default-deny и проверяться отдельно от model inference.
- **Benchmarks.** 3.1 model card показывает DER/false alarm/miss/confusion на
  AISHELL-4, AliMeeting, AMI, AVA-AVD, DIHARD, MSDWild, REPERE и VoxConverse.
  Эти цифры полезны для сравнения pipeline, но не подтверждают качество на
  русской бытовой речи, микрофоне Windows или ambient episode Евы.

#### Что можем использовать в Еве

- **Optional diarization enrichment.** Ввести отдельный post-processing/job
  слой: `ambient_utterance + audio window -> speaker_spans`. Не включать его
  в базовую транскрипцию и не задерживать появление обычного utterance.
  Если job недоступен или отменён, transcript должен оставаться полноценным
  с текущим speaker state `unverified`.
- **Span-based schema.** Перенять модель `Annotation` как список
  `{start_ms, end_ms, cluster_id, overlap}`. Не добавлять один обязательный
  `speaker_id` к utterance: один utterance может пересекать несколько
  speakers или не иметь достаточно уверенного сопоставления. Mapping между
  ASR segments и diarization spans должен хранить overlap/IoU и uncertainty.
- **Anonymous cluster semantics.** Записывать только episode-scoped
  `speaker_cluster_id` вроде `cluster-0`, с признаком `unverified`. Не
  пытаться по голосу назвать человека, сопоставлять cluster с identity или
  строить voiceprint без отдельного явного consent/permission дизайна.
- **Overlap-aware transcript.** Если diarization возвращает два активных
  speakers, Core должен сохранить overlap как событие/спаны, а не выбрать
  одного говорящего и потерять информацию. UI может показывать
  `multiple-speakers`/`uncertain`, но не должен превращать это в уверенную
  реплику конкретного пользователя.
- **Known speaker-count bounds.** Для контролируемых meeting/room sessions
  можно передать bounded `min_speakers`/`max_speakers` или exact count. Эти
  значения должны приходить из user/session policy и попадать в redacted
  provenance; модель не должна сама менять их в процессе.
- **RTTM/evidence export.** RTTM-подобное представление подходит как
  interoperable export для offline tools и evaluation. Внутри Core нужен
  versioned serde schema с episode ID, source audio hash, model revision,
  normalization, thresholds и processing status; RTTM строится как export,
  а не хранится единственным источником истины.
- **Diarization metrics.** Перенять DER decomposition: false alarm, missed
  speech, confusion и overlap-aware scoring. Для Евы добавить cluster purity,
  assignment stability across chunks, latency, cancellation и privacy tests.
- **Batch/offline job lifecycle.** `ProgressHook` и excerpt processing
  подсказывают контракт bounded job: `queued -> running -> partial ->
  completed/cancelled/failed`, с progress, deadline, heartbeat и no-audio
  retention after completion. Результат связывать с transcript revision, а
  не менять уже опубликованный utterance незаметно.
- **Model manifest/pinning.** Зафиксировать model revision, dependent
  segmentation revision, pyannote.audio compatibility, PyTorch backend,
  checksum/size/license manifest и telemetry setting. Gated download должен
  происходить только в контролируемой поставке/подготовке runtime, не во
  время обычного запуска и не через Electron.
- **Local-only policy.** Если когда-либо появится diarization provider,
  разрешить только локальный host за supervisor/Core IPC либо явно
  permissioned provider. Audio, embeddings, access token и raw Annotation не
  должны уходить в HF/pyannote cloud без отдельного egress approval.
- **Evaluation fixtures.** Добавить короткие русские fixtures с одним,
  двумя и тремя speakers, overlap, speaker changes, silence, noise,
  multichannel downmix, resampling, unknown speaker и cancellation mid-job.
  Результат проверять по spans и anonymous cluster consistency, а не только
  по transcript text.

#### Ограничения и риски

- **Gated Hugging Face access.** Нужны пользовательские условия, контактные
  данные, HF token и доступ к зависимому gated `segmentation-3.0`. Это
  неприемлемо как скрытая runtime-зависимость local-first продукта; token
  нельзя распространять в installer или хранить в пользовательском profile
  без отдельной secret policy.
- **Voice privacy.** Diarization использует speaker embeddings внутри
  pipeline. Даже если наружу выдаются только anonymous labels, raw audio,
  embeddings и длительные cluster histories могут быть биометрически
  чувствительными. В Еве не сохранять embeddings/voiceprints по умолчанию,
  ограничить retention и проводить отдельный consent review.
- **Anonymous labels are not identity.** Cluster assignment может меняться
  между episodes, chunks, model revisions и recovery runs. Нельзя отвечать
  на вопрос «кто говорил» только по `SPEAKER_00`; максимум — «какой
  anonymous cluster говорил в этом episode».
- **Overlap and error modes.** Cross-talk, room echo, music, far-field mic,
  short utterances и одновременная речь дают false alarm/miss/confusion.
  Ошибка diarization не должна удалять ASR transcript или менять его текст.
- **Batch latency.** Full diarization включает segmentation, embeddings и
  clustering; это тяжелее текущего low-latency listener. Нельзя запускать
  модель на каждом audio frame или блокировать ambient capture ожиданием
  полного episode.
- **Python/PyTorch mismatch.** `pyannote.audio` — Python-first PyTorch
  toolkit с FFmpeg/torch audio ecosystem. Прямое встраивание создаст второй
  runtime и packaging/supervisor boundary, что не соответствует архитектуре
  EvoHime.
- **Implicit normalization.** Автоматические downmix/resample могут скрыть
  неправильный capture input и сделать timestamps/quality incomparable. Все
  преобразования должны быть зафиксированы в provenance и bounded по размеру.
- **Telemetry and egress.** Optional pyannote telemetry нужно отключать
  явно; HF model fetching и возможные provider/cloud alternatives создают
  network egress. Не считать MIT license разрешением на передачу microphone
  audio или metadata третьей стороне.
- **Legacy model.** Текущий pyannote.audio README называет 3.1 legacy по
  сравнению с `community-1`; выбор 3.1 без benchmark против successor может
  закрепить худшее speaker counting/assignment. При этом community-1 тоже
  требует отдельной проверки лицензии, gated access, ресурса и Windows
  packaging.
- **Model/data licensing.** MIT страницы модели не распространяется
  автоматически на upstream datasets, dependent checkpoints, PyTorch,
  FFmpeg или converted/quantized files. Перед поставкой нужен полный
  component manifest.
- **Storage conflict.** Текущий ambient storage намеренно не содержит audio
  BLOB. Без короткого opt-in audio buffer diarization нельзя надёжно выполнить
  после того, как listener уже выбросил PCM; добавлять скрытую запись ради
  модели нельзя.

#### Предварительное решение

`адаптировать anonymous span schema, overlap semantics, DER evaluation,
offline job lifecycle, model manifest и privacy policy`; `наблюдать за 3.1 как
legacy reference`; `не подключать gated pyannote Python/PyTorch pipeline,
HF token, speaker embeddings или cloud/provider diarization в базовый runtime
Евы`.

Модель может появиться только после отдельного opt-in дизайна ambient audio:
ограниченный in-memory/temporary buffer, явное разрешение, retention/erase,
локальный host, bounded job и отдельная проверка community-1 против 3.1. До
этого текущая семантика `speaker=unverified` остаётся источником истины.

#### Связь с EvoHime

- Текущие `ambient_episodes`/`ambient_utterances` и правило, что аудио не
  хранится в SQLite, не менять в рамках исследования. Diarization не должна
  автоматически добавлять BLOB или persistent voiceprint.
- Будущий design-only контракт может содержать `speaker_spans` с
  episode-scoped anonymous cluster, overlap, confidence/uncertainty,
  model_revision, audio_revision и processing status; speaker identity
  остаётся `unverified`.
- Core владеет microphone permission, bounded audio lifetime, model access,
  cancellation, provenance, redaction и storage; Electron показывает только
  redacted diarization state через authenticated IPC.
- Критерии будущей проверки: gated credentials не попадают в продуктовые
  логи/installer, telemetry и egress default-deny, model artifacts pinned,
  no embeddings persisted, overlap не теряется, ASR не блокируется batch job,
  cancellation очищает audio/model state, cluster IDs не трактуются как
  identity, DER/cluster metrics воспроизводимы на русских fixtures и
  retention/forget удаляют временный audio вместе с derived spans.

### 16. Vocode Core

- **Источник:** [vocodedev/vocode-core](https://github.com/vocodedev/vocode-core), [README](https://github.com/vocodedev/vocode-core/blob/main/README.md)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `e054c33a72787b6a4920f91eb8598ad0bafb4240`
- **Лицензия исходного кода:** MIT
- **Состав:** Python SDK `vocode`, версия `0.1.114a2`, `StreamingConversation`, provider adapters для transcriber/agent/synthesizer, локальный microphone I/O, WebSocket/LiveKit и telephony-интеграции.
- **Назначение:** построение real-time voice agents для микрофона, телефонных звонков, Zoom и streaming transport.
- **Краткий вывод:** это полезный практический reference для жизненного цикла одной голосовой сессии, barge-in и событийного transcript. Сам runtime Еве не подходит: репозиторий Python-first, последний основной commit датирован 2024-11-15, зависим от большого набора внешних провайдеров и допускает небезопасные для Core внешние actions.

#### Что изучено

- `StreamingConversation` разделяет конвейер на transcriptions, agent responses, synthesis, output device, filler audio и actions. Между этапами используются очереди и отдельные workers, а provider-specific классы скрываются за типизированными конфигурациями.
- `Transcription` содержит сообщение, confidence, `is_final`, `is_interrupt`, признак того, что бот говорил во время распознавания, и длительность. Endpointing поддерживает временной и пунктуационный cutoff; interrupt confidence и mute во время TTS задаются конфигурацией.
- Barge-in реализован через interruptible events: очередь дренируется, output device останавливается, текущие agent/response/action tasks отменяются, а оборванное сообщение остаётся в transcript как неполное. Backchannel и короткие реплики фильтруются политикой чувствительности.
- Synthesis выдаёт аудио чанками; у событий есть обработчики play/interrupt, filler audio и метрики TTFB. Это позволяет отделить генерацию текста от фактического воспроизведения и зафиксировать задержки.
- `ActionConfig` и Pydantic-модели строят JSON-схемы function calls и phrase triggers. Transcript/EventLog публикует human/bot/action start/action finish события и полный transcript по завершении сессии.
- `ExecuteExternalAction` подписывает HTTP payload через HMAC и использует timeout/retries. В repository также есть adapters для Twilio/Vonage, WebSocket, LiveKit и других транспортов, но они не являются частью нужной desktop-границы Евы.
- Worker-слой допускает async и thread-backed обработчики через `janus`. При этом очереди не ограничены, отмена потоков кооперативная, а комментарий в коде отмечает незавершённость контроля concurrency.
- Источник содержит pytest/pytest-asyncio тестовую инфраструктуру, однако исследование ограничилось чтением исходников и compile-only проверкой; `compileall` выявил warning о неэкранированном `\\w`, но синтаксическая ошибка не обнаружена.

#### Что можем использовать в Еве

- **Контракт voice session.** Самостоятельно оформить в Rust Core отдельный lifecycle `capture -> partial/final transcript -> agent -> synthesis -> output -> receipt`, не перенося Python implementation.
- **Типизированные transcript events.** Ввести design-only формы `voice_transcript_delta` и `voice_transcript_final` с confidence, duration, endpoint reason, interruption и revision. Сохранять стабильную финализацию отдельно от промежуточных гипотез.
- **Политику endpointing и barge-in.** Адаптировать time/punctuation endpoint, minimum interrupt confidence, low/high sensitivity и backchannel suppression. Решения должны приниматься Core с bounded таймерами и уже существующими permission/cancellation правилами.
- **Interruptible event model.** Полезны явные `interruptible`, `terminal/uninterruptible`, correlation id и stop token для каждого output/action события. В реализации Евы нужны bounded priority queues, гарантированная отмена и явный `unknown outcome`, если внешний side effect уже начался.
- **Потоковый TTS/output contract.** Использовать аудиочанки, `on_play`/`on_interrupt`, progressive text и TTFB/TTFA/latency metrics для receipts и диагностики. Сырые аудиоданные не сохранять по умолчанию.
- **Typed action schemas.** Взять идею Pydantic/JSON Schema и trigger separation, но расширить каждый action capability, approval requirement, idempotency key, timeout, redacted input/output и receipt. HMAC считать только транспортной аутентификацией, а не разрешением на действие.
- **Event log как observability.** Перенять typed action start/finish и transcript-complete события как append-only redacted события в Core/SQLite; mutable transcript из Vocode не делать источником правды и не писать raw action payload без redaction/retention policy.
- **Provider conformance matrix.** Сохранить единый контракт для текущего локального whisper.cpp и будущих STT/LLM/TTS adapters, добавив fake/offline providers и interruption/reconnect fixtures.
- **Transport adapters как изолированный слой.** WebSocket/LiveKit/telephony можно рассматривать как будущие transport reference, но для Евы UI остаётся через authenticated `desktop-ipc-v1`, а внешняя сеть — отдельной permissioned capability.

#### Ограничения и риски

- **Runtime mismatch и возраст.** Python `asyncio`/threads/`janus` создают второй runtime рядом с Rust Core. Основная ветка имеет последний commit от 2024-11-15 и README просит community maintainers; provider APIs и совместимость нельзя считать актуальными без отдельной проверки.
- **Backpressure и cancellation.** Unbounded queues, cooperative `task.cancel()` и невозможность надёжно остановить thread-backed provider не отвечают требованиям bounded execution. После interrupt внешний provider может продолжить работу, поэтому результат должен быть `unknown`, а не автоматически считаться отменённым.
- **External action surface.** Конфигурация принимает URL и signing secret и делает HTTP POST с retries. Не видны обязательные SSRF allowlist, bounded response, redirect policy, idempotency/replay protection, capability scope, approval и durable receipt. Такой механизм нельзя подключать к Core напрямую.
- **Секреты, сеть и приватность.** Quickstart использует `.env` и ключи облачных STT/LLM/TTS. Transcript и action events могут содержать raw PII и параметры действий; telemetry/Sentry и provider egress требуют явной policy. MIT исходников не покрывает условия моделей, облачных API и телефонных данных.
- **Transcript storage.** Mutable in-memory transcript и полный `TranscriptCompleteEvent` не задают миграций, retention, forget или redaction boundary. В Еве это особенно важно для ambient context: Vocode не является основанием добавлять audio BLOB, voiceprint или raw action logs.
- **Telephony и consent.** Запись звонков, номера, DTMF, transfer и внешние webhook требуют отдельного согласия, privacy review и сетевой политики; для базового desktop продукта это лишняя поверхность.
- **Интеграционные обещания.** Единый adapter API не устраняет различия в streaming, reconnect, rate limits, billing, latency и семантике provider interruption. Каждая интеграция должна проходить собственные contract/e2e tests.

#### Предварительное решение

`адаптировать` lifecycle голосовой сессии, typed contracts, interrupt/endpoint policy, chunked output, action schema и evaluation ideas; `наблюдать` за проектом как reference; `не подключать` Vocode Python SDK, telephony stack и прямой внешний action executor в runtime Евы.

#### Связь с EvoHime

- Текущий Rust listener и ambient storage остаются источником истины. Новые Vocode-подобные события должны проходить через Core, supervisor и authenticated `desktop-ipc-v1`; Electron только отображает redacted state.
- Будущий design-only контракт может использовать события `voice_session`, `voice_transcript_delta`, `voice_transcript_final`, `voice_control`, `voice_output_chunk`, `voice_action_call`, `voice_receipt` и `voice_metric` с sequence/correlation id.
- Для ambient режима сохраняются текущие ограничения: speaker остаётся `unverified`, audio не превращается в persistent SQLite BLOB, а diarization/embeddings не появляются из-за voice-agent orchestration.
- Перед реализацией нужны bounded queues, cancellation/interrupt precedence, approval и idempotency semantics, redaction/retention, offline fake providers, reconnect tests, TTFB/latency metrics и проверка crash/restart с незавершённым action.
- По сравнению с Pipecat Vocode даёт более конкретную семантику одной разговорной сессии и barge-in; Pipecat остаётся более сильным reference для frame/worker/bus orchestration. Оба проекта не следует добавлять в runtime одновременно.

### 17. Voice Lab

- **Источник:** [saharmor/voice-lab](https://github.com/saharmor/voice-lab), [README](https://github.com/saharmor/voice-lab/blob/main/README.md)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `8b89fdf101784dfa116710f58edc988cc7332957`
- **Лицензия исходного кода:** Apache-2.0; в репозитории есть `NOTICE` с требованием attribution/link-back.
- **Состав:** Python-пакеты `core`, `llm_testing`, `speech_testing`, `eval_agent`, `web_eval`, JSON-конфигурации сценариев и метрик, HTML report generation. GitHub показывает 122 commits; последний исследованный commit датирован 2025-05-02.
- **Назначение:** тестирование и оценка voice/LLM agents по моделям, system prompts, personas и заданным критериям. README прямо отмечает, что основной готовый сценарий пока тестирует text-часть voice agent; speech-анализ остаётся экспериментальным направлением.
- **Краткий вывод:** для Евы ценнее всего не код выполнения, а отдельный evaluation layer: versioned scenarios, simulated personas, structured rubric, evidence, model/prompt matrix, cost/latency/quality comparison и пост-фактум speech metrics. Репозиторий не следует добавлять в runtime или делать его LLM judge единственным release gate.

#### Что изучено

- `llm_testing` хранит сценарий в JSON: набор моделей и prompts, initial message, strict function tool `end_conversation`, success criteria, persona, traits, mood, response style и additional context. Это позволяет прогонять один контракт на нескольких конфигурациях и проверять role switching/jailbreak-сценарии.
- `GoalBasedTestRunner` чередует ответ voice agent и ответ симулированного callee-persona, ограничивает историю для persona, поддерживает tool call завершения и передаёт полный conversation history в evaluator.
- `eval_metrics.json` задаёт именованные метрики с типом `success_flag` или `range_score`. `LLMConversationEvaluator` просит structured Pydantic output: summary, score, threshold, reasoning и evidence/quotes по каждому metric.
- В сценариях уже предусмотрены adversarial cases: раздражительный собеседник, недостижимая цена, jailbreak со сменой роли и проверка обязательных подтверждений. Это хороший материал для будущих regression fixtures, но не доказательство безопасности реального действия.
- `speech_testing` экспериментально объединяет pyannote diarization, точные timestamps stable-whisper, word-to-speaker overlap mapping, LLM mapping `SPEAKER_00/01` в `voice_agent/callee`, detection of interruptions и pauses дольше двух секунд.
- Есть альтернативный двухканальный VAD/noise-reduction прототип и `eval_agent` с Gemini bidi WebSocket: PCM 16 kHz input, 24 kHz output, asyncio tasks, audio queue и опциональный interruption во время воспроизведения.
- `core/utils/generate_report.py` строит HTML-отчёт по результатам и показывает conversation history вместе с метриками. Это полезный формат для человека, но строки transcript вставляются в HTML-рендер, поэтому для EvoHime требуется escaping/redaction.
- Проверка `python -m compileall` для checkout прошла без syntax errors. Runtime/import/e2e прогон не выполнялся: speech-модули требуют внешние модели и keys, а в коде видны несовпадающие imports и зависимости, которые не полностью перечислены в requirements.

#### Что можем использовать в Еве

- **Отдельный evaluation artifact.** Оформить независимый от runtime пакет/запуск, где scenario содержит agent contract revision, prompt revision, provider/model revision, persona, bounded context, tool schemas и success criteria. Core не должен зависеть от evaluator для обычного выполнения.
- **Typed metric contract.** Адаптировать поля `metric name`, `output type`, `score`, `success threshold`, `reasoning`, `evidence` и `summary`. Evidence хранить как redacted references/quoted spans с message ids и provenance, а не как безусловно полный transcript.
- **Scenario matrix.** Прогонять одну задачу по нескольким model/prompt/provider configurations и сравнивать quality, latency, token/cost estimate, tool correctness, interruption handling и failure class. Это лучше подходит для выбора поставщика и регрессий, чем субъективный ручной просмотр.
- **Persona simulator.** Использовать typed persona с traits, mood, response style, knowledge/context и adversarial behavior для offline fake conversations. Включить role-switching, jailbreak, ambiguity, silence, refusal, repeated request и early termination.
- **Termination evidence.** Идея строгого `end_conversation` tool полезна как contract test: завершение допускается только при ясном reason, who-ended и последних сообщениях-доказательствах. В Еве это должно быть наблюдением/оценкой, а не полномочием обходить Core approval.
- **Speech evaluation metrics.** Адаптировать bounded `speaker_span`, overlap/interruption span, pause duration, ASR confidence и turn latency для анализа уже разрешённого transcript/audio buffer. Нельзя из LLM speaker mapping выводить личность; неизвестный speaker остаётся unknown/unverified.
- **Replay and report bundle.** Генерировать отчёт с scenario/config/metric files, revision hashes, transcript hash, model/provider revisions, score/evidence и failed fixture ids. Отчёт должен быть безопасно экранирован, redacted и воспроизводим без доступа к secrets.
- **LLM-as-a-Judge как advisory signal.** Использовать structured judge для triage и сравнений, с фиксированными rubric/thresholds, повторными прогонами и human review для спорных случаев. Hard security/approval invariants проверять детерминированными assertions в Core.
- **Attribution discipline.** Если будут заимствоваться отдельные Apache-2.0 элементы, сохранить LICENSE/NOTICE и attribution; предпочтительнее перенять форматы и идеи, написав реализацию в Rust/TypeScript самостоятельно.

#### Ограничения и риски

- **Не runtime.** Репозиторий Python-script-first, без стабильного сервиса, schema migration, bounded job manager, durable result store или Core security boundary. Встраивание создаст второй execution/evaluation runtime и не нужно для desktop-продукта.
- **LLM judge не является oracle.** Оценщик может быть необъективен, недетерминирован, подвержен prompt injection из transcript и совпадать с тестируемой моделью. `reasoning`/`evidence` — полезные объяснения, но не криптографическое доказательство и не единственный критерий релиза.
- **Секреты и PII.** Примеры содержат synthetic credit-card context, а запуск использует `OPENAI_API_KEY`, `GEMINI_API_KEY` и `HUGGING_FACE_TOKEN`. Transcript, persona context и HTML reports могут содержать PII; в Еве нужны fake secrets, redaction, retention и default-deny egress.
- **Экспериментальная speech-часть.** `speech_testing/transcribe.py` импортирует `faster_whisper` и символ `REQUIRED_AUDIO_TYPE`, не согласованный с локальным `data_types.py`; root requirements не покрывает все используемые `pydub`, `webrtcvad`, `noisereduce` и faster-whisper imports. Compile-only успех не означает исполнимость.
- **Diarization and identity.** Pyannote требует gated model/token и тяжёлый Python/PyTorch runtime. LLM mapping anonymous speaker labels по задаче и transcript ошибается при трёх участниках, overlap и role ambiguity; это не identity verification и не должно менять ASR text.
- **Cloud egress.** Gemini bidi example отправляет microphone PCM в Google, а Silero VAD загружается через `torch.hub`; OpenAI judge и HF diarization также требуют внешние сервисы/модели. Для local-first Евы это только permissioned evaluation job, не обычный listener path.
- **Realtime prototype limitations.** Audio queue не ограничена, cancellation/cleanup кооперативны, `config=None` конфликтует с последующим `config.get`, а обработка interruptions выбирает между пропуском input и попыткой interrupt. Эти решения нельзя принимать как Core guarantees.
- **Report safety.** HTML generator вставляет данные conversation history в markup; если результат содержит `<script>` или служебные секреты, отчёт может стать XSS/PII каналом. Нужны escaping, CSP, redaction и безопасное открытие локального файла.
- **Speech metric quality.** Простое пересечение временных сегментов чувствительно к ошибкам diarization, VAD, clock drift и неравномерным timestamps; pause threshold в две секунды — heuristic, а не универсальная UX-норма. Нужны fixtures, uncertainty и dataset-specific calibration.
- **Test quality/maintenance.** README и code paths выглядят как исследовательский prototype: standalone eval agent помечен coming soon, часть contribution items не реализована, а актуальность провайдерских SDK и моделей требует отдельной проверки.

#### Предварительное решение

`адаптировать` evaluation artifacts, scenario/persona schema, metric/evidence contract, model/prompt matrix, deterministic assertions и safe report provenance; `наблюдать` за speech/eval-agent частями; `не подключать` Python Voice Lab, LLM judge как единственный gate, cloud bidi runtime и pyannote pipeline в базовый runtime Евы.

#### Связь с EvoHime

- Evaluation должен быть отдельным offline/CI job поверх Core IPC/test-agent harness и не переносить runtime state из Rust Core в Electron. Его входом могут быть redacted transcript events, tool receipts и voice metrics, а не сырые secrets/audio по умолчанию.
- Для текущего listener полезны fixtures с partial/final ASR, overlap, interruption, pause, unknown speaker и cancellation. Они должны уважать существующее правило ambient storage: speaker остаётся `unverified`, persistent audio BLOB не добавляется.
- Будущий формат может содержать `eval_run`, `eval_scenario`, `eval_metric_result`, `eval_evidence_ref`, `eval_model_revision`, `eval_failure_class` и `eval_report_manifest` с hash/provenance/retention.
- Перед реализацией нужны deterministic Core assertions для approvals, capability boundaries, tool schemas, receipts и redaction; только после них — advisory LLM judge и human review. Критерии: повторяемость, bounded time/cost, отсутствие egress по умолчанию, безопасный отчёт и сравнение моделей по единому сценарию.
- По отношению к уже изученному Vocode Voice Lab полезен как внешний evaluation layer, а не как voice orchestration. Его speech metrics дополняют Pipecat/Vocode lifecycle ideas, но не должны добавлять в runtime ещё один Python audio stack.

### 18. Qwen2-VL

- **Источник:** [коллекция Qwen2-VL](https://huggingface.co/collections/Qwen/qwen2-vl), модели [2B-Instruct](https://huggingface.co/Qwen/Qwen2-VL-2B-Instruct), [7B-Instruct](https://huggingface.co/Qwen/Qwen2-VL-7B-Instruct), [2B-Instruct-AWQ](https://huggingface.co/Qwen/Qwen2-VL-2B-Instruct-AWQ), [официальный обзор Qwen](https://qwenlm.github.io/blog/qwen2-vl/)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** у коллекции нет одной ревизии; проверены актуальные карточки моделей. Представительные SHA: 2B-Instruct `895c3a49bc3fa70a340399125c650a463535e71c`, 7B-Instruct `eed13092ef92e448dd6875b2a00151bd3f7db0ac`, 2B-Instruct-AWQ `4f6ea6d22fcf0f8c1ed64d1d2a3d722d4d7bbcea`.
- **Лицензия:** карточки open-weight моделей и quantized variants заявляют Apache-2.0. Для каждой конкретной модели, конвертации и runtime всё равно нужен отдельный component/license manifest; лицензия весов не подтверждает права на пользовательские изображения, видео или данные из них.
- **Состав:** семейство base/instruct моделей на 2B, 7B и 72B параметров, а также AWQ/GPTQ 4/8-bit варианты. 2B и 7B доступны как локальные модели; 72B и её варианты требуют отдельного инфраструктурного решения.
- **Назначение:** image-text-to-text и multimodal conversational inference: понимание изображений разного разрешения, нескольких изображений, видео, текста внутри документов, визуальный вопрос-ответ и visual reasoning.
- **Краткий вывод:** Qwen2-VL может стать optional perception backend Евы для разрешённых screenshots, документов и коротких video clips. Перенимать следует контракты multimodal input, visual budget, timestamp/frame provenance и evaluation; Python/PyTorch/vLLM runtime, continuous capture и автоматическое выполнение visual-agent команд пока не подключать.

#### Что изучено

- Модельная архитектура использует dynamic resolution: визуальный input преобразуется в переменное число visual tokens вместо обязательного фиксированного размера. M-ROPE разделяет текстовые, пространственные и временные позиции для текста, изображений и видео.
- Карточка заявляет понимание изображений произвольного aspect ratio, multilingual text in images, OCR/document tasks, visual QA и видео длительностью более 20 минут. Это заявленные возможности, которые для русских документов и реальных Windows screenshots нужно проверять отдельными fixtures.
- Формат запроса — структурированный multimodal chat: сообщения содержат text, image, несколько images или video. `qwen-vl-utils` обрабатывает URL/base64/local file и interleaved image/video inputs.
- Processor позволяет ограничивать `min_pixels`/`max_pixels`, задавать точные размеры, а для видео — sampling FPS/число кадров и общий визуальный бюджет. Для Qwen2-VL базовый resize factor связан с 28-пиксельной сеткой.
- Официальный путь inference — Transformers с `Qwen2VLForConditionalGeneration`/`AutoProcessor`; карточка предупреждает, что для старых Transformers может потребоваться установка исходников, иначе возникает `KeyError: 'qwen2_vl'`.
- Также показаны vLLM и SGLang с OpenAI-compatible HTTP API, Docker Model Runner и ссылки на quantizations для local apps. Это варианты отдельного model worker, а не библиотека для прямого встраивания в Rust Core.
- Коллекция содержит base/instruct и quantized 2B/7B/72B варианты. У 2B-Instruct-AWQ конфигурация 4-bit явно оставляет visual module в `modules_to_not_convert`, поэтому номинальные 4-bit параметры не равны полной 4-bit экономии памяти.
- По текущим карточкам 2B-Instruct имеет примерно 2.2B BF16 параметров, 7B-Instruct — примерно 8.3B; model files и runtime memory существенно больше одного удобного числа параметров. 2B quantized вариант заметно легче, но всё равно требует проверки GPU/CPU latency.
- Qwen2-VL — vision-language модель: аудио/STT/TTS в этой коллекции нет. Её нельзя считать заменой текущего listener, Whisper, Moshi или voice-agent pipeline.

#### Что можем использовать в Еве

- **Optional visual perception contract.** Ввести design-only `vision_request`/`vision_result` с `image_ref`/`video_ref`, text prompt, purpose, model revision, processor revision, pixel/frame budget, timestamps и redacted evidence. Передавать raw bytes через Electron не нужно; Core должен владеть временным input handle.
- **Bounded visual budget.** Перенять явные `min_pixels`, `max_pixels`, exact resize, FPS/frames и total budget. Для каждой задачи заранее задавать лимиты времени, размера, кадров, output tokens и памяти; не позволять модели динамически расширять capture scope.
- **Document/OCR enrichment.** Рассматривать 2B/7B как optional offline job для разрешённого изображения документа, screenshot или короткого clip: извлечение текста, таблиц, визуальных фактов и ссылок на region/frame. Результат должен быть evidence-bearing и отделён от authoritative tool state.
- **Visual grounding schema.** Просить структурированный ответ с объектом, region/frame/time span, confidence и uncertainty, затем валидировать координаты и схему в Core. Текст на картинке и модельные инструкции считать untrusted input, а не capability grant.
- **Model ladder.** Исследовать 2B quantized как быстрый PoC, 2B/7B Instruct как quality comparison, а 72B оставить только для удалённого или отдельного GPU benchmark. Выбор делать по русским OCR, screenshot understanding, latency и memory, а не по общим benchmark claims.
- **Visual context для RAG.** Сохранять при необходимости redacted caption/OCR/structured facts и content hash с provenance; не превращать исходные screenshots/video frames в постоянную SQLite knowledge base без отдельного opt-in retention policy.
- **Provider/model manifest.** Зафиксировать model SHA, quantization, tokenizer/processor, Transformers/qwen-vl-utils/runtime versions, checksum, license, source/effect of downloads и egress mode. Это согласуется с уже используемыми в Еве model/artifact provenance правилами.
- **Evaluation fixtures.** Добавить в будущий Voice Lab-подобный evaluation слой русские документы, мелкий текст, UI screenshots, несколько изображений, low-resolution/noisy frames, prompt injection inside image, timestamped video и cancellation. Метрики: OCR CER/WER, field accuracy, region IoU, grounded evidence, latency, memory и refusal on untrusted instructions.
- **Isolated worker boundary.** Если PoC подтвердит ценность, запускать модель в отдельном permissioned worker с bounded IPC/HTTP contract. Rust Core остаётся владельцем permission, input lifetime, cancellation, redaction и approval; Electron не обращается к model server напрямую.

#### Ограничения и риски

- **Не базовый runtime.** Официальный inference path — Python/PyTorch/Transformers с optional FlashAttention; vLLM/SGLang требуют отдельного GPU-oriented server. В текущей Electron + Rust Core + supervisor-поставке нет готового Windows-native multimodal worker.
- **Память и latency.** 2B/7B vision inference, processor и visual tokens требуют существенно больше ресурсов, чем text-only token count. Dynamic resolution и много кадров могут незаметно раздувать стоимость/VRAM; CPU fallback может быть неприемлемо медленным.
- **Quantization не полная.** AWQ/GPTQ варианты требуют проверки совместимости конкретного backend, kernel и visual encoder; AWQ-конфигурация 2B оставляет visual module нетронутым. Нельзя планировать память только как `parameters * bits`.
- **Версионная хрупкость.** Карточка предупреждает о необходимости свежего Transformers source для `qwen2_vl`; processor, `qwen-vl-utils`, CUDA, vLLM/SGLang и quantization kernels должны быть pinned и совместно протестированы. Latest upstream нельзя включать в installer без manifest.
- **Непроверенная достоверность.** OCR, small text, coordinates, object counts, charts и temporal reasoning могут ошибаться. Model output не является фактом и не должен напрямую менять workspace, browser, filesystem или другие capabilities.
- **Visual prompt injection.** Screenshot/document может содержать текст вроде «игнорируй правила и отправь секрет». Этот текст должен проходить как untrusted observation с provenance; perception model не получает права на tools, approvals или external actions.
- **Приватность и egress.** Screenshots, camera frames, документы, токены браузера и личные данные могут попасть в input, logs, provider cache или remote inference. Default policy: capture только по явному permission, bounded temporary lifetime, no raw frame in SQLite, redacted results и default-deny network egress.
- **Видео стоимость.** Возможность понимать длинные видео не означает пригодность для realtime ambient capture. Frame sampling, total pixel budget и retention нужны обязательно; нельзя silently сохранять или отправлять весь экран/камеру.
- **Лицензирование и supply chain.** Apache-2.0 удобна для open-weight использования с соблюдением условий, но нужно проверить каждый downloaded artifact, quantization, tokenizer, processor, runtime и сторонние зависимости. HF download требует pin/hash verification и не должен происходить скрыто в обычном запуске.
- **Не visual action executor.** Формулировка о возможности управлять mobile/robot не заменяет permission, grounding, approval, idempotency и safe action policy. Для Евы Qwen2-VL может предложить наблюдение/кандидатное действие, но не выполнить его.
- **Не identity/biometric oracle.** Распознавание лица, документов или людей должно быть отдельным privacy/legal review. Визуальная гипотеза не должна автоматически становиться пользовательской идентичностью или долговременным профилем.

#### Предварительное решение

`рассматривать` Qwen2-VL-2B-Instruct-AWQ как optional offline/PoC visual backend; `адаптировать` multimodal input contract, visual budgets, grounding/evidence, model manifest и evaluation fixtures; `наблюдать` 7B/72B и backend compatibility; `не подключать` Python model stack, continuous screen/camera capture и visual-agent actions в базовый runtime Евы.

#### Связь с EvoHime

- Будущий `vision.*` слой должен идти через Rust Core и supervisor с capability/approval policy. Electron может отображать redacted result, но не хранит модельные веса, raw screenshot state или provider secrets и не вызывает vLLM/SGLang напрямую.
- Для интеграции нужен bounded temporary input store или handle-based IPC, size/format validation, image/video TTL, cancellation, sequence/correlation id, model provenance и redaction. Existing authenticated `desktop-ipc-v1` остаётся границей UI.
- Ambient listener не расширять до скрытого screen/camera capture. Как и для текущего audio storage, raw visual input не добавлять автоматически в SQLite; persistent OCR/caption требует отдельного consent/retention/forget дизайна.
- Наиболее реалистичный порядок: сначала design-only schema и offline fixtures, затем worker PoC на 2B quantized, затем benchmark против 7B/remote provider, и только после этого решение о packaging/GPU support. Acceptance criteria: no secret/image egress by default, bounded memory/time, cancellation, reproducible model hashes, safe prompt-injection behavior и Russian visual QA metrics.
- Qwen2-VL дополняет уже исследованные browser/computer-use и Voice Lab идеи как perception backend. Он не заменяет Playwright/Puppeteer action boundary, Vocode/Pipecat voice orchestration, Whisper ASR или текущий Rust listener.

### 19. mPLUG/DocOwl2

- **Источник:** [модель mPLUG/DocOwl2](https://huggingface.co/mPLUG/DocOwl2), [официальный mPLUG-DocOwl repository](https://github.com/X-PLUG/mPLUG-DocOwl), [README модели](https://huggingface.co/mPLUG/DocOwl2/blob/main/README.md), [техническая работа](https://arxiv.org/abs/2409.03420)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** HF model `205b9e18b0cb503c9ef0dde1e7b120e6925778d9`; связанный source checkout `f91a76859babfdebe7420db6133b66f06f65ecf2`.
- **Лицензия:** Apache-2.0 для модели/source repository. Training datasets, upstream libraries, custom kernels и пользовательские документы требуют отдельного component/data/license review.
- **Состав:** custom Transformers model code, visual encoder, high-resolution `DocCompressor`, processor, tokenizer и evaluation scripts. HF карточка помечает модель как `custom_code`; размер — около 9B параметров в BF16, model storage около 17.1 GB.
- **Назначение:** OCR-free понимание многостраничных документов: text lookup/parsing, concise/detailed multi-page VQA, page evidence, cross-page structure и text-rich video understanding.
- **Краткий вывод:** DocOwl2 сильнее и точнее сфокусирован на document worker, чем универсальный Qwen2-VL. Для Евы полезны page-aware input, evidence pages, bounded document budgets и benchmark contracts. Сам 9B BF16 Python/CUDA runtime слишком тяжёлый и небезопасный для прямого включения в Rust Core/Electron package.

#### Что изучено

- Главная идея — high-resolution `DocCompressor`: каждая страница кодируется примерно в 324 visual tokens, что уменьшает цену multi-page context и позволяет отвечать с учётом нескольких страниц.
- README выделяет multi-page text lookup/parsing, короткие и подробные ответы с evidence pages, а также text-rich video. Пример передаёт список page images и query; вопрос может ссылаться на конкретную страницу или документ целиком.
- HF quickstart использует `AutoTokenizer(use_fast=False)`, `AutoModel.from_pretrained(..., trust_remote_code=True, low_cpu_mem_usage=True, torch_dtype=torch.float16, device_map='auto')`, затем `init_processor(tokenizer, basic_image_size=504, crop_anchors='grid_12')` и `model.chat(...)`.
- Custom `processor.py` принимает локальные image paths/PIL images, конвертирует RGB, строит high-resolution crops по anchor grid и добавляет page/image ordinal tokens в текстовый prompt. Порядок страниц кодируется самим списком входов.
- `visual_compressor.py` использует FlashAttention для cross-attention; код требует CUDA tensors и float16/bfloat16. Это не portable CPU/Windows Rust implementation.
- В репозитории опубликованы MP-DocStruct1M, MP-DocReason51K, DocDownstream-2.0 и DocGenome12K, а evaluation покрывает single-image задачи, MP-DocVQA, DUDE и NewsVideoQA. Данные и их лицензии не следует автоматически считать частью лицензии модели.
- Benchmark scripts сохраняют JSONL predictions и считают ANLS, exact/relaxed/contain accuracy, IoU, BLEU, ROUGE, METEOR и CIDEr. Это полезный reference для document-specific evaluation, но не готовая production quality gate.
- HF model card показывает 9B BF16 weights и отсутствие Inference Provider deployment. Значит, для использования нужен собственный local worker/GPU или отдельно организованный provider, а не готовый HF endpoint.
- Код processor выставляет `ImageFile.LOAD_TRUNCATED_IMAGES = True` и `Image.MAX_IMAGE_PIXELS = None`. Для продукта это опасные defaults: до вызова модели Core обязан валидировать формат, размеры, pixel count, decode time и временное хранение.
- `compileall` для checkout evaluation/source paths прошёл без syntax errors. Это не подтверждает работоспособность inference: необходимы CUDA, FlashAttention, PyTorch/Transformers compatibility, веса и все evaluation dependencies.

#### Что можем использовать в Еве

- **Специализированный document backend.** В архитектуре разделить `vision_image`/`vision_video` и `document_multipage` capabilities. Для многостраничного PDF/скана выбирать DocOwl-подобный worker, а не заставлять универсальную VLM обрабатывать страницы без page-aware контракта.
- **Page manifest.** Ввести `document_id`, page number/order, source path scope, content hash, dimensions, orientation, render revision и retention. Ответы должны ссылаться на `evidence_pages`/`evidence_regions`, а не только выдавать свободный текст.
- **Bounded document budget.** Перенять идею фиксированного page compression, но задать Core limits: максимальный размер документа, число страниц, pixel budget на страницу, суммарные visual tokens, время job, output tokens и memory estimate. Не передавать модели произвольный весь workspace.
- **Typed query modes.** Разделить `lookup`, `extract_fields`, `summarize`, `compare_pages`, `answer_with_evidence` и `table_read`. Для каждого режима определить schema, required evidence и допустимое `uncertain/needs_review` состояние.
- **Evidence-first output.** Результат должен содержать answer, page references, optional region/time spans, confidence/uncertainty, model revision и input hash. Evidence — ссылка на исходную страницу/фрагмент, а не утверждение модели о самом себе.
- **OCR-free как enrichment/fallback.** Использовать визуальное понимание для сложных таблиц, layout, диаграмм и сканов после обычного text extraction/OCR, а не удалять проверенный текстовый слой. Конфликт между parser/OCR и VLM должен идти в review/uncertainty, не молча перезаписывать источник.
- **Document RAG integration.** В существующий local Agentic RAG добавлять redacted page summaries, extracted fields и page citations с content hash. Исходные images/PDFs остаются в исходной permissioned workspace scope и не копируются в SQLite без отдельной политики.
- **Evaluation contracts.** Адаптировать ANLS для OCR-like answers, exact/relaxed accuracy для полей и чисел, IoU для region grounding и cross-page fixtures. Добавить русские технические manuals, таблицы, чертежи, mixed orientation, small text, page references и adversarial instructions inside documents.
- **Model manifest/custom code review.** Для любого PoC фиксировать HF SHA, source commit, custom-code files, tokenizer/processor, PyTorch/Transformers/FlashAttention versions, checksum, memory profile, license и egress. `trust_remote_code` разрешать только на pinned, reviewed artifact.
- **Isolated worker boundary.** Если PoC успешен, запускать документный worker отдельно под supervisor/permissioned capability. Core владеет page render, file access, cancellation, retention, redaction и receipt; Electron видит только redacted progress/result.

#### Ограничения и риски

- **Тяжёлый runtime.** Около 9B BF16 параметров и примерно 17.1 GB model storage требуют отдельной GPU policy; веса, activation memory, high-resolution crops и FlashAttention workspace увеличивают реальное потребление. Для обычной Windows desktop поставки это неприемлемая базовая зависимость.
- **CUDA/FlashAttention dependency.** Custom compressor импортирует `flash_attn` и проверяет CUDA/half tensors. Нет подтверждённого Rust/Candle/Windows-native backend и нет HF Inference Provider; fallback CPU не является поддержанным production path.
- **Remote custom code.** `trust_remote_code=True` запускает Python code из model repository. Нужны pinned revision, code review, offline packaging, allowlist imports и запрет автоматического обновления весов/процессора при старте.
- **Processor safety defaults.** Разрешение truncated images и отключение `MAX_IMAGE_PIXELS` может открыть decompression bomb, memory exhaustion или очень долгий decode. Core должен отбраковывать неподходящие inputs до custom processor, а worker — иметь memory/time watchdog.
- **Качество и hallucination.** OCR-free модель может неправильно прочитать мелкий текст, таблицу, формулу, номер страницы или cross-page relation. Generated evidence page не доказывает ответ; проверять нужно исходный render и, где возможно, deterministic text extraction.
- **Язык и домен.** HF card помечает язык как English, тогда как Еве нужны русские документы, технические manuals и mixed-language scans. Нужен собственный benchmark; нельзя переносить paper metrics на русские документы без измерения.
- **Данные и приватность.** Документы могут содержать персональные данные, ключи, финансовые сведения и внутренние инструкции. Не отправлять их в remote provider по умолчанию, не писать raw pages/model prompts в логи, ограничить TTL и поддержать forget/erase для derived summaries.
- **Prompt injection in documents.** Текст страницы может содержать инструкции «игнорируй правила», фальшивые approvals или tool commands. DocOwl получает только capability `document.read`; его вывод никогда не получает право менять workspace, browser, provider или secrets.
- **Page explosion/DoS.** Multi-page compression снижает token cost, но не отменяет decode/render/GPU cost. Ограничить number of pages, total pixels, crop count, concurrent jobs, queue size и cancellation at every page.
- **License boundary.** Apache-2.0 модели не покрывает автоматически MP-Doc datasets, training data, tokenizer/runtime dependencies, FlashAttention или документы пользователя. Для redistribution нужен полный manifest и attribution/license bundle.
- **Evaluation leakage.** Benchmark answers и datasets могут попасть в local cache или reports; не смешивать их с пользовательскими workspace facts и не считать benchmark score evidence of safety or authorization.

#### Предварительное решение

`рассматривать` DocOwl2 как optional offline document-worker PoC; `адаптировать` page manifest, evidence contract, document budgets, deterministic fallback и evaluation metrics; `наблюдать` за quantized/portable successors; `не подключать` 9B BF16 custom Python/CUDA runtime и `trust_remote_code` в базовый runtime Евы.

#### Связь с EvoHime

- Будущий `document.*` capability должен идти через Rust Core: permissioned path scope, render/parse limits, temporary page handles, cancellation, redaction, retention и durable receipt. Electron не читает workspace напрямую и не запускает model code.
- Для Local Agentic RAG DocOwl2 может быть optional enrichment для page-aware citations, но не заменяет SQLite/FTS5 source-of-truth и не должен записывать raw document images как BLOB.
- Возможные design-only события: `document_ingest_started`, `document_page_ready`, `document_query`, `document_evidence`, `document_uncertain`, `document_job_receipt` с sequence/correlation id, source hash и model revision.
- Реалистичный порядок: schema/fixtures и deterministic text extraction → isolated worker PoC → benchmark на русских manuals → memory/latency/security review → решение о model/backend/package. Acceptance criteria: bounded pages/pixels/memory/time, no unapproved egress, pinned custom code, safe prompt-injection handling, evidence page accuracy, cancellation и erase derived data.
- DocOwl2 дополняет Qwen2-VL: Qwen2-VL подходит как универсальный image/video perception backend, DocOwl2 — как более узкий multi-page document backend. Оба не должны добавляться одновременно в базовую установку без capability routing и общего model manifest.

### 20. Mem0

- **Источник:** [mem0ai/mem0](https://github.com/mem0ai/mem0), [README](https://github.com/mem0ai/mem0/blob/main/README.md), [документация](https://docs.mem0.ai/), [исследовательская работа](https://arxiv.org/abs/2504.19413)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `feb12852c0789a1f1182b05ee0dbc386037b012f`; package version `2.0.18`; GitHub показывает 2,604 commits на момент проверки.
- **Лицензия исходного кода:** Apache-2.0. В репозитории есть Python SDK, npm/TypeScript SDK, self-hosted server и cloud/platform integrations; лицензирование сервиса, моделей, vector stores и данных нужно рассматривать отдельно.
- **Состав:** `Memory`/`AsyncMemory`, LLM/embedding/vector-store/reranker factories, SQLite history/messages, optional spaCy NLP, entity store, hybrid semantic/BM25/entity ranking, CLI, server и многочисленные backend adapters.
- **Назначение:** долгосрочная память AI assistant/agent: extraction фактов из диалога, поиск релевантных memories, user/session/agent state, procedural memory, history, update/delete/expiration и multi-backend storage.
- **Краткий вывод:** Mem0 даёт хороший reference для жизненного цикла memory, scope и retrieval, но не должен становиться второй базой знаний Евы. Уже реализованный Local Agentic RAG/SQLite Core-first слой остаётся источником истины; из Mem0 нужно перенять контракты и проверки, а не Python SDK, cloud backend или автоматическое запоминание всего transcript.

#### Что изучено

- Основной API разделяет `add`, `search`, `get`, `get_all`, `update`, `delete`, `delete_all`, `history`, `reset` и async-варианты. `add` принимает `user_id`, `agent_id` или `run_id`; retrieval использует явные `filters`, а отсутствие scope отклоняется.
- `MemoryConfig` собирает LLM, embedder, vector store, optional reranker, `history_db_path`, version и custom extraction instructions. В README default LLM — OpenAI `gpt-5-mini`, default embedding — `text-embedding-3-small`, а для hybrid search предлагается Qwen embedding или comparable model.
- Current README описывает новый April 2026 algorithm: single-pass ADD-only extraction без LLM UPDATE/DELETE, first-class agent-generated facts, entity linking, semantic+BM25+entity retrieval и temporal reasoning. README прямо предупреждает, что benchmark scores включают proprietary optimizations managed platform и не равны гарантии OSS SDK.
- OSS-код сохраняет явные API `update`/`delete`, а extraction prompt требует ADD-only self-contained factual memories с `attributed_to`, `linked_memory_ids`, summary, recently extracted/existing memories, observation date и custom instructions. Это важное разделение: факт можно добавить, а ревизию/удаление должны контролировать отдельные операции.
- Memory payload содержит текст, MD5 hash, metadata, `created_at`, `updated_at`, optional `expiration_date`, `memory_type`, lemmatized text и scope. SQLite history хранит old/new memory, event ADD/UPDATE/DELETE, actor/role, timestamps и deletion flag; отдельная messages table ограничивает последние 10 сообщений на session scope.
- Retrieval сначала использует vector store, затем может добавлять normalized BM25 score, entity boost и optional reranker. Threshold применяется до hybrid scoring, есть `top_k`, metadata filter operators и `explain` с деталями semantic/BM25/entity scores.
- Entity linking — не отдельный graph database. Опциональный spaCy extraction выделяет `PROPER`, `QUOTED`, `TOPIC`, `IDENTIFIER`, entity rows хранят `linked_memory_ids`, а поиск сущностей повышает связанные memories. В текущем OSS checkout отдельного graph-memory модуля не найдено.
- Scope helpers валидируют и trim entity IDs, запрещают подменять `user_id`/`agent_id`/`run_id` через свободную metadata и требуют фильтры для `get_all`/`search`/`delete_all`. Это полезная защита от случайного cross-scope retrieval, но не замена полноценной Core ACL/capability policy.
- Expiration скрывает истёкшие memories из обычного search/get_all; explicit `delete` пишет tombstone в history, удаляет vector и чистит linked entities. `delete_all` обрабатывает batches до 1000 и требует хотя бы одного scope filter; `reset` удаляет всю коллекцию и SQLite history.
- Telemetry в OSS включена по умолчанию через `MEM0_TELEMETRY=True`: используется PostHog, anonymous/user identifier и lifecycle/hot-path events с sampling. Код redacts известные secret fields, но сама default network egress policy не соответствует local-first требованиям Евы без явного отключения/consent.
- Test suite покрывает memory, API, integration, vector stores, LLMs, auth и telemetry. `python -m compileall` для checkout прошёл; полноценные тесты и provider integrations в этом исследовании не запускались.

#### Что можем использовать в Еве

- **Memory lifecycle contract.** Перенять явное разделение `propose/add`, `search`, `get`, `revise/supersede`, `forget`, `history` и `reset`, но реализовать его в Rust Core и SQLite schema v25+ с transaction/backup guarantees.
- **Scope model.** Идею `user_id`/`agent_id`/`run_id` расширить до `workspace_id`, `profile_id`, `chat_id`, `repository_id`, `source_event_id` и capability scope. Scope должен быть установлен Core из authenticated context, а не доверяться полям, пришедшим от LLM или renderer.
- **Additive extraction.** Перенять ADD-only candidate extraction: LLM предлагает self-contained fact, attribution, source span, observation time, sensitivity и links; отдельная deterministic policy решает, можно ли сохранить факт. Это безопаснее, чем разрешать LLM молча переписывать или удалять память.
- **Revision semantics.** Вместо накопления конфликтующих ADD facts ввести `supersedes`, `valid_from`, `valid_to`, `confidence`, `status=proposed/accepted/superseded/forgotten` и human/Core review. Старый факт не уничтожать до успешной транзакции и сохранять provenance.
- **Hybrid retrieval.** Объединить текущий SQLite FTS5 и semantic retrieval с нормализованным score, optional reranker/entity boost и explain details. FTS5 остаётся fallback, а retrieval result обязан содержать citations/source event ids и reason for inclusion.
- **Entity linking как enrichment.** Перенять typed entity candidates и linked-memory index как optional boost, но не делать entity label идентичностью. Для русскоязычного EvoHime нужны собственные tokenizer/lemmatization fixtures и защита от ложных fuzzy links.
- **Temporal retrieval.** Использовать observation time, created/updated/valid/expiration timestamps для запросов «сейчас», «раньше» и «на дату». В Core хранить timezone/precision/source of time и не подменять фактическую дату временем индексации.
- **Expiration/forget.** Сочетать expiration, per-source retention, user-visible forget и tombstone. Критерий завершения forget должен включать primary memory, history policy, entity links, embeddings, caches, derived summaries и export/backup behavior.
- **Procedural memory boundary.** Идею memory type для agent workflow можно адаптировать для одобренных процедур/предпочтений Евы, но procedural memory не должна становиться скрытой инструкцией с правами. Любой capability-affecting fact проходит policy/approval и provenance.
- **Provider adapters without provider coupling.** Использовать Mem0 как reference для LLM/embedder/vector-store factories и fake/mock providers в тестах, но оставить Core-owned Rust implementations/IPC. Remote OpenAI/Qdrant/managed Mem0 — только explicit provider capability.
- **Memory evaluation.** Перенять классы benchmark queries LoCoMo/LongMemEval/BEAM: factual recall, temporal selection, entity linking, conflict resolution, stale-memory suppression, long-context token budget и scale. Добавить русские fixtures, security leakage, forget completeness и crash/restart.
- **Explainable result.** `score_details` и history reference полезны для UI/debug, но наружу выдавать только redacted citations, confidence/uncertainty и policy decision. Не показывать raw hidden prompt, provider secret или весь vector payload без permission.

#### Ограничения и риски

- **Второй runtime и datastore.** Python `mem0ai` тянет LLM, embedding, vector-store и optional NLP ecosystems; прямое подключение создаст вторую память рядом с Rust Core/SQLite RAG и размоет source of truth. npm SDK не меняет это архитектурное ограничение.
- **Cloud defaults и telemetry.** Quickstart отправляет conversation в OpenAI LLM/embedding, а OSS telemetry по умолчанию включает PostHog egress. Локальный provider можно настроить, но default конфигурация неприемлема для sensitive ambient/workspace data.
- **Сильные benchmark claims ограничены.** README указывает, что новые цифры managed platform содержат proprietary optimizations; их нельзя использовать как подтверждённую характеристику self-hosted OSS и нельзя переносить в acceptance criteria без собственного прогона.
- **Raw PII persistence.** Vector payload и SQLite history хранят исходные memory strings, old/new revisions и actor/role. Нет автоматического field-level encryption/redaction/consent policy, совместимой с ЕvoHime. Memory extraction может захватить секреты, финансовые данные, credentials или приватный ambient transcript.
- **Extraction is not truth.** LLM prompt старается извлекать всё, включая assistant recommendations и shared documents; он может hallucinate, misattribute, сохранить prompt injection или принять временное желание за постоянную preference. Нужны deterministic filters, sensitivity classifier, source evidence и review.
- **ADD-only accumulation.** Новая algorithmic семантика не обновляет/удаляет автоматически и может накапливать stale/conflicting facts. Явные update/delete API существуют, но конфликт между extraction и lifecycle должен быть разрешён продуктовым контрактом, а не случайным порядком вызовов.
- **Deletion atomicity и cleanup.** Vector store, SQLite history и entity store — отдельные операции; entity cleanup ошибки намеренно не ломают primary delete/update. Это повышает живучесть, но forget нельзя считать завершённым без reconciliation job и проверяемого receipt.
- **Scope is not ACL.** `user_id`/`agent_id`/`run_id` защищают фильтры от части ошибок, но не моделируют workspace ownership, role, capability, export restrictions, encryption key или cross-agent policy. Нельзя принимать caller-supplied metadata как authorization.
- **Entity false links.** Semantic entity matching и linked memory IDs могут связать похожие имена/проекты и усилить неправильный факт. Entity store также может сохранять PII отдельно от основной memory; erase/reconciliation должен учитывать обе копии.
- **Language mismatch.** README NLP extra предлагает `en_core_web_sm`; BM25 lemmatization/entity extraction не гарантируют русскую морфологию. Для русского языка нужна отдельная evaluation matrix, а FTS5/tokenization fallback должен оставаться рабочим.
- **Temporal limitations.** В OSS `add(timestamp=...)` помечен как platform-only/unsupported, хотя prompts и metadata умеют observation/expiration. Для Евы временная семантика должна быть реализована локально и транзакционно, без зависимости от managed platform.
- **Operational consistency.** Vector insert/update/delete и SQLite history не образуют единую транзакцию. При сбое после одной операции возможны orphan vectors, missing history или stale entity links; нужны WAL/transaction journal/reconciliation в Core.
- **License and service boundary.** Apache-2.0 разрешает адаптацию с соблюдением условий, но не лицензирует Mem0 cloud, LLM providers, embedding models, vector databases или user data. Self-host server auth — отдельная поверхность, cloud API и agent signup для Евы не нужны.

#### Предварительное решение

`адаптировать` scope model, additive candidate extraction, hybrid retrieval, temporal/expiration/forget contracts, entity-link enrichment, provenance и evaluation ideas; `наблюдать` за Mem0 algorithm/platform evolution; `не подключать` Python/npm SDK, managed Mem0, default PostHog telemetry и raw automatic memory writes в базовый runtime Евы.

#### Связь с EvoHime

- Mem0 не должен создавать отдельную memory database. Каноническое состояние остаётся в Rust Core и существующем SQLite/FTS5 Local Agentic RAG; Mem0-подобные идеи оформляются как Core-owned memory contracts и migrations.
- Будущие IPC-команды могут быть `memory.propose`, `memory.commit`, `memory.search`, `memory.get`, `memory.supersede`, `memory.forget`, `memory.history` и `memory.reconcile`; Electron показывает только redacted result/citations и не решает scope/approval.
- Ambient/transcript facts поступают в memory только после redaction и явной policy. Текущие правила сохраняются: `speaker=unverified`, audio BLOB не добавляется в SQLite, raw ambient capture не превращается автоматически в long-term memory.
- Реалистичный порядок: typed schema/provenance и scope → candidate extraction/fake LLM → deterministic sensitivity/approval → SQLite/FTS5 hybrid retrieval → entity/temporal enrichment → forget/reconciliation → benchmark/crash-restart tests. Acceptance criteria: no default egress, no cross-workspace retrieval, complete forget receipt, bounded latency/storage, source citations, stale/conflict handling и safe recovery после частичного сбоя.
- По сравнению с LlamaIndex/LangChain Mem0 фокусируется на personalized long-term memory lifecycle, но для Евы его нужно встроить как bounded Core subsystem, а не как ещё один orchestration/RAG framework.

### 21. Letta (MemGPT) / Letta Code

- **Источник:** [letta-ai/letta](https://github.com/letta-ai/letta), актуальный исходный код [letta-ai/letta-code](https://github.com/letta-ai/letta-code), [документация Letta Code](https://docs.letta.com/letta-code/)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `letta-code` `95aa5b411a89921f677e3e355e209bfad9455593`; package `@letta-ai/letta-code` `0.30.28`; `letta-ai/letta` на `main` является landing page
- **Лицензия исходного кода:** Apache-2.0. В LICENSE отдельно исключены брендовые материалы Letta; лицензии зависимостей, моделей, каналов и облачного сервиса проверяются отдельно.
- **Состав:** TypeScript/Bun-проект с CLI и desktop/web surfaces, agent harness, локальным backend, optional Letta Cloud/App Server, messaging channels, MCP, skills, hooks, schedules, subagents и git-backed MemFS.
- **Назначение:** persistent/stateful agents с identity, опытом и памятью, которая сохраняется между сообщениями, разговорами, перезапусками и средами выполнения.
- **Краткий вывод:** Letta даёт сильную reference-архитектуру для слоёв памяти, context engineering, typed memory writes, истории изменений и изоляции memory-worker. Runtime и cloud-модель не подходят как прямая зависимость Евы; ценность — в контрактах, тестовых идеях и security boundaries.

#### Что изучено

- Ссылка `letta-ai/letta` больше не является актуальным runtime: README направляет в `letta-ai/letta-code`, а историческая V1 на `archive` обозначена авторами как unsupported и без security updates. Анализ кода выполнен по актуальному `letta-code`, а не по архивной Python-системе.
- Архитектура разделяет несколько видов состояния: неизменяемый recall всей истории сообщений, recent messages текущего диалога, summaries после compaction, memory blocks, external memory/skills и agent identity. Recall ищется отдельным recall-subagent, поэтому вся история не помещается автоматически в каждый prompt.
- Стандартные memory blocks создаются из prompt-assets (`persona`, `human`) и становятся частью system prompt. `system/`-файлы считаются in-context memory; остальные markdown-файлы представлены metadata/описанием и читаются по необходимости. В коде есть read-only block label для защищённых блоков.
- Инструмент `memory` предоставляет явные операции `create`, `str_replace`, `insert`, `delete`, `rename`, `update_description` с обязательным непустым `reason`. Файлы ограничены memory directory, требуют frontmatter с description, запрещают path traversal и не позволяют изменить `read_only`-блок.
- `memory_apply_patch` вводит patch/hunk-формат с exact context. При несовпадении текущего файла операция отклоняется с диагностикой вместо молчаливого overwrite; add/update/delete проверяют существование, frontmatter и границы каталога.
- Каждая запись сначала требует чистого memory-repository, затем коммитится с agent identity в author/email; для remote MemFS harness после хода пытается синхронизировать clean commits. Git даёт diff, rollback, provenance автора и переносимость памяти между окружениями, но также создаёт sync/conflict и egress surface.
- MemFS имеет scoped path по `agent_id`, local и remote режимы, создание checkout, fast-forward pull/rebase/push, pre-commit проверки frontmatter и ограничения для memory worktrees. В актуальном локальном backend состояние может оставаться на машине без Letta Cloud.
- Для memory subagents есть fail-closed confinement: процесс не запускается без поддержанного sandbox backend, writable roots ограничены собственной memory directory/worktrees и harness state, а memory других агентов закрыта.
- Tool permissions отделяют read-only инструменты от опасных действий. При этом `memory` и `memory_apply_patch` в текущей конфигурации отмечены как не требующие approval; это осознанная возможность self-editing, но не готовый security baseline для чувствительной локальной Евы.
- Agent prompt прямо допускает self-evolution: агент может менять memory, skills, prompts и harness через mods. Есть subagents, hooks, cron/heartbeat и channels, благодаря чему агент работает за пределами одного интерактивного хода.
- Package требует Node `>=22.19.0`, использует Bun как package manager и зависит от Letta client, MCP SDK, React/Ink, WebSocket, terminal/desktop и channel libraries. Это отдельная TypeScript/Bun ecosystem, а не библиотека для Rust Core.
- Проверка была read-only: checkout `letta-code` чистый на указанной ревизии; сборку и полный test suite внешнего проекта не запускал, потому что задача — исследование, а не его разработка.

#### Что можем использовать в Еве

- **Layered memory contract.** Перенять разделение на bounded in-context blocks, archival/searchable memory, immutable conversation recall, procedural skills и ephemeral current context. Для Евы каноническое хранение остаётся в Rust Core/SQLite и существующем Local Agentic RAG, а не в отдельном MemFS runtime.
- **Core memory blocks.** Ввести ограниченные типизированные блоки для identity, user preferences, project facts и текущего working state. Каждый блок должен иметь schema/version, byte/token budget, scope, sensitivity, provenance, revision и optional expiration; oversized block не должен незаметно разрастаться внутри system prompt.
- **Typed memory operations.** Перенять отдельные read, search, propose/add, revise/supersede, forget, history и reset операции. LLM предлагает изменение через typed tool, но только Core решает scope, redaction, sensitivity, approval, transaction и итоговый status.
- **Patch/diff semantics.** Идея `memory_apply_patch` полезна для безопасного изменения memory: exact anchors, bounded patch, конфликт при устаревшей версии и понятный diff. В SQLite это лучше выразить через optimistic revision/CAS, append-only change event и reversible snapshot, не создавая второй Git-репозиторий в продукте.
- **In-context versus external discovery.** Хранить в system context только компактный индекс и правила поведения; подробности, цитаты и историю оставлять в FTS5/semantic archive. Это напрямую усиливает существующий context budget и context ledger EvoHime.
- **Recall worker.** Отдельный read-only worker/subagent для поиска старых разговоров и evidence может снижать prompt size. Результат должен возвращать redacted excerpts, source event/citation, relevance reason и uncertainty, а не произвольный transcript.
- **Scoped identity.** Перенять привязку memory к устойчивой agent identity, но расширить её до `workspace_id`, `profile_id`, `chat_id`, `repository_id`, `source_event_id` и capability scope. Все scope выводятся Core из authenticated context; поля от LLM или renderer не являются authorization.
- **Memory audit trail.** Полезны обязательные reason, actor, timestamp, old/new revision, affected paths и receipt. Для Евы это можно объединить с существующим event journal, export JSONL и backup/migration guarantees.
- **Confinement pattern.** Перенять fail-closed правило для фоновых memory/consolidation workers: без доступного sandbox работа не запускается; writable roots ограничены собственным scope, temporary worktree и служебным каталогом; память соседнего workspace/agent недоступна.
- **Context maintenance.** Идеи compaction, `/doctor`, dreaming/sleeptime и memory quality audit можно использовать как bounded Core jobs с cancellation, checkpoint, retry/recovery, token budget и понятным пользовательским receipt. Они не должны менять policy, identity или capability silently.
- **Self-evolution boundary.** Разделить в Еве user memory, procedural knowledge, system policy и executable capability. Агент может предложить изменение факта/процедуры, но не получает право менять approval policy, sandbox, provider secrets, IPC permissions или собственный Core.
- **Memory UI.** Перенять представление дерева/блоков, diff, source history и affected paths как основу для прозрачного Electron UI. Renderer только показывает данные Core и отправляет intent; он не открывает SQLite, memory files или workspace напрямую.
- **Test ideas.** Добавить fixtures на restart/recompile, stale revision conflict, oversized blocks, read-only policy, path traversal, cross-scope search, concurrent writes, failed sync, partial forget, prompt injection inside memory, sandbox absence и exact citation preservation.

#### Ограничения и риски

- **Runtime mismatch.** `letta-code` — Node/Bun/TypeScript CLI/harness с Letta client и optional server/cloud. Прямое подключение вернёт в EvoHime второй runtime, второй lifecycle и внешнюю модель хранения вместо Rust Core-first архитектуры.
- **Self-modifying prompt injection.** Пользовательский текст, workspace-файл или найденная web-страница могут попытаться записать persistent instruction в memory block, skill, hook или prompt. In-context memory имеет повышенную долговечность и приоритет, поэтому нужны provenance, trust level, redaction, immutable policy blocks и approval для чувствительных изменений.
- **Approval gap.** В текущем Letta Code memory tools не требуют approval. Для Евы нельзя переносить это поведение: даже «обычная» запись памяти может сохранить PII, ошибочный факт, prompt injection или изменить последующие решения.
- **Git/remote privacy.** Git history хранит старые версии и удалённые строки; remote MemFS может синхронизировать их на внешний сервер. Для EvoHime это конфликтует с local-first, retention/forget и секретами, если не добавить encryption, redaction, tombstone и проверяемое удаление всех производных копий.
- **Scope leakage.** Общая agent identity, несколько разговоров, subagents, channels и remote environments увеличивают риск cross-conversation/cross-agent retrieval. Простого `agent_id` недостаточно для workspace ACL, role, export или provider policy.
- **Unbounded self-evolution.** Возможность менять skills, prompts и harness полезна для исследований, но опасна в desktop-продукте: persistent change может обойти approval, привести к RCE через hook/mod или изменить security posture после одного prompt injection.
- **External egress.** Letta Cloud, channels, remote environments, telemetry, MCP и provider APIs создают network/data-egress paths. В базовой Еве они должны быть выключены или оформлены отдельными capability с явным consent и receipt.
- **License boundary.** Apache-2.0 допускает адаптацию кода при соблюдении условий, но не даёт права на Letta trademarks/brand assets и не покрывает модели, npm dependencies, cloud service или данные пользователей. Предпочтительнее перенимать идеи и собственные Rust-контракты.
- **Operational complexity.** Git sync, background reflection, cron, hooks, channels и subagents требуют recovery/observability. Не следует добавлять их одновременно с memory schema: каждая поверхность должна иметь отдельный capability, cancellation и failure receipt.

#### Предварительное решение

`адаптировать` layered-memory model, bounded core blocks, recall worker, typed patch/history contracts, scoped identity, memory audit и fail-closed confinement; `наблюдать` за Letta Code research вокруг dreaming, self-improvement и agent memory; `не подключать` `letta-code`, Letta Cloud/App Server, channels, archive V1 и git-backed remote MemFS в базовый runtime Евы.

#### Связь с EvoHime

- Letta подтверждает направление для будущего Core-owned memory subsystem поверх текущих SQLite/FTS5/RAG и context-budget механизмов: компактный always-in-context слой плюс архив с citations и неизменяемый transcript recall.
- Будущие IPC-команды могут быть `memory.block.get`, `memory.block.propose`, `memory.block.commit`, `memory.search`, `memory.recall`, `memory.history`, `memory.supersede` и `memory.forget`; Core сам устанавливает scope и approval outcome, Electron отображает diff/receipt.
- Любая запись из ambient/transcript проходит текущие redaction и policy rules; `speaker=unverified`, отсутствие audio BLOB и запрет автоматического превращения raw capture в long-term memory сохраняются.
- Практический критерий пригодности: после перезапуска и compaction Ева воспроизводит только разрешённые факты с citations, не смешивает workspace scopes, отклоняет stale patch, сохраняет audit/revision history, полностью обрабатывает forget и не запускает memory-worker без sandbox/permission.

### 22. AgentOps

- **Источник:** [AgentOps-AI/agentops](https://github.com/AgentOps-AI/agentops), [документация](https://docs.agentops.ai/), [README SDK](https://github.com/AgentOps-AI/agentops/blob/main/README.md)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `f8e907b92dabe47232978023fdcb01e2a7d4b752`; Python SDK `0.4.21`; checkout `main` на этой ревизии
- **Лицензия:** корневой Python SDK — MIT. Каталог `app/` (FastAPI API, Next.js dashboard и self-hosted platform) содержит отдельный `LICENSE` под Elastic License 2.0; лицензии интеграций и зависимостей проверяются отдельно.
- **Состав:** Python SDK поверх OpenTelemetry, decorators/context managers, provider/framework instrumentation, semantic conventions, OTLP exporters, metrics, validation helpers и legacy events; отдельный app-стек с FastAPI, Next.js, Supabase, ClickHouse и Docker Compose.
- **Назначение:** observability для AI agents: session replay, execution graph, LLM/tool/workflow spans, token usage, cost/latency/error metrics, framework integrations и evaluation/debugging.
- **Краткий вывод:** AgentOps полезен как reference для Core-owned telemetry schema и UI-диагностики, особенно для иерархии run → model/tool → result/error и унификации token/cost/latency. Python SDK и внешний backend создадут второй runtime и network egress; в базовую Еву их не подключать.

#### Что изучено

- Root README описывает session replay, step-by-step execution graphs, LLM cost management, framework integrations и self-hosting. Public SDK и dashboard — разные части проекта с разными лицензиями и операционными требованиями.
- SDK строится вокруг OpenTelemetry `TracerProvider`, `BatchSpanProcessor`, OTLP HTTP exporter и `MeterProvider`; queue size, flush interval, endpoint и custom exporter/processor конфигурируются. Сессия является root span, вложенные agent/workflow/operation/tool/LLM spans наследуют текущий context.
- Public API имеет `init`, `start_trace`, `end_trace`, `update_trace_metadata`, decorators `session`, `agent`, `task`, `workflow`, `operation`, `tool`, `guardrail`, `track_endpoint` и legacy-совместимость `start_session/end_session/ToolEvent/LLMEvent`.
- Decorator factory обрабатывает sync/async функции, sync/async generators и классы; записывает input/output, exception, tags, version и custom attributes. Для endpoint создаются отдельные request/response spans, для streaming span завершается после исчерпания генератора.
- Semantic conventions выделяют `agent.*`, `tool.*`, `workflow.*`, `gen_ai.*`, `operation.*`, `http.*`, trace/span/parent IDs, status и session end state. Indexed prompt/completion/tool-call fields задают стабильный формат для dashboard и анализа.
- Tool schema включает `tool.id`, `tool.name`, `tool.description`, `tool.parameters`, `tool.result`, `tool.status`; statuses — `executing`, `succeeded`, `failed`. Workflow schema дополнительно содержит run/session IDs, step status, model/provider, message/tool counts, streaming flag и memory/storage type.
- LLM instrumentation извлекает prompt/completion/total tokens, cached prompt/read tokens и reasoning tokens из разных provider response shapes. Есть token histogram, duration histogram, exception counter, generation-choice counter и derived token/cache efficiency.
- Trace lifecycle имеет explicit end state `SUCCESS`, `ERROR`, `UNSET`, context manager автоматически завершает span по exception, а shutdown пытается закрыть active traces и force-flush exporters. Отдельный validation helper запрашивает trace по ID, ждёт eventual export, проверяет минимальное число spans и наличие LLM activity/metrics.
- Authenticated OTLP exporter поддерживает dynamic JWT provider, timeout, compression, custom non-critical headers и fail-soft handling для 401/403/network/API errors. Authorization и другие критические headers нельзя переопределить пользовательскими headers.
- Trace attributes могут содержать serialized input/output до 1 MiB на значение; `safe_serialize` превращает модели и сложные объекты в JSON или string, но это redaction/secret filtering не заменяет. Prompt, tool arguments, результаты, headers и HTTP body могут попасть в telemetry при включённом capture.
- При создании стандартной session SDK добавляет system resource attributes: host/OS/CPU/RAM и imported libraries. Вспомогательный `get_host_env` также умеет собирать installed packages, рабочий каталог, virtualenv и disk details; `env_data_opt_out` присутствует в config/docs, но его применение к каждой точке сбора нужно проверять отдельно.
- Self-hosted app принимает OTLP, сохраняет trace/span/event maps в ClickHouse и строит dashboard через FastAPI/Next.js; Supabase отвечает за auth/primary data. В schema есть TTL для отдельных telemetry tables, но retention/PII policy Евы нельзя считать готовой по этому примеру.
- Unit tests покрывают session lifecycle, decorators, async/concurrent instrumentation, token counting, serialization, attributes, exporter header protection и provider fixtures. `python -m compileall -q agentops` прошёл; выбранные pytest-тесты не стартовали из-за отсутствующего dev-пакета `requests_mock` в окружении.

#### Что можем использовать в Еве

- **Core trace contract.** Перенять иерархию `run/session → agent turn → model call/tool call/guardrail → result/error`, связывая события через `trace_id`, `span_id`, `parent_span_id`, `correlation_id` и sequence. Это дополняет существующие Core run/task/approval receipts и Electron operations timeline.
- **Typed semantic conventions.** Зафиксировать Rust/IPC schema для `agent`, `model`, `tool`, `workflow`, `approval`, `retrieval`, `memory`, `listener` и `supervisor` events. Имена, status enums, version и required fields должны быть каноническими, а renderer не должен придумывать их самостоятельно.
- **Tool lifecycle.** Использовать состояния `requested/approved/running/succeeded/failed/cancelled/denied`, bounded parameters/result preview, duration, retry count и policy decision. Связать tool span с immutable approval `call_hash`, capability scope и durable receipt.
- **LLM usage accounting.** Перенять `prompt_tokens`, `completion_tokens`, `total_tokens`, cache/read/reasoning tokens, model/provider, finish reason и streaming timing. Стоимость считать в Core по pinned model-price manifest, не доверять цене, пришедшей от provider или UI.
- **Latency metrics.** Ввести duration/TTFT/streaming duration/chunk count, queue wait, tool execution, retrieval and approval latency. Histogram/counter semantics пригодны для локальной диагностики и budget decisions, без отправки raw prompts.
- **Error and end-state model.** Явно различать success, error, cancelled, denied, indeterminate, timeout и crash-recovered. Это улучшит текущие supervisor/core logs и не позволит UI считать «нет ответа» успешным завершением.
- **Context propagation.** Перенять OpenTelemetry-style current context для nested async/tool work, но реализовать его в Rust task-local execution context и IPC frames; не добавлять Python contextvars или второй telemetry SDK.
- **Bounded capture policy.** Идею decorator `capture_request/capture_response` адаптировать как per-event capture mode: metadata-only, redacted preview, hash/citation или explicit full payload. Default для Евы — no raw prompt/secret/header/audio capture; payload size, depth and item counts ограничиваются Core.
- **Evaluation hooks.** Перенять trace-based validation: после run проверять наличие ожидаемых spans, tool/LLM activity, terminal state, citations, approval receipts и budgets. Evaluation result должен ссылаться на trace/run IDs и fixture revision.
- **Local replay/debug UI.** Дерево spans, timeline, session drilldown и time-to-event графики полезны для Electron OperationsPanel. UI получает redacted snapshots через IPC, а не читает OTLP/SQLite/ClickHouse напрямую.
- **Export abstraction.** Интерфейс custom exporter/processor полезен как design-only boundary: local JSONL/SQLite sink по умолчанию, optional OTLP exporter только как явно включаемая provider/network capability с consent, redaction и bounded queue.
- **Quality fixtures.** Перенять provider fixtures и тесты на streaming, async generators, nested spans, partial failures, exporter auth/header protection, missing usage fields, retries, flush/shutdown и concurrent runs. Добавить Windows supervisor restart, IPC replay и approval denial cases.
- **Cost and resource dimensions.** Учитывать model/provider, workspace/project scope, run/task, prompt source, retrieval count, tool count, CPU/memory and listener/vision workload. Это позволит видеть стоимость и latency, не превращая observability в скрытое хранение пользовательского контекста.

#### Ограничения и риски

- **Second runtime/backend.** Python SDK требует Python/OpenTelemetry и provider-specific integrations; self-hosted platform требует FastAPI, Next.js, Supabase, ClickHouse, Docker и внешнюю auth/storage topology. Это несовместимо с Rust Core + SQLite + supervisor как единственным владельцем state.
- **Default cloud egress.** SDK по умолчанию экспортирует traces/metrics на `otlp.agentops.ai`, использует API/JWT auth и dashboard URLs. Для local-first Евы это должно быть выключено; даже optional exporter не может отправлять данные до redaction/consent.
- **Sensitive payload capture.** Input/output decorators сериализуют произвольные args/results и могут записать prompts, tool arguments, filesystem paths, HTTP headers/body, secrets, PII, ambient transcript или screenshots. Size limit — не privacy filter; нужен deny-by-default redaction and field policy.
- **Environment leakage.** Host/OS/CPU/RAM/imported-libraries and helper environment data раскрывают fingerprint, working directory, installed packages и disk layout. `env_data_opt_out` нельзя принимать на доверии без теста всех collection paths; в Еве environment telemetry должна быть allow-listed and coarse-grained.
- **Telemetry availability versus execution.** Fail-safe exporter errors и queued batch export полезны для observability, но Core execution не должен ждать сеть, менять tool outcome или терять approval receipt из-за exporter failure. Local durable event write должен быть отдельным обязательным путём.
- **Loss and eventual consistency.** Batch queue, periodic flush, shutdown and network retry могут терять/задерживать spans. Для Евы acceptance требует SQLite transaction/sequence first, replay after restart and explicit export status.
- **Semantic drift.** В проекте есть legacy event API и современный OpenTelemetry API, несколько provider integrations и TODO/отклонения от GenAI conventions. Не копировать поля без собственной schema governance and compatibility tests.
- **Dashboard license boundary.** MIT относится к SDK, но `app/` лицензирован ELv2 и его hosted-service ограничения неприменимы как готовая база для redistributable EvoHime. Внешние SaaS, Supabase, ClickHouse, Sentry/PostHog/Stripe и provider licenses также отдельны.
- **Privacy/retention mismatch.** ClickHouse tables, trace replay and indexed attributes are designed for analytics, not user-visible forget of prompts, paths, secrets or derived data. Retention and erasure must be Core-owned and verifiable.
- **Instrumentation fragility.** Monkey-patching/provider wrappers and decorators can break across SDK versions, miss streaming branches or double-instrument calls. In EvoHime instrumentation should happen at canonical Core gateways, not by patching arbitrary user libraries.
- **Metrics are not truth.** Token/cost values may be missing, provider-specific or estimated; dashboard success and LLM span presence do not prove task correctness, policy compliance or user authorization. Evaluation needs deterministic receipts and independent assertions.

#### Предварительное решение

`адаптировать` OpenTelemetry-inspired trace/span hierarchy, typed semantic conventions, tool/LLM lifecycle, token/latency/cost metrics, bounded capture modes, local replay UI and trace-based validation; `наблюдать` за AgentOps TypeScript SDK/evaluation features; `не подключать` Python SDK, default cloud OTLP, AgentOps SaaS и ELv2 self-hosted app в базовый runtime Евы.

#### Связь с EvoHime

- AgentOps хорошо ложится на уже существующие Core-owned events, supervisor lifecycle, approval receipts, retrieval citations и Electron OperationsPanel: нужна не внешняя observability platform, а единый локальный event/trace contract.
- Возможные будущие IPC/read models: `run.trace`, `run.span`, `run.metrics`, `run.replay`, `run.export_status`; write-side остаётся только в Core, а Electron получает redacted snapshots с sequence/correlation IDs.
- Для LLM/model gateway логировать usage и timing; для tools — canonical call hash, scope, approval, cancellation, output summary; для RAG/memory — query plan, source IDs, citations, freshness and redaction decision; для ambient — только уже разрешённые transcript events, без audio BLOB.
- Базовые критерии: no network required for execution, no raw secrets/prompts by default, durable local event before optional export, bounded payload/queue, restart recovery, ordered replay, exact approval linkage, scope isolation, retention/forget and exporter failure cannot change tool result.

### 23. THUDM AgentBench

- **Источник:** [THUDM/AgentBench](https://github.com/THUDM/AgentBench), [описание framework](https://github.com/THUDM/AgentBench/blob/main/docs/Introduction_en.md), [AgentRL environment overview](https://github.com/THUDM/AgentRL)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `d1e4a10db08c87075c78972e48ecc182be03e2d5`; checkout `main` чистый
- **Лицензия:** Apache-2.0 для AgentBench; AgentRL, который используется текущей FC-веткой, имеет отдельную MIT-лицензию и отдельные условия заимствования
- **Состав:** исходная версия с Agent Client, Task Client, Task Controller/Workers и Assigner; текущая AgentBench FC интегрирована с AgentRL и контейнеризует `alfworld`, `dbbench`, `knowledgegraph`, `os_interaction` и `webshop`
- **Назначение:** воспроизводимая многозадачная оценка LLM-агентов в интерактивных средах: от OS/DB/KG до WebShop, ALFWorld и старых карточных/диалоговых сценариев
- **Краткий вывод:** это хороший источник архитектуры evaluation harness и контрактов сценария, сессии, action/observation, статусов, reward и независимого judge. Подключать Python/Docker benchmark как runtime Евы не нужно; полезные части следует реализовать как Core-owned локальный evaluation слой.

#### Что изучено

- Исходная архитектура намеренно разделяет Task Server, Agent Server и Client. Task Controller регистрирует workers и выдаёт единый API `start_sample`/`interact`, а Task Worker владеет конкретной интерактивной средой.
- `TaskClient.run_sample` запускает сессию, передаёт историю агенту, принимает ответ, возвращает его в среду и завершает цикл по terminal status. Ошибки сети, старта, взаимодействия, недоступности worker и ошибки агента разделены и могут быть повторены.
- Типизированные модели выделяют `SampleStatus`: running, completed, context limit, validation failed, invalid action, task limit, unknown и task error. `AgentOutput` отдельно поддерживает normal, cancelled и context-limit состояния.
- Текущая FC-ветка использует function-calling вместо старого свободного формата ReAct. В OS task tool call имеет имя, JSON-аргументы и `tool_call_id`; action parser различает `bash_action`, `finish_action` и `answer_action`, а не принимает произвольный текст как исполняемую команду.
- Task worker на каждый sample создаёт и уничтожает изолированную среду. В OS task это контейнерная сессия с init/start scripts, продлением lease, bounded round limit, очисткой terminal escape sequences, обрезкой длинного вывода и обязательным cleanup в `finally`.
- Judge отделён от agent loop: после terminal answer выполняются `match` или независимые `check` scripts, затем выдаются `result`, `status` и reward/score. Общий итог сохраняет total/pass/wrong/accuracy, а клиент дополнительно считает доли статусов и длину истории.
- Конфигурация декларативно связывает `agent × task`, concurrency и output directory. Assigner восстанавливает завершённые samples из `runs.jsonl`, сохраняет `error.jsonl`, поддерживает resume/auto-retry и распределяет работу через max-flow по свободной ёмкости agents и tasks.
- Набор задач проверяет разные способности: OS — bounded tool use и операции в файловой системе; DB — SQL/структурированные запросы; KG — многошаговый графовый поиск; ALFWorld — состояние и planning; WebShop/Mind2Web — web interaction. В исходной версии есть Dev/Test split и многошаговые траектории.
- Текущий deployment требует Docker Compose, AgentRL Controller, отдельные workers, Redis для allocation и Freebase service для KG. README прямо предупреждает о потреблении около 16 GB RAM WebShop и утечке памяти/диска в ALFWorld до перезапуска worker.
- `py -m compileall -q src` на checkout прошёл; обнаружено предупреждение Python о неэкранированной regex-последовательности в OS task. Полный benchmark не запускался: он требует Docker, модели/API, datasets и тяжёлых внешних сервисов.

#### Что можем использовать в Еве

- **Локальный evaluation harness как отдельный слой.** Завести версионируемые `eval.scenario` fixtures, которые запускают Еву через Core/IPC и не зависят от production workspace. Сценарий должен иметь id/version, описание, required capabilities, seed/fixture revision, budgets, ожидаемый terminal predicate и независимый evaluator.
- **Контракт сценария `reset → step → observe → evaluate`.** Перенять чёткое разделение начальной настройки, одного agent action, observation, terminal state и judge. Это удобно для тестов tool execution, RAG, memory, browser и recovery, не смешивая их с обычным chat transcript.
- **Typed action/function-calling evaluation.** Проверять не только текст ответа, но и canonical tool name, JSON schema, аргументы, call id, policy decision, approval receipt и result. Невалидный action должен давать отдельный статус и bounded feedback, а не попадать в shell/browser executor.
- **Набор конечных статусов.** Адаптировать различия `completed`, `cancelled`, `timeout/limit`, `invalid_action`, `validation_failed`, `denied`, `task_error`, `crash_recovered` и `context_limit`. Это пригодится для реальных acceptance-метрик, где провал по безопасности не должен сливаться с обычным неправильным ответом.
- **Изолированный fixture environment.** Для OS-like тестов использовать временный workspace или отдельный supervisor-controlled worker; network по умолчанию выключать, задавать CPU/RAM/disk/time limits, очищать окружение после каждого sample и проверять отсутствие утечки процессов/файлов. Произвольные benchmark shell-команды на хосте не выполнять.
- **Независимый детерминированный judge.** Перенять `match` для точных ответов и `check` для состояния/артефактов. В Еве evaluator должен проверять итоговые filesystem/database/event predicates, policy/approval invariants и citations, а не доверять заявлению модели или LLM-as-a-Judge.
- **Trajectory и reward ledger.** Сохранять для каждого sample ordered events: run/session, step, prompt/evidence hash, action/tool call, observation preview/hash, approval, duration, status, evaluator result и resource usage. Score может включать success, safety, efficiency, latency, retries, cost и citation correctness; сырые секреты и полные prompt/tool payloads по умолчанию не сохранять.
- **Матрица agent × task и bounded concurrency.** Использовать идею декларативных assignments для nightly/CI evaluation: несколько model/provider/prompt revisions против набора сценариев, с ограничением одновременных runs и отдельными budgets. Восстановление по JSONL/SQLite sequence после падения должно быть частью контракта.
- **Повторяемость и split.** Разделить dev fixtures и скрытый test set, закреплять provider/model/prompt/config/fixture revision, seed, OS/runtime version и evaluator version. Результаты должны быть сравнимы только при совпадающей provenance, иначе показывать `not comparable`.
- **Категории сценариев для Евы.** Первыми сделать локальные лёгкие варианты OS/tool safety, DB/query validation, RAG multi-hop, memory scope/forget, approval denial, cancellation, malformed tool call, provider timeout и supervisor restart. Web/browser и multimodal tasks оставить опциональными внешними workers.
- **Worker health и recovery.** Перенять controller/worker health, heartbeat, lease renewal, capacity и restart semantics. Это особенно полезно для Windows supervisor: crash или resource exhaustion одного evaluation worker не должен ломать Core и соседние пользовательские runs.
- **Extension contract.** Новый task должен добавлять только manifest/fixture, environment adapter, action schema и evaluator; общий runner, trace storage, cancellation, timeout, redaction и report format остаются едиными. Так можно расширять evaluation без копирования orchestration кода.

#### Ограничения и риски

- **Не runtime Евы.** Репозиторий закреплён на старом Python stack (`numpy~=1.23`, Pydantic 1, FastAPI и другие зависимости), а текущая FC-ветка зависит от AgentRL. Это второй runtime и не соответствует Electron + Rust Core + SQLite + supervisor.
- **Тяжёлая и хрупкая инфраструктура.** Docker, Redis, MySQL/Freebase, browser data и task workers усложняют Windows packaging, offline-first и поддержку. Предупреждения о RAM/disk leaks требуют bounded quotas, restart policy и cleanup acceptance tests даже в отдельном evaluation service.
- **Внешние источники снижают воспроизводимость.** KG в старой версии зависел от нестабильного online SPARQL; cloud/API providers, WebShop и реальные сайты меняют состояние. Для Евы нужны локальные fixtures, fake providers и pinned snapshots; network evaluation — отдельный opt-in класс.
- **Нельзя смешивать версии протокола.** Старый AgentBench принимает текстовые `Think/Act`, текущий FC — function calls и AgentRL task API. Нельзя переносить старый prompt parser в production tools; каноническим источником для Евы остаются собственные IPC/tool schemas.
- **Benchmark success не равен безопасности.** Простая accuracy/pass метрика не доказывает корректность authorization, sandbox, redaction или отсутствие side effect. Каждый сценарий должен иметь отрицательные cases и safety assertions с более высоким приоритетом, чем task reward.
- **Side effects и секреты.** OS/DB/web tasks могут менять файлы, базы, network state и передавать контекст модели. Промышленный runner обязан использовать synthetic data, ephemeral state, allow-listed capabilities, no-network по умолчанию и отдельное consent для внешних providers.
- **Результаты чувствительны к окружению.** Model/provider, prompt, context window, tool descriptions, concurrency, dataset split и evaluator version меняют score. Без provenance и сравнения конфигураций цифры будут вводить в заблуждение.
- **Данные и лицензии.** Apache-2.0 покрывает код AgentBench, но datasets, Docker images, Freebase/ALFWorld/WebShop/Mind2Web и AgentRL имеют отдельные условия и attribution. Перед включением fixtures в EvoHime нужно проверить право на redistribution и не включать внешние данные в пользовательскую память.

#### Предварительное решение

`адаптировать` scenario/task/evaluator contracts, function-call validation, isolated fixtures, typed terminal statuses, trajectory ledger, deterministic scoring, matrix assignments, resume/retry и worker health semantics; `наблюдать` за AgentRL/FC evolution и использовать его как внешний research harness; `не подключать` Python benchmark runtime, Docker Compose stack, Redis/Freebase, внешние datasets/services и прямое выполнение benchmark actions на production host.

#### Связь с EvoHime

- AgentBench следует связать с будущим локальным evaluation-планом поверх Core-owned run/event storage, существующих approval/call-hash, context-budget, RAG citations и supervisor recovery. Electron должен показывать отчёт и trace через IPC, но не запускать evaluator и не читать fixtures напрямую.
- Возможные будущие сущности: `eval_scenario`, `eval_run`, `eval_step`, `eval_assertion`, `eval_score`, `eval_artifact`; каждая должна иметь scope, fixture/evaluator revision, run/sequence IDs, policy outcome и retention/forget semantics.
- Evaluation runner должен вызывать те же Core tool gateways, что и пользовательский режим, но в отдельном capability scope и ephemeral workspace. Нельзя создавать специальный обход approval или скрытую ветку исполнения только ради высокой benchmark accuracy.
- Базовые критерии: offline deterministic smoke suite; zero host side effects; invalid/denied/cancelled actions classified separately; full ordered replay after restart; bounded CPU/RAM/disk/time; evaluator independent from agent answer; no raw secrets/prompts by default; reproducible score with provenance; exporter/test failure cannot alter Core execution.

### 24. Traceloop OpenLLMetry

- **Источник:** [traceloop/openllmetry](https://github.com/traceloop/openllmetry), [OpenLLMetry documentation](https://traceloop.com/docs/openllmetry), [OpenTelemetry](https://opentelemetry.io/)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `62e24c2ffde6c1ee04dc290e52d8d5dbda054cff`; пакет `traceloop-sdk` версии `0.62.3`; checkout `main` чистый
- **Лицензия:** Apache-2.0 для репозитория и SDK; лицензии OpenTelemetry, provider SDK, instrumentation packages и подключаемых exporters проверяются отдельно
- **Состав:** Python SDK, OpenTelemetry GenAI semantic conventions, instrumentations для LLM providers/vector DB/frameworks/MCP, decorators/manual spans, metrics/logs/exporters, prompt manager, experiments/trajectory capture, annotations и guardrail/evaluator API
- **Назначение:** инструментировать существующее LLM-приложение и отправлять traces, metrics и logs в совместимые с OpenTelemetry backends; отдельно поддерживать эксперименты, user feedback и удалённые evaluators/guardrails
- **Краткий вывод:** OpenLLMetry — сильный reference для семантики LLM/tool/RAG событий, токенов, latency, streaming и prompt provenance. В Еве нужно перенять контракты и тестовые fixtures на стороне Rust Core, сохранив локальную SQLite/event модель; Python SDK и его provider monkey-patching не подходят как продуктовая зависимость.

#### Что изучено

- Root README описывает OpenLLMetry как набор OpenTelemetry extensions: можно использовать стандартные instrumentations отдельно или `traceloop-sdk` для автоматического запуска. Поддерживаются LLM providers, vector DBs, LangChain/LlamaIndex и MCP, а traces можно направлять в разные OTLP-compatible destinations.
- `traceloop-sdk` 0.62.3 требует Python 3.10+ и тянет OpenTelemetry API/SDK/exporters, semantic conventions, requests/SQLAlchemy/Redis instrumentation и большой набор provider/framework packages. Это монорепозиторий с множеством отдельных пакетов и собственными tests/cassettes.
- `Traceloop.init` поднимает tracer, metrics и optional logging providers, выбирает batch или simple processor, endpoint/headers/API key из аргументов и environment, а также автоматически активирует доступные instrumentors. Есть custom exporter/processor, disable flags и per-instrument selection.
- Manual API задаёт `LLMMessage`, `LLMUsage` и `track_llm_call(vendor, type)`. В span можно записать request/response model, роли и content сообщений, prompt/completion, input/output/total/cache token counts и request type. Это полезный минимальный контракт для providers, которые нельзя автоматически обернуть.
- Provider instrumentations записывают model/system, request parameters, streaming, structured-output schema, finish reason, tool calls, token usage, reasoning/cache tokens, duration, errors и streaming time-to-first-token/time-to-generate. Для OpenAI отдельно поддерживаются chat/completions, responses, embeddings, image generation и realtime paths.
- SDK выделяет workflow/agent/task/tool span kinds, entity name/version/path, conversation id, association properties, managed prompt key/version/hash/template variables и context propagation. Эти поля позволяют собирать иерархию run → workflow/agent → model/tool/retrieval без привязки UI к конкретному provider.
- Контентная политика в коде имеет два слоя: `TRACELOOP_TRACE_CONTENT` и context override, а `ContentAllowList` сопоставляет association properties с серверным allow-list. В текущей реализации default environment для content tracing — `true`; значит отсутствие явного запрета нельзя считать безопасной redaction policy.
- `dont_throw` делает instrumentation fail-soft: ошибки трейсинга логируются и не должны ломать wrapped provider call. Это полезная характеристика optional telemetry, но не замена обязательной записи Core event/approval receipt до эффекта инструмента.
- Experiment utility создаёт `InMemorySpanExporter`, запускает задачу с `disable_batch=True`, извлекает из spans prompt/completion trajectory, tool names/inputs/outputs и final completion. Это простой способ тестировать agent behavior без внешнего exporter.
- Evaluation/guardrail API нормализует input fields, допускает custom evaluator, thresholds и conditions, поддерживает fail-fast или run-all, parallel/sequential execution и варианты `raise`, `log` или `ignore` on failure. Реальные evaluator calls идут по удалённым `/v2/evaluators/...` или `/v2/guardrails/...` endpoints.
- В SDK есть annotation/user-feedback и experiment metadata с dataset slug/version, evaluator slugs и run metadata. Это хороший reference для provenance, но backend и persistence находятся за API клиента, а не в локальной SQLite модели.
- `py -m compileall -q` для SDK, OpenAI instrumentation и LangChain instrumentation прошёл без ошибок. Полный suite не запускался: checkout содержит много provider-specific packages, а их интеграционные tests требуют соответствующих Python SDKs и cassette/test dependencies.

#### Что можем использовать в Еве

- **Канонические локальные GenAI event fields.** Адаптировать `system/provider`, model, operation, request/response type, finish reason, streaming, tool call, structured-output schema, input/output/total/cache/reasoning tokens, TTFT и generation duration в Rust/Core schema. Эти поля дополняют AgentOps-карточку конкретными LLM semantics.
- **Иерархия spans без внешнего OpenTelemetry runtime.** Использовать `workflow`, `agent`, `task`, `tool`, `retrieval`, `memory`, `guardrail`, `approval` и `model` как типизированные event kinds, связанные `run_id`, `parent_id`, `sequence` и `correlation_id`. На wire/SQLite хранить собственный компактный контракт, а OTLP рассматривать только как optional export projection.
- **Manual instrumentation boundary.** Для каждого Core gateway иметь явный `start/end` lifecycle, чтобы один и тот же код покрывал локального provider, HTTP provider, streaming и тестовый fake provider. Это предпочтительнее автоматического monkey-patching внешних библиотек и сохраняет Core единственным владельцем state.
- **Streaming и partial failure fixtures.** Перенять проверки TTFT, stream duration, chunk count, cancellation, provider exception, malformed response, missing usage, retries и late stream termination. Для Евы дополнить их IPC replay, supervisor restart и approval denial.
- **Prompt/config provenance.** Хранить prompt template key/version/hash, model/provider revision, tool-manifest revision, context policy, RAG query/evidence revision и evaluator version. Полный prompt по умолчанию заменять redacted preview, hash или citation IDs; пользовательский opt-in должен быть отдельным capture mode.
- **Association/scope metadata.** Идею association properties адаптировать к `workspace_id`, project/repository, chat/session, user/identity, run/task и capability scope. Core обязан валидировать их сам и не принимать произвольные UI tags как основание для доступа или content export.
- **Usage и cost accounting.** Записывать provider-reported usage как untrusted observation, а normalized totals и cost считать в Core по pinned model-price manifest. Отдельно показывать estimated/missing usage и не давать отсутствующему usage превращаться в нулевую стоимость.
- **Local in-memory trajectory capture.** Перенять подход `InMemorySpanExporter` для unit/integration evaluation: собрать bounded trajectory из Core events, проверить tool calls/observations/final result, затем удалить fixture. Для длительных runs использовать SQLite sequence и redacted projections, а не бесконечный memory buffer.
- **Evaluator input normalization как тестовый helper.** Сопоставление `question/prompt/query`, `answer/completion`, `context`, `trajectory` полезно для evaluation fixtures, но в Еве canonical field names должны быть строго определены schema и не маскировать отсутствующие доказательства.
- **Guardrail lifecycle.** Взять typed `guardrail` span с name/version, input/output summary, threshold, condition, pass/fail/error, latency и policy action. LLM evaluator остаётся advisory signal; hard-deny, approval и capability policy должны исполняться локально Core.
- **Fail-fast/parallel policy для advisory checks.** Для независимых redaction/injection/format checks можно описывать `parallel`, `fail_fast`, deadline и aggregate outcome. Все результаты должны иметь bounded input, evaluator revision и explicit `unknown/timeout`, а не молча считаться pass.
- **Tests with recorded fixtures.** Перенять cassette-like tests для provider response shapes, streaming chunks, tool calls, embeddings and structured outputs, но заменить реальные prompts/secrets synthetic data и проверять redaction/header absence. Это позволит сохранять compatibility без сетевых запросов.
- **Optional OpenTelemetry export projection.** В будущем сделать однонаправленный local event → OTLP adapter с allow-listed fields, queue budget, explicit consent и export status. Export failure не должен блокировать run, менять tool result или удалять локальный receipt.

#### Ограничения и риски

- **Несовместимый runtime.** SDK и все instrumentations — Python, в основном через wrappers/monkey-patching provider libraries. EvoHime использует Electron shell + Rust Core + SQLite + supervisor; прямое добавление traceloop-sdk создало бы второй runtime и не дало бы наблюдаемости над нативными Core gateways.
- **Content tracing небезопасен по умолчанию.** `TRACELOOP_TRACE_CONTENT` в коде имеет default `true`, а manual API прямо записывает prompt/completion content и headers/metadata могут включать чувствительные значения. Это противоречит local privacy boundary Евы без собственной allow-list/redaction policy.
- **Remote export и configuration egress.** `Traceloop.init` ориентирован на OTLP endpoint, API key, metrics/logging endpoints и Traceloop API; при облачной конфигурации traces, prompt content, tool arguments, paths и usage могут покинуть устройство. Remote prompt/config sync и evaluator endpoints должны считаться отдельными network capabilities.
- **Allow-list не равен privacy boundary.** Association-based server allow-list может разрешить content tracing для совпавшего scope, но он не доказывает отсутствие secrets/PII и не заменяет Core-owned field classification, redaction, retention/forget и user consent.
- **Автоматическая instrumentation surface.** Автоподключение всех найденных provider/framework packages повышает риск двойных spans, несовместимости версий, захвата сетевых headers и изменения поведения обёрнутых клиентов. В Еве instrumentировать только канонические Rust boundaries.
- **Fail-soft telemetry может скрывать потерю данных.** `dont_throw` правильно не ломает provider execution, но optional exporter может потерять spans, а batch processors добавляют eventual consistency. Для Евы сначала нужен durable local event с sequence, затем best-effort export и отдельный diagnostic status.
- **LLM guardrails не являются hard security.** Remote toxicity/PII/prompt-injection/evaluator scores могут быть ошибочными, недоступными или скомпрометированными prompt injection. Они не должны автоматически выдавать capability, обходить approval или определять единственный итог policy.
- **Поверхность зависимостей и лицензий.** Один SDK тянет множество provider/vector/framework packages с собственными версиями и лицензиями; отдельные packages могут развиваться с разной скоростью. Apache-2.0 репозитория не означает совместимость всех подключаемых dependencies или backend services.
- **Высокая cardinality и payload cost.** Prompt/completion attributes, tool arguments, per-message fields, association tags и provider-specific metadata раздувают spans, SQLite/OTLP queues и UI. Нужны bounded depth/bytes, hash/preview modes, sampling и запрет raw payload по умолчанию.
- **Не вся observability переносится в product UX.** OpenTelemetry traces удобны для backend analytics, но renderer Евы не должен читать OTLP/экспорт или произвольные spans. Core должен выдавать redacted, scope-checked read model через IPC.
- **Evaluation provenance смешивается с telemetry.** Dataset/evaluator/experiment metadata и user feedback должны иметь отдельную retention/consent policy и не попадать автоматически в долгосрочную memory/RAG. Результат evaluator не является фактом пользователя.

#### Предварительное решение

`адаптировать` GenAI semantic fields, typed span/event hierarchy, manual instrumentation boundary, prompt/model/tool provenance, streaming/usage metrics, bounded capture modes, local trajectory capture и evaluator test fixtures; `наблюдать` за OpenTelemetry GenAI semantic conventions и совместимостью provider instrumentation; `не подключать` Python `traceloop-sdk`, automatic monkey-patching, default OTLP/cloud export, remote prompt manager и remote guardrail/evaluator services в базовый runtime Евы.

#### Связь с EvoHime

- OpenLLMetry дополняет AgentOps-карточку: AgentOps даёт общую trace/session/metrics модель, а OpenLLMetry — более конкретные LLM, streaming, prompt provenance, vector DB и GenAI semantic conventions. В будущей схеме нужно выбрать собственный минимальный Rust vocabulary и не копировать два SDK.
- Для Core-owned observability можно расширить текущие event/trace contracts полями `gen_ai_system`, `model`, `operation`, `request_type`, `finish_reason`, `usage`, `streaming`, `tool_call`, `prompt_revision`, `evidence_revision`, `capture_mode` и `export_status`; имена должны пройти собственный schema review и IPC compatibility tests.
- Electron OperationsPanel получает только redacted run/span projections: timing, status, model/provider, token totals, tool/approval links, citations и bounded previews. Raw prompts, secrets, headers, full tool arguments и ambient audio/transcript остаются запрещёнными по умолчанию.
- Evaluation runner из AgentBench может использовать локальный trajectory projection, inspired by `InMemorySpanExporter`, для проверки deterministic scenarios, guardrail outcomes, provider fallback, RAG citations и recovery. В production run evaluation must not alter policy path or bypass approval.
- Базовые критерии: no network required for telemetry or execution; durable event before optional export; no raw content by default; explicit field-level capture consent; bounded payload/cardinality/queue; ordered replay after restart; redaction/forget across local events and exports; provider wrapper failure cannot change Core result; evaluator/guardrail outage yields explicit unknown/timeout.

### 25. OpenBMB AgentVerse

- **Источник:** [OpenBMB/AgentVerse](https://github.com/OpenBMB/AgentVerse), [task-solving README](https://github.com/OpenBMB/AgentVerse/blob/main/README_tasksolving_cases.md), [simulation README](https://github.com/OpenBMB/AgentVerse/blob/main/README_simulation_cases.md)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `f90c4bd9680fdd3bcff8c52c9170911a59b23478`; последний commit `2024-09-09`; пакет `agentverse 0.1.8.1`; checkout `main` чистый
- **Лицензия:** Apache-2.0
- **Состав:** Python framework, YAML task configs, registries для agents/LLMs/environments/rules/memory/parsers, task-solving pipeline, simulation environments, CLI/GUI commands, datasets и optional BMTools/XAgent tool services
- **Назначение:** сборка групп LLM-агентов для решения задач и моделирование поведения нескольких агентов в управляемой среде. README выделяет два независимых направления: task-solving и simulation.
- **Краткий вывод:** AgentVerse полезен как reference для composable environment lifecycle, typed message routing, role/pipeline decomposition и simulation fixtures. Прямое подключение не подходит: Python 3.9+ stack, старые OpenAI/LangChain/Pydantic pins, внешние tool servers, mutable in-memory state и исследовательская нестабильность simulation.

#### Что изучено

- `AgentVerse.from_task` и `Simulation.from_task` читают YAML, через registry создают agents, memory, LLM, tools, parser и environment. `run` выполняет `reset → step` до terminal state, `next` позволяет пошаговый запуск, `update_state` — изменять состояние environment.
- Базовый `BaseEnvironment` задаёт `agents`, `rule`, `max_turns`, `cnt_turn`, `last_messages`, `rule_params`, async `step`, `reset`, `is_done` и отчёт по стоимости. Это простой контракт среды, пригодный для локальных evaluation fixtures.
- `Message` — Pydantic-модель с content, sender, receiver set, sender_agent и tool_response. Специализированные сообщения (`SolverMessage`, `CriticMessage`, `ExecutorMessage`, `EvaluatorMessage`, `RoleAssignerMessage`) добавляют типовые поля для разных этапов pipeline.
- Simulation environment разделяет правила на `order`, `visibility`, `selector`, `updater` и `describer`. На каждом шаге выбираются агенты, строится индивидуальное описание, ответы могут выполняться параллельно через `asyncio.gather`, затем сообщения фильтруются, память обновляется и видимость пересчитывается.
- Task-solving environment отдельно композит `role_assigner`, `decision_maker`, `executor` и `evaluator`. Типичный цикл: recruit experts → build plan → execute subtasks/tools → evaluate score/advice → accept или повторить следующий round.
- Role assignment может быть одноразовым или повторяться по раундам; decision makers поддерживают central/horizontal/vertical/dynamic/concurrent варианты; executor и evaluator являются отдельными pluggable stages. Это позволяет сравнивать orchestration strategies при одной и той же группе агентов.
- Registry pattern позволяет регистрировать новые classes по строковому ключу и создавать их из YAML. Однако конфигурация использует mutable `pop`, глобальные registry dictionaries и динамическую загрузку, поэтому контракты не являются безопасной schema boundary сами по себе.
- Conversation agent формирует prompt из role/environment/chat history и делает retry при ошибке parser/LLM. Tool agent выполняет цикл `model → parse AgentAction → tool → observation` до AgentFinish, а task-solving executor использует function-call-like output parser и проверяет имя функции по списку tools.
- Chat history memory умеет bounded history по token limit и LLM-generated running summary. Summary создаётся отдельным вызовом модели, удаляет user messages из summarization input и хранит last trimmed index; это полезная идея, но не доказательный memory store.
- Reflection memory manipulator оценивает importance/immediacy, применяет recency/relevance/importance/immediacy scoring и cosine similarity, затем генерирует вопросы/insights. Plan manipulator сохраняет LLM-generated планы в self-only memory.
- Task-solving example показывает декларативную multi-stage задачу с role assigner, planner/solver, critics, executor, evaluator и manager, общими prompt anchors и отдельными model/temperature/token budgets для каждой роли.
- Simulation examples включают classroom, prisoner dilemma, Pokemon и code-repair team; правила задают порядок, видимость сообщений, селекцию, обновление памяти и environment descriptions. SDE team применяет writer/tester/reviewer loop с unit-test feedback.
- README требует Python >=3.9, OpenAI/Azure credentials для стандартных примеров, а local/vLLM/FSChat режимы требуют дополнительных серверов и зависимостей. Tool-using cases используют BMTools или XAgent ToolServer.
- README прямо предупреждает, что simulation code refactoring идёт сейчас; для стабильного simulation-only варианта предлагается `release-0.1`. На исследуемой ревизии последний commit датирован 2024-09-09.
- `py -m compileall -q agentverse agentverse_command` прошёл без ошибок. Полный запуск не выполнялся: он требует OpenAI/local model server, внешние tool services и example-specific data.

#### Что можем использовать в Еве

- **Environment lifecycle для локальных сценариев.** Адаптировать `reset`, bounded `step`, `is_done`, `update_state` и explicit `max_turns` в evaluation/scenario runner Евы. Каждый run должен иметь isolated fixture state, cancellation, deadline и durable terminal receipt.
- **Rule composition вместо жёсткого orchestration graph.** Разделить policy для `order`, `visibility`, `selector`, `updater`, `describer` и отдельно для role assignment, planning, execution и evaluation. В Core это должны быть typed Rust strategies, выбранные из allow-listed manifest, а не произвольные Python class paths.
- **Scoped message routing.** Перенять `sender`, `receiver` и typed message kinds как основу внутренней multi-agent communication модели. Core должен проверять receiver scope, capability и workspace/session boundaries; сообщение `all` не должно автоматически раскрывать секреты, hidden evidence или tool output всем агентам.
- **Private/public observation model.** Simulation visibility rules полезны для тестов: агент получает только разрешённые observations, а evaluator может видеть полный fixture state. Это даст сценарии проверки leakage, least privilege и prompt-injection через сообщение другого агента.
- **Role/pipeline separation.** Перенять отдельные role assigner, planner, critic, executor и evaluator stages как optional orchestration policy. Базовый single-agent path должен оставаться простым; multi-agent включается отдельной capability с concurrency, budget, trace и approval semantics.
- **Deterministic scheduling fixtures.** Реализовать sequential, round-robin, concurrent и priority scheduling с фиксированным seed, bounded parallelism и cancellation. Это пригодится для воспроизводимых тестов races, duplicate tool calls, order-sensitive memory и supervisor recovery.
- **Independent evaluator stage.** Отделить task result/effect validation от planner и executor. Evaluation может возвращать score/advice/status, но acceptance должен опираться на Core assertions: filesystem/database predicates, approval invariants, citations, no-side-effect checks и resource budgets.
- **Stage-level telemetry.** Для каждого этапа записывать role, prompt revision, input/output hashes, selected recipients, model usage, duration, retry count, tool/approval links и terminal status. Это естественно соединяется с AgentOps/OpenLLMetry findings без внедрения их Python SDK.
- **Typed message variants.** Зафиксировать Rust/IPC equivalents для plan, criticism, tool request, tool result, evaluator verdict, user instruction, system observation и error. Сериализация должна быть versioned и bounded, с сохранением unknown fields policy и replay compatibility.
- **Memory visibility/update policy.** Разделить append-to-memory, private self-memory, shared group memory, tool memory и evaluator-only state. Любое изменение long-term memory проходит текущие redaction/scope/forget rules, а не записывается автоматически каждым агентом.
- **Configurable retry and parser feedback.** Идею bounded retries при malformed model output можно использовать для provider gateway: invalid structured output получает typed feedback, остаётся в trace, но retry ограничивается budget/deadline и не превращается в бесконечный loop.
- **Tool-use evaluation.** AgentVerse examples показывают полезную проверку полного цикла plan → function/tool call → observation → evaluator. Для Евы добавить обязательные Core policy, approval, call_hash, sandbox и cancellation; tool name/arguments должны проходить schema validation до запуска.
- **Code-repair fixture pattern.** SDE team даёт основу для безопасного offline сценария: synthetic repository, fixed tests, writer/reviewer/tester roles, bounded iterations и feedback artifacts. Выполнять код только в supervisor-controlled ephemeral workspace, не в рабочей копии пользователя.
- **Cost and budget by role.** Раздельные token/model/temperature/max-token settings для ролей полезны как evaluation matrix. Нужны aggregate budgets per run/role, provider rate manifest и fail-closed behavior при исчерпании бюджета.
- **Plugin registry idea.** Перенять allow-listed registry для scenario/evaluator/policy implementations, но хранить schema/version/hash и строить только встроенные безопасные компоненты. Динамический import по строке и `eval`-подобные tool paths в production запрещены.

#### Ограничения и риски

- **Старый Python runtime.** `setup.py` требует Python >=3.9, но requirements pin OpenAI 1.1.0, LangChain 0.0.157, FastAPI 0.95.1, Pydantic 1.10.7 и старые auxiliary packages. Это не совместимо с Rust Core и создаёт значительный maintenance burden.
- **Simulation нестабилен.** Авторский README сообщает о продолжающемся refactoring и рекомендует `release-0.1` для стабильной simulation-only версии. Нельзя принимать текущую main как production contract или копировать её mutable state model.
- **LLM-driven roles не являются правами.** Role description, receiver set и evaluator score формируются конфигурацией/моделью и сами по себе не дают authorization. Capability, approval, workspace ACL и secret access должны оставаться Core-owned.
- **Message leakage.** `receiver={"all"}`, broad visibility и broadcast memory update легко раскрывают prompts, tool outputs, user data и hidden evaluator state. Для Евы default должен быть deny-by-default и explicit recipient list.
- **Неизолированные инструменты.** Tool agent вызывает LangChain `tool.run/arun`; task-solving examples подключают BMTools/XAgent ToolServer, browser, notebook и search. В репозитории нет сопоставимой с EvoHime supervisor policy, approval receipt, host sandbox или network egress governance.
- **Prompt parser вместо канонического action protocol.** Часть agents парсит свободный текст и regex, function-call path использует старый OpenAI functions format. Это нельзя переносить в production tool gateway без строгих JSON schema и canonical IPC contract.
- **Слабая durability.** Environment state, messages, memory, logs и results в основном живут в Python objects/обычных файлах; нет транзакционного SQLite event ledger, ordered replay, crash recovery или migration/forget contract.
- **Retry и exception handling.** Retry loops широко ловят exceptions и продолжают работу; `ToolNotExistError` наследуется от `BaseException`, что усложняет корректное завершение. Для Евы нужны typed failures, cancellation propagation, max attempts и explicit unknown/timeout.
- **Summary/reflection могут выдумывать факты.** Running summary, importance/immediacy, reflection и evaluator являются LLM-generated outputs. Их можно использовать как advisory projections, но нельзя принимать как immutable user memory, authorization или ground truth.
- **Mutable defaults и global registries.** В моделях есть list/set/dict defaults, а registry/config mutation выполняется через `pop`. Это повышает риск cross-run state leakage и делает concurrent/replay behavior менее предсказуемым.
- **External credentials and egress.** Стандартные examples требуют OpenAI/Azure keys; local modes требуют vLLM/FSChat; tool examples — отдельные servers. Все они расширяют network surface и могут отправить shared agent context внешним сервисам.
- **Cost accounting устарел.** Стоимость считается локальными hard-coded maps для старых моделей и не покрывает современный provider pricing, cache/reasoning tokens или missing usage. В Еве cost должен считаться Core по версионируемому manifest.
- **Неполная проверка результата.** Часть evaluators — LLM-based score/advice, а в `BasicEnvironment` есть произвольный threshold `8` для list scores. Такой verdict не заменяет deterministic assertions и safety evaluation.
- **Лицензии внешних компонентов.** Apache-2.0 распространяется на AgentVerse, но BMTools, XAgent, LangChain, OpenAI SDK, datasets, models и tool servers имеют собственные лицензии/условия. Их нельзя автоматически включать в EvoHime package.

#### Предварительное решение

`адаптировать` environment `reset/step/is_done`, composable rule strategies, scoped sender/receiver routing, visibility policy, role/pipeline separation, deterministic scheduling, independent evaluator, typed stage telemetry, bounded retries и code-repair fixtures; `наблюдать` за AgentVerse и его simulation refactor; `не подключать` Python runtime, dynamic import registry, BMTools/XAgent servers, direct LangChain tools, LLM-only acceptance и uncontrolled multi-agent autonomy в базовый runtime Евы.

#### Связь с EvoHime

- AgentVerse полезен для будущего optional multi-agent orchestration layer поверх Core: Electron отображает stages/participants/decisions через IPC, а Core владеет scheduler, messages, memory scopes, tools, approvals и evaluation.
- Возможные typed сущности: `agent_group`, `agent_role`, `agent_message`, `agent_visibility`, `agent_stage`, `agent_schedule`, `agent_tool_call`, `agent_verdict`; каждая должна иметь workspace/chat/run scope, parent/sequence IDs, schema revision и redaction outcome.
- В evaluation harness AgentBench можно добавить fixtures для hidden-message leakage, role reassignment, parallel ordering, critic feedback, tool denial, evaluator disagreement, cancellation и restart between stages.
- Multi-agent capability должна быть выключена по умолчанию, иметь общий run budget, per-agent context/cost limits, bounded concurrency, explicit user-visible plan и approval перед любым external side effect. Агентская роль не может расширить права пользователя.
- Базовые критерии: deterministic reset/replay; no cross-run memory leakage; receiver/visibility enforcement; no tool execution without Core policy and approval; bounded rounds/retries/tokens; cancellation reaches every stage; durable events before UI projection; evaluator is advisory unless backed by deterministic assertions; full forget/redaction across shared and private memories.

### 26. Sierra τ-bench

- **Источник:** [sierra-research/tau-bench](https://github.com/sierra-research/tau-bench), [README и предупреждение об устаревших задачах](https://github.com/sierra-research/tau-bench/blob/main/README.md), [environment contract](https://github.com/sierra-research/tau-bench/blob/main/tau_bench/envs/base.py), [user simulator](https://github.com/sierra-research/tau-bench/blob/main/tau_bench/envs/user.py), [актуальная линия τ³-bench](https://github.com/sierra-research/tau2-bench)
- **Дата проверки:** 2026-08-21
- **Ревизия/commit:** `59a200c6d575d595120f1cb70fea53cef0632f6b`; последний commit `2026-03-18`; checkout `main` чистый; `py -m compileall -q tau_bench` прошёл без ошибок
- **Лицензия:** MIT
- **Состав:** Python package `tau_bench 0.1.0`, Pydantic-типы задач/действий/наград, stateful environments для retail и airline, typed tool schemas, policy wiki/rules, LLM/human user simulators, tool-calling и ReAct-подобные agents, trajectory/checkpoint runner, Pass^k metrics и auto error identification
- **Назначение:** benchmark динамического взаимодействия `user simulator → tool-using agent → domain API` в реалистичных доменах, где результат зависит не только от финального текста, но и от состояния данных, последовательности вызовов и соблюдения policy
- **Краткий вывод:** это сильный reference для evaluation contract Евы: изолированная stateful-среда, typed actions/tools, policy как отдельный артефакт, expected action trace, проверка финального состояния, adversarial user tasks, повторные trials и cost/reliability telemetry. Прямое подключение не подходит: README помечает airline/retail задачи устаревшими, runtime Python/LiteLLM зависит от внешних LLM/API, user verification/reflection сами вероятностны, а benchmark tools не являются безопасным production execution boundary.

#### Что изучено

- `types.py` разделяет `Action`, `Task`, `RewardResult`, `SolveResult`, `EnvResponse`, `EnvRunResult` и `RunConfig`. Задача содержит user id, инструкцию, ожидаемую последовательность действий и expected outputs; результат содержит reward, подробный info, trajectory и trial.
- `Env` задаёт lifecycle `reset → step → done`, загружает отдельный domain state, policy/wiki, список tools и task fixture. Tool schemas индексируются по имени функции, а каждое действие становится частью trajectory.
- `step` различает финальный `respond`, доменный tool call, terminate/handoff tool и неизвестное действие. Исключение tool превращается в observation с ошибкой; terminate tool завершает run.
- Reward проверяет не только ответ агента: state hash вычисляется до и после, expected non-respond actions переигрываются как oracle, затем сравниваются итоговое состояние и ожидаемые строки ответа. В `RewardOutputInfo` и `RewardActionInfo` отдельно отражаются output/action reward и данные для диагностики.
- `to_hashable` и `consistent_hash` канонизируют вложенные dict/list/set и дают детерминированный SHA-256 fingerprint состояния. Это полезный минимальный паттерн для доказательства side effects, но не готовая замена typed assertions и SQLite receipts.
- LLM user simulator скрывает часть инструкции, выдаёт реплики по одной, поддерживает `llm`, `react`, `verify` и `reflection` стратегии. Verify добавляет отдельный judge-вызов, reflection может попросить модель исправить ответ и повторить его.
- User simulator поддерживает human mode и считает отдельную стоимость вызовов. Следовательно, benchmark оценивает не только agent model: поведение и цена второй модели являются частью экспериментальной конфигурации.
- Retail policy формализует подтверждение личности, запрет действий при неизвестном user id, явное подтверждение перед refund/address/cancel, запрет выдумывания данных, один tool call за раз и отсутствие одновременного tool call с user response. Tool description дублирует часть этих ограничений.
- Retail tasks покрывают смену решения после подтверждения, отказ сообщить данные, неверные утверждения пользователя, insistence, privacy concerns, несколько заказов и reactive users. Это хороший источник негативных и state-conflict сценариев.
- `run.py` запускает изолированные task indices, поддерживает task ids/split/seed/max steps/max concurrency, сохраняет checkpoint JSON, считает средний reward и Pass^k по повторным trial. Ошибки одного задания превращаются в диагностируемый failed result с traceback.
- Auto error identification классифицирует fault owner (`user`, `agent`, `environment`) и типы вроде `goal_partially_completed`, `used_wrong_tool`, `used_wrong_tool_argument`, `took_unintended_action`; авторы отдельно предупреждают, что LLM-классификация может ошибаться.
- README этого checkout прямо сообщает, что задачи airline/retail не обновлены, и направляет за исправленными задачами, banking-доменом и voice modality в [τ³-bench](https://github.com/sierra-research/tau2-bench). Поэтому текущая карточка оценивает базовые идеи tau-bench, а не рекомендует его dataset как источник истины.

#### Что можем использовать в Еве

- **Первоклассный evaluation environment.** Ввести для offline evaluation контракт `reset`, bounded `step`, `is_done`, terminal receipt и isolated fixture state. Для EvoHime это могут быть synthetic workspace/project/provider/approval сценарии, запускаемые Core без доступа к рабочему checkout пользователя.
- **Typed action/tool/task contracts.** Перенять разделение task instruction, canonical action, tool schema, observation, trajectory, reward info и run config. Реализация должна быть в Rust/Core и совместима с существующим IPC, а не встраивать Python models в runtime.
- **Проверку состояния, а не только текста.** Для каждого сценария задавать before/after state predicates: создан ли файл, изменился ли только разрешённый путь, появился ли approval receipt, сохранился ли call hash, не ушёл ли секрет, корректно ли обновился SQLite ledger. Финальный текст остаётся отдельной проверкой output contract.
- **State fingerprint как доказательство.** Использовать canonical state hash для fixture snapshots и side-effect diff, дополняя его typed field-level assertions, redaction checks и durable event IDs. Hash не должен скрывать важные различия или заменять audit trail.
- **Policy и tool schema как отдельные входы evaluator.** Держать human-readable policy, machine-readable constraints, dangerous-operation confirmation requirements и tool schema версионируемыми артефактами. Описание модели считается подсказкой; authorization, sandbox, approval и cancellation повторно проверяет Core.
- **Confirmation invariants.** Перенять retail-паттерн: identity/permission check → объяснение последствия → явное подтверждение → side effect → receipt. Нужны тесты на смену решения после подтверждения, stale state, повторное подтверждение, отказ от identity и конфликтующие инструкции.
- **Adversarial user/scenario library.** Добавить synthetic fixtures для privacy concerns, wrong IDs, user claims против фактического state, multitask requests, partial completion, repeated insistence, prompt injection в tool output и попыток обойти approval. Это должно быть детерминированным scripted user в CI; LLM simulator допустим только как дополнительный stress layer.
- **Trajectory и error taxonomy.** Сохранять bounded sequence `model decision → canonical tool request → policy/approval result → tool observation → user response`, а ошибки разделять на malformed action, wrong tool, wrong arguments, denied policy, unintended side effect, timeout/cancel и user/environment fault. Каждое событие получает run/task/step IDs и redaction outcome.
- **Pass^k и надёжность.** Использовать повторные trials с seed/model/provider/config fingerprint и считать не только средний success, но и Pass^k/consistency: несколько независимых запусков одного сценария должны безопасно завершаться одинаково. Отдельно показывать task success, safety, side-effect correctness, approval correctness, output quality, latency и cost.
- **Dual cost accounting.** Идею отдельного user-model cost перенять для evaluation: считать model input/output/reasoning/cache tokens и стоимость по каждому role/provider, но не смешивать эти данные с production budget. Runtime должен fail closed при исчерпании budget и не продолжать side effect из-за evaluator retry.
- **Checkpoint/replay.** Адаптировать checkpoint после каждого terminal/meaningful step, resumable evaluation и bounded concurrency. Replay должен работать по recorded canonical actions against a fixture snapshot без повторной отправки prompts или external side effects.
- **Handoff/termination как явный результат.** `transfer_to_human_agents` полезен как benchmark outcome: агент может корректно эскалировать, когда policy/authority/ambiguity этого требует. В Еве это связывается с approval/escalation state machine, а не с произвольным tool call.
- **Independent evaluator.** Отделить agent trajectory от evaluator, чтобы модель, которая планировала действие, не была единственным источником истины. LLM judge может классифицировать качество и fault hypothesis, но pass/fail по правам, state, approval, sandbox и side effects определяется deterministic Core assertions.

#### Ограничения и риски

- **Dataset устарел.** Сам README помечает airline/retail fixtures outdated и рекомендует τ³-bench (`tau2-bench`). Нельзя строить план интеграции на этих task definitions или принимать их leaderboard как актуальный baseline.
- **Внешняя LLM user simulation.** User, verify и reflection вызывают LLM; это повышает стоимость, недетерминизм и риск judge bias. Для EvoHime scripted users и deterministic fixtures должны быть базой regression suite.
- **LiteLLM/provider egress.** Пакет рассчитан на OpenAI, Anthropic, Google, Mistral, AnyScale и другие провайдеры; prompts, trajectories и synthetic state могут покидать машину. В Core production path внешний benchmark runner и его provider credentials не подключать.
- **Benchmark tool boundary слабее Core.** Tool invocation выполняется внутри Python environment, без сопоставимых с EvoHime supervisor Job Object, sandbox, approval receipt, named-pipe auth, cancellation propagation и durable SQLite event ownership.
- **Reward replay имеет побочные риски.** `calculate_reward` переигрывает expected actions через environment и может повторно мутировать in-memory state/user simulator; такой oracle нельзя без изменений переносить в production. В Еве evaluator должен работать на snapshot/transaction и не менять каноническое состояние run.
- **Есть дефект случайного выбора task.** В `Env` fallback для отсутствующего task index использует `random.randint(0, len(tasks))`, то есть верхняя граница включает несуществующий индекс. В обычном runner task index задаётся явно, но это подтверждает необходимость собственных bounds и тестов.
- **Ограниченный agent protocol.** Tool-calling agent использует только первый tool call, имеет жёсткий max step limit, а malformed JSON может завершить trial через exception. Для Евы нужен canonical structured action protocol, schema validation, typed parse errors, cancellation и bounded retry.
- **In-memory/domain fixture state.** Hash state и JSON checkpoints полезны для benchmark, но не дают SQLite migrations, crash recovery, ordered replay, retention/forget и redaction contracts. Эти гарантии остаются ответственностью EvoHime Core.
- **LLM fault attribution неточна.** Автоматическая классификация причин ошибки является диагностической гипотезой, а не доказательством. Не использовать её для security decisions, blame, автоматических permissions или изменения policy.
- **Дублирование policy в prompt и tool description.** Если правила расходятся с Core, модель может выбрать опасный путь. В Еве единственный enforcement — Core; prompts/tool descriptions лишь объясняют доступные действия.
- **Concurrency и checkpoint не равны production durability.** ThreadPoolExecutor и финальная запись JSON подходят для эксперимента, но требуют замены на durable run ledger, atomic transitions, resume semantics и per-run isolation.
- **Лицензионные и эксплуатационные условия.** MIT покрывает этот репозиторий, но модели, provider SDK, datasets и актуальный τ³-bench нужно проверять отдельно перед любым включением в package или redistribution.

#### Предварительное решение

`адаптировать` environment/task/tool/evaluator contracts, canonical state fingerprint, policy-driven confirmation fixtures, deterministic user scenarios, adversarial task library, trajectory/error taxonomy, Pass^k/reliability metrics, dual cost accounting, checkpoint/replay и explicit handoff outcome; `наблюдать` актуальный [τ³-bench](https://github.com/sierra-research/tau2-bench) как возможный внешний источник новых evaluation ideas; `не подключать` tau-bench Python runtime, LiteLLM user simulator, external provider calls, benchmark tools и outdated datasets в production Core.

#### Связь с EvoHime

- Для Core-first архитектуры Евы tau-bench задаёт полезный evaluation layer поверх model gateway и tool/approval contracts: Electron показывает итоговые metrics/trajectory projections, а Core владеет fixture state, policy, permissions, receipts и evaluator assertions.
- Минимальный будущий Rust contract может включать `EvalScenario`, `EvalTask`, `EvalAction`, `EvalObservation`, `EvalStep`, `EvalVerdict`, `EvalTrial`, `EvalFault` и `EvalRunConfig`; все сущности должны иметь schema revision, run/task/step IDs, scope, hashes, budget и redaction metadata.
- Рекомендуемый первый offline fixture: synthetic workspace task с чтением файла, изменением только разрешённого пути, обязательным approval перед записью, намеренно stale observation и финальной проверкой diff/hash/events. Затем добавить identity/privacy, cancellation, handoff и provider failure cases.
- В AgentBench/OpenLLMetry/AgentOps-derived observability можно связать `EvalTrial` с existing run/span/event ledger, но evaluator не должен менять execution path, обходить approval или сохранять raw prompts/secrets без explicit capture policy.
- Критерии готовности: deterministic reset/replay; state and output assertions; no unauthorized side effect; approval/identity invariants; bounded steps/retries/cost; cancellation; isolated snapshots; durable checkpoint; reproducible Pass^k report; redacted trajectory export; clear distinction between deterministic verdict and LLM diagnostic hypothesis.

## Итог для будущего плана

Этот раздел заполняется после завершения набора исследований:

- подтверждённые возможности для интеграции;
- идеи, которые реализуем самостоятельно без заимствования кода;
- внешние компоненты, допустимые после проверки лицензии;
- отклонённые варианты и причины;
- зависимости, порядок этапов и критерии готовности.
