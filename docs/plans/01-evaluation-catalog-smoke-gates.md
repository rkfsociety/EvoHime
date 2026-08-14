# План: Evaluation catalog и smoke-gates

Статус: draft для реализации после ревью.

## Цель

Создать повторяемую систему проверки агентного поведения на четырёх уровнях:
быстрые unit/contract tests, offline evaluation Core/model behavior,
security-gates и быстрые smoke-gates после сборки или запуска packaged desktop
runtime.

Smoke-тест проверяет работоспособность сборки и runtime, но не доказывает
качество ответа. Evaluation проверяет поведение и не заменяет security tests.

## Термины и границы

- `Core` — Rust runtime агента, единственный владелец состояния и выполнения
  инструментов.
- `fixture` — версионированный синтетический набор входных данных, ожиданий и
  ограничений для одного или нескольких evaluation cases.
- `case` — один воспроизводимый сценарий внутри fixture.
- `runner` — изолированный запускатель, который загружает fixture, выполняет
  case и формирует redacted trace и verdict.
- `model-dependent` — проверка, результат которой может зависеть от модели,
  провайдера, prompt или routing.
- `deterministic` — проверка с заранее фиксированным поведением и точными
  ожиданиями протокола.
- `smoke-gate` — короткая проверка packaged или установленного runtime.

Текущий продуктовый пакет ориентирован на Windows, однако verification для
macOS/Linux не запрещается: smoke запускается на каждой платформе, которая
входит в фактический target scope конкретного runtime или CI job.

## Политика verdict’ов и promotion

Runner использует следующие verdict’ы:

- `pass` — все обязательные assertions пройдены;
- `fail` — assertion или обязательный runtime-check не пройден;
- `blocked` — запуск невозможен из-за обязательной зависимости или окружения;
- `skipped` — case сознательно не запускался; обязательно указываются причина,
  владелец и срок пересмотра;
- `no_verdict` — результат нельзя надёжно интерпретировать;
- `flaky` — повторяемая нестабильность результата, не разрешённая как pass.

`fail`, `blocked`, `no_verdict` и `flaky` блокируют соответствующий gate и
promotion. `skipped` не превращается в pass: gate блокируется, если пропуск
не разрешён явной политикой с владельцем и неистёкшим сроком. Повторы имеют
фиксированный лимит; retry не должен скрывать flaky case.

Каждый CI summary должен явно показывать verdict, причину, категорию,
владельца skipped/flaky cases и ссылки на redacted artifacts.

## Каталог fixtures

Базовый каталог:

```text
tests/evals/
  schema/
  fixtures/
    tool-use/case_<id>.json
    workspace-study/case_<id>.json
    memory/case_<id>.json
    approval/case_<id>.json
    recovery/case_<id>.json
    rag/case_<id>.json
    security/case_<id>.json
    routing/case_<id>.json
    child-workflows/case_<id>.json
    concurrency/case_<id>.json
    long-running/case_<id>.json
    failures/case_<id>.json
  thresholds.toml
```

Начальные категории:

- `tool-use` — корректная схема, аргументы, retry после ошибки и правило
  параллельных tool calls;
- `workspace-study` — list/read/search порядок и citations;
- `memory` — retrieve, confirm, conflict, forget и scope isolation;
- `approval` — pause, deny, approve, replay и exact-call hash;
- `recovery` — Core restart, cancellation, lease loss и resume;
- `rag` — answer grounded in selected chunks;
- `security` — prompt injection, secret redaction, sandbox escape и policy
  boundaries;
- `routing` — local/cloud route, fallback, budget exhaustion и provider outage;
- `child-workflows` — handoff, typed report, timeout/loop и reject malformed or
  malicious output;
- `concurrency` — parallel tasks, shared resources и ordering;
- `long-running` — progress, cancellation, timeout и resume;
- `failures` — malformed tool output, disk full, DB corruption, permission
  violations и multi-workspace isolation.

Каждый fixture содержит `schema_version` и уникальный `fixture_version`.
Изменение несовместимой схемы увеличивает major-версию. Старые fixtures либо
переводятся проверенным мигратором, либо явно помечаются `deprecated` с датой
удаления. Изменение поведения не должно молча переписывать старый case:
fixture обновляется в том же review или создаётся новый `id`/version с
обоснованием.

