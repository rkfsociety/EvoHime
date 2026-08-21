# Сводка исследований для будущего плана EvoHime

Документ содержит только выводы, которые можно использовать при подготовке
будущего implementation plan. Подробности внешних источников, ссылки,
ревизии, лицензии и карточки исследований удалены.

Это не описание уже реализованного состояния продукта. Фактическое состояние
перед началом каждого этапа нужно сверять с кодом, `docs/current-state.md`,
`docs/architecture.md` и текущим `docs/development-plan.md`.

## Главный вывод

Новые возможности следует реализовывать как собственные контракты и модули
внутри существующей архитектуры:

```text
Electron renderer → Electron/main IPC → authenticated desktop IPC
                  → Rust Core → SQLite
                  → Windows supervisor
```

UI остаётся projection/control layer. Доверенное состояние, policy, выполнение
инструментов, журнал событий, memory и evaluation принадлежат Core. Внешние
идеи можно использовать как reference, но они не должны становиться вторым
runtime, второй базой данных или обходом Core.

## Что нужно строить

### 1. Базовые контракты выполнения — блокирующий фундамент

- Core-owned event ledger с устойчивыми `event_id`, `run_id`, `session_id`,
  монотонной последовательностью и временем;
- типизированные `ActionRequest`, `ToolCall`, `Observation` и `ToolReceipt`;
- связи `action_id ↔ tool_call_id ↔ observation`, provider/model response ID,
  error class и artifact references;
- отдельные состояния `running`, `paused`, `waiting_for_confirmation`,
  `finished`, `error`, `stuck`;
- разделение durable events и временных streaming deltas;
- replay после reconnect и явное поведение при устаревшей версии Core;
- атомарная запись результата и аудита в SQLite.

### 2. Policy, безопасность и границы возможностей — блокирующий фундамент

- `CapabilitySnapshot` для каждого run: разрешённые инструменты, workspace,
  network routes, browser sessions и лимиты;
- единый Core-owned resolver для абсолютных и относительных путей;
- проверка capability, scope, path, network, timeout и cancellation на каждой
  операции, независимо от решения модели;
- approval request, risk signal, explicit rejection и terminal receipt с
  причиной;
- preflight/postflight hooks для policy, redaction, telemetry и validation;
- bounded input/output, бюджет, дедлайн и лимит параллельности;
- секреты только через supervisor/DPAPI references, не через renderer,
  prompt или аргументы командной строки;
- модельный текст, reasoning и runtime context не являются доказательством
  разрешения на host action.

### 3. Совместимость и адаптеры — блокирующий фундамент

- typed `CoreInfo` с protocol major, build/runtime revision, capabilities,
  feature flags и limits;
- явное различие `unavailable`, `unsupported`, `unknown` и stale session;
- один transport adapter на каждом уровне: renderer → main IPC → Core IPC;
- запрет ad-hoc transport calls из UI;
- provider/worker adapter с versioned settings и capability discovery;
- credentials и workspace scope должны материализоваться только на доверенной
  стороне.

## Направления после фундамента

### Память и RAG

Сохранять существующий Local Agentic RAG/SQLite Core-first слой источником
истины. Улучшать его следующими контрактами:

- записи памяти с типом, scope, consent, provenance, source/evidence links,
  confidence и временем действия;
- разделение scratch state, текущего context и долговременной memory;
- retrieval по нескольким факторам с детерминированным tie-break;
- hybrid retrieval, локальный embedding cache и bounded context budget;
- explicit expiry, deletion и forget для всех проекций и embeddings;
- reflection и compaction как cancellable Core background operations с
  budget, snapshot revision и idempotency key;
- generated thought без evidence не становится фактом или trusted memory;
- plan preview отделён от выполнения побочных действий.

Первый полезный offline fixture:

```text
запрос → observation/tool receipt → retrieval с объяснением score
       → draft плана → approval-gated action
       → summary с citations
```

### Telemetry и evaluation

- локальная схема `run → model/tool → result/error`;
- token, cost, latency, timeout, cancellation и retry metrics;
- structured scenario, session, action, observation, final-state и reward;
- детерминированные fixtures, recorded inputs, replay и state predicates;
- adversarial user tasks, policy/approval checks и error attribution;
- повторные trials и reliability metrics, включая Pass^k-подобную оценку;
- report provenance и evidence links;
- LLM-as-a-Judge может быть advisory signal, но не единственным release gate;
- telemetry не должна автоматически отправляться во внешний сервис.

### Изолированный browser backend

Рассматривать только как отдельный permission-gated backend:

- отдельный BrowserContext на run;
- locators и actionability checks вместо координатных кликов;
- accessibility/DOM snapshot как evidence;
- network policy, redirect/SSRF checks и ограничение egress;
- trace, screenshot и artifact references;
- typed navigation/click/type/content/state receipts;
- отдельный packaging, security и lifecycle plan до включения в продукт.