Секреты, реальные пользовательские данные и восстанавливаемые персональные
данные запрещены. Для security cases используются только синтетические
секретные маркеры. До запуска и в CI выполняется fixture lint: pattern scan
секретов/PII, проверка schema, размера и допустимых ссылок.

## Формат case и ожидаемые данные

Каждый case содержит:

- обязательные `id`, `schema_version`, `prompt`, `expected_events`, `assertions`
  и limits;
- опциональные `workspace_fixture`, `required_tool_calls`,
  `forbidden_tool_calls`, `expected_final_state`, `model_profile` и
  `failure_policy`;
- `workspace_fixture` в стандартизованном виде: путь к синтетическому snapshot
  либо inline minimal state и его snapshot hash;
- ожидаемые события, policy decisions, tool calls с каноническими arguments,
  citations, approval state и final state.

Минимальный пример:

```json
{
  "id": "tool-use-001",
  "schema_version": "1.0",
  "fixture_version": "2026-08-14.1",
  "prompt": "Вычисли 2+2",
  "required_tool_calls": [
    {"name": "calculator.add", "args": {"left": 2, "right": 2}}
  ],
  "forbidden_tool_calls": ["filesystem.read"],
  "expected_final_state": {"answer": 4},
  "limits": {"max_events": 32, "max_trace_bytes": 65536, "timeout_ms": 5000},
  "assertions": ["tool_args_match", "final_state_match"]
}
```

`required_tool_calls` и `forbidden_tool_calls` проверяются отдельно. На первом
этапе параллельные tool calls либо явно запрещены case policy, либо описываются
как группы с допустимым порядком; runner не должен неявно превращать
параллельный вызов в последовательный.

Для memory/approval cases ожидаемое состояние должно явно задавать scope
isolation, записи/отсутствие записи, approval nonce, session/context и exact
hash. Для retry policy задаются `max_attempts`, backoff, идемпотентность и
инструменты, которые нельзя повторно вызывать.

## Deterministic fingerprint и redaction

Каждый run сохраняет fingerprint, включающий commit, fixture/schema version,
tool registry version, model/provider version, model route, seed, temperature,
prompt version и runner version. В trace не попадают raw prompts, memory bodies,
секреты и персональные данные.

Для `exact-call hash` используется каноническая сериализация с фиксированным
порядком ключей и типами. В hash входят tool name и arguments, workspace
snapshot hash, approval nonce, session/context и policy-relevant fields.
`timestamp`, `request_id` и иные transport-only поля исключаются явно и
перечисляются в спецификации hash.

Redaction проверяется автоматически во всех artifacts: logs, event trace,
errors, memory snapshots, IPC diagnostics и citations. Sandbox-escape cases
запускаются в отдельной изолированной fixture DB и временной директории, без
доступа к реальной системе.

## Этапы

### 1. Schema и runner

- Ввести JSON Schema с обязательными/опциональными полями, major-version,
  bounded counts/strings, максимальным trace size и timeout на case, suite и
  полный run.
- Реализовать Rust loader с проверкой schema, limits, fixture lint и
  canonical serialization.
- Runner должен изолировать fixture DB и временные директории между прогонами,
  включая параллельные запуски.
- Поддержать `static` mock model с cassette и `deterministic` mock model с
  фиксированной логикой. Real model mode допускается только для явно
  model-dependent jobs.
- Обрабатывать async events, streaming, cancellation и timeout; runner должен
  завершать runaway case и сохранять diagnostic artifact.
- Проверять выходные tool calls и arguments по схеме до сравнения assertions.
- Предусмотреть CLI-контракт для воспроизведения:
  `cargo eval --fixture <path> --case <id> --mode {static|deterministic|real}
  --model <name> --verbose`.

### 2. Offline evaluation

- На каждый Core change запускать быстрый deterministic набор; тяжёлые
  model-dependent matrix запускать nightly или при изменении model/provider/
  prompt/routing.
- Проверять в первую очередь event sequence, tool arguments, policy decisions,
  citations, memory writes и approval state; свободный текст ответа оценивается
  только дополнительными assertions структуры/ключевых фактов.
- Для protocol/security cases использовать exact deterministic assertions.
- Для model-dependent cases хранить thresholds в `thresholds.toml` отдельно по
  model/provider/route и поддержать режимы `strict` для PR/Gate B и `soft` для
  nightly matrix. Фиксировать baseline, метрики, aggregation method, sample
  size, latency/token/cost budget и критерий статистической значимости.
- Допускать шаблоны допустимой последовательности событий для параллельных
  операций, не разрешая произвольное нарушение policy order.
- Каждый production failure добавлять вручную через review как regression
  fixture с `source: "prod-incident-YYYY-MM-DD"` и одной командой
  воспроизведения.

### 3. Mandatory security evaluation

- Выделить security как обязательный gate, независимый от smoke и quality
  evaluation.
- Проверять indirect prompt injection через user prompt, файлы/workspace,
  tool results, RAG chunks и child-workflow output.
- Проверять secret redaction и permission boundaries во всех типах artifacts.
- Запускать sandbox escape, permission violation и destructive-tool cases только
  в изолированном окружении с синтетическими данными.

### 4. Smoke-gates

После `cargo build`, native package и installed launch выполнять на каждой
целевой платформе соответствующей CI job:

- подтверждение, что запущен именно packaged/installed Core, а не dev/mock;
- первый запуск и инициализация state;
- Core reachable и authenticated IPC;
- read-only task;
- approval-required task с approve/deny negative path;
- cancellation, offline/no-network path и controlled Core kill/recovery;
- upgrade и повреждённый state, если сценарий поддерживается пакетом;
- clean shutdown: exit code 0, отсутствие orphan processes/handles и ожидаемый
  набор final events с сохранением данных.

Smoke имеет отдельные timeout, suite/run timeout и cleanup policy: остановка
процессов, удаление временных папок, сброс тестового state и сбор логов при
падении. Failure artifact включает package version, commit, fixture version,
model route, redacted trace, logs, diagnostics и причину verdict.

### 5. Promotion policy

Gates выполняются последовательно:

- **Gate A** — compile, format/diff checks, unit и contract tests;
- **Gate B** — deterministic offline evaluation;
- **Gate C** — packaged/startup/IPC/recovery smoke на заявленных target
  platforms;
- **Gate D** — provider/model evaluation с baseline, thresholds и cost/latency
  budget;
- **Gate S** — обязательная security evaluation.

Gate D обязателен для изменений model, provider, prompt, routing или fallback.
Для чистых Core/protocol изменений он может быть `skipped` только с причиной,
владельцем и сроком; это не считается pass. Регрессия блокирует конкретный
model/provider route, а не маскируется общим зелёным результатом.

`no_verdict`, `fail`, `blocked` и `flaky` блокируют promotion. Gate summary
публикует commit, versions/fingerprints, fixture coverage по категориям,
counts, verdicts, duration, tokens/cost, flake rate, skipped reasons и ссылки
на redacted artifacts.

## Критерии готовности

- каждая новая agent feature имеет минимум happy-path и failure/edge-path
  fixture;
- каждый regression case имеет одну локальную команду воспроизведения;
- schema и fixtures проходят lint, version/hash review и не содержат
  секретов/PII;
- runner изолирует state и соблюдает case/suite/run limits;
- deterministic Gate B укладывается в согласованный CI budget, smoke — в
  согласованный startup/runtime budget;
- smoke проверяет реальный packaged Core, а не только mock;
- результаты не скрывают skipped, no_verdict или flaky под видом pass;
- security gate проверяет indirect injection, redaction и sandbox boundaries;
- CI artifacts redacted и достаточны для диагностики failure;
- изменения fixtures и thresholds проходят review с CODEOWNERS или эквивалентным
  контролем.

## Зависимости и порядок начала

Используются существующие Rust/Electron/native checks, event replay, approval,
recovery, diagnostics и IPC authentication. Перед реализацией нужно составить
матрицу готовности этих модулей: что уже доступно, что требует adapter, а что
нужно сначала стабилизировать.

Начинать с schema/runner и deterministic fixtures для `tool-use`, `memory`,
`approval`, `recovery` и `security`; затем добавить packaged smoke, provider
matrix и расширенные failure/concurrency cases. До реализации Agentic RAG/
receipts допустимо использовать минимальные grounded-RAG fixtures, не объявляя
полную функциональность уже реализованной.