Прямое управление всем desktop не включать по умолчанию.

### Голос и ambient audio

- frame/event contracts для realtime pipeline;
- typed STT → LLM → TTS lifecycle;
- endpointing, interruption/barge-in, cancellation и backpressure;
- streaming transcript events с segment/word timestamps;
- model manifest, preprocessing и quality fallback;
- optional speaker segmentation как offline enrichment, но не как доказанная
  identity;
- privacy permission, retention, deletion и bounded audio windows.

Текущий listener остаётся на `whisper.cpp`. Новый voice runtime допустим
только после отдельного PoC, проверки лицензий, памяти, GPU/CPU budget и
security boundary.

### Vision и document worker

Рассматривать как optional offline worker, а не как базовую capability Core:

- bounded image/video/document input;
- visual budget и лимиты страниц/кадров/разрешения;
- page/frame provenance и evidence references;
- OCR и multilingual visual QA;
- page-aware answers для многостраничных документов;
- benchmark fixtures и проверяемый quality fallback;
- запрет continuous capture и автоматических visual-agent actions до отдельного
  permission/security плана.

Прямое включение тяжёлого Python/CUDA/PyTorch runtime в Electron package или
Rust Core не допускается без отдельного решения по упаковке, ресурсам,
лицензиям и приватности.

### Workflow, automation и длительные simulation jobs

Рассматривать после базовых event, policy и persistence контрактов:

- Core-owned input queue и single-owner state machine;
- generation/lease protection от stale или overlapping runs;
- разделение tick/step и высокочастотных сообщений от durable state;
- operation lock для асинхронных provider calls;
- snapshots/diffs, архивирование завершённых сущностей и history separation;
- `AutomationDefinition`, trigger, run, activity log, health и cancellation;
- idempotency key, permission snapshot и approval policy для каждого запуска;
- UI только изменяет definition и показывает projection.

Непредсказуемая multi-agent autonomy не является базовой возможностью продукта.

## Порядок будущего плана

1. Зафиксировать event, action/observation, receipt, sequence replay и SQLite
   persistence.
2. Зафиксировать capability snapshot, path/network scope, approval,
   rejection, cancellation, timeout и redaction.
3. Добавить CoreInfo/version negotiation и typed adapter contract tests.
4. Довести memory/RAG contracts: provenance, retrieval, expiry и forget.
5. Добавить локальные telemetry/evaluation fixtures и replay.
6. Отдельными планами рассматривать browser, voice и vision workers.
7. После стабилизации базовых контрактов добавлять automation и длительные
   simulation jobs.

Блокирующая зависимость от более позднего этапа недопустима. Каждый этап
должен иметь собственную схему, тесты, миграции, security review и критерии
отката.

## Явно не включать в базовый runtime

- сторонние Python/Node agent SDK и второй execution runtime;
- cloud control plane, внешний telemetry/export backend и обязательный network
  egress;
- Docker или host-full-access режим вместо Windows supervisor и Core policy;
- публичный HTTP API вместо authenticated local IPC;
- browser extension и unrestricted desktop control;
- автоматическое запоминание всего transcript;
- трактовку speaker cluster как личности пользователя;
- модельные reasoning/text instructions как authority над filesystem, network
  или секретами;
- production side effects из benchmark/simulation окружений;
- неограниченную цепочку child agents и multi-agent autonomy.

## Общие критерии готовности

Функциональность не считается готовой, пока не подтверждены:

- Core/SQLite остаются единственным durable source of truth;
- все внешние действия проходят capability, scope, approval/policy и
  cancellation checks;
- схемы versioned и имеют contract tests;
- есть устойчивые IDs, atomic writes и sequence replay после reconnect;
- secrets, PII и чувствительный output redacted;
- действуют timeout, budget, bounded size и concurrency limits;
- есть deterministic fixture и replay из записанных входов;
- supervisor recovery не ломает состояние и аудит;
- renderer не может напрямую вызвать инструмент или изменить durable state;
- error, rejection, timeout и unknown model output представлены типизированно;
- для новых внешних компонентов отдельно подтверждены packaging, licensing,
  privacy, egress и maintenance cost.

## Что ещё нужно решить перед первым планом

- какой из фундаментальных контрактов становится первым этапом;
- какие текущие схемы IPC и SQLite расширяются, а какие остаются без изменений;
- нужен ли отдельный worker process для browser/voice/vision и какая у него
  граница capability;
- какие лимиты CPU, GPU, memory, disk, latency и retention считаются
  приемлемыми;
- какие features optional, а какие входят в обязательную поставку;
- какие fixtures и failure cases являются release gates;
- где будет вестись inventory лицензий и attribution для будущих компонентов.
