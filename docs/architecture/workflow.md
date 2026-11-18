# EvoHime — доменный контракт: Workflow, триггеры и фоновое исполнение

Канонический контракт домена workflow/v1, оркестрации, триггеров и durable background execution. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).
См. также: [../features/task-dependency-graphs.md](../features/task-dependency-graphs.md).

## Workflow orchestration

Составная задача описывается графом контракта `workflow/v1`
(`crates/evohime-core/src/workflow.rs`). Граф immutable после запуска: новая
версия создаётся целиком, а начатый запуск продолжает работать по своему
snapshot и его canonical hash (`canonical_json`/`canonical_hash` —
нормализованный JSON с отсортированными ключами, узлами и рёбрами).

Узел объявляет action profile: `research`, `transform`, `tool`, `condition`,
`approval`, `subgraph`, `loop`, `child`, `mcp_tool`, `context_provider`. Поля
идентичности (`tool_name`, `server_id`, `provider_id`, роль child, имя
маршрута, `block_id`) ограничены charset `[a-z0-9._:-]`, поэтому URL, путь или
команда физически не помещаются в identity; inline script, Python, shell и
dynamic import в контракте отсутствуют как понятие. Дополнительно узел несёт
`block` (стабильная identity возможности и её версия), `routes` (allowlist
исходящих маршрутов), `acceptance` (схема результата, минимум evidence,
разрешённые статусы, retryable-классы ошибок), `on_failure`, `join`,
`concurrency` и `batch`.

Разрешение идентичностей принадлежит Core-owned реестру
(`workflow_registry.rs`): каталог блоков с test fixtures, MCP-серверы с
транспортом, endpoint и allowlist инструментов, read-only контекстные
провайдеры с потолком свежести и объёма, список допущенных инструментов и
Core-owned подграфы. `WorkflowRegistry::validate_bindings` отклоняет неизвестный
блок, несовпадение версии или схемы блока, неизвестный инструмент или сервер,
инструмент вне allowlist сервера, недоступный транспорт (`transport_unavailable`
— поддержан только remote JSON-RPC поверх существующего Core tool `mcp_call`),
host вне `EVOHIME_MCP_ALLOWED_HOSTS`, провайдера сверх зарегистрированного
бюджета и любую попытку child получить grants, бюджет или контекст шире
родительских. `NodeType::Subgraph` — не nested child delegation: Core
разворачивает уже проверенный граф статически до запуска, наследует
`ExecutionPolicy` родительского узла и запрещает вложенные подграфы.

Библиотека шаблонов (`workflow_templates.rs`) — versioned definitions в коде
Core: `repository-research`, `plan-implement-review`,
`parallel-security-review`. Подстановка входов идёт только в свободный текст
(цель child, запрос провайдера, значения аргументов), поэтому вход
пользователя не может подменить capability. Шаблон объявляет
`schedule_eligibility`: сегодня supervisor-контракт умеет только
`once`/`interval`, а шаблон с обязательным approval помечен `unavailable`.

Отдельного раздела составных задач в Electron-навигации нет. Через обычный чат
Core предоставляет агенту закрытые инструменты `workflow_catalog`,
`workflow_start`, `workflow_status` и `workflow_cancel`; registry adapter
отказывает вне Core agent runtime. `child`-узлы исполняет Core-owned
`child_executor::RegisteredChildExecutor`: он выбирает только зарегистрированный
профиль роли (persisted Agent Role Profile или Core-owned built-in), повторно
проверяет `parent grants ∩ read-only policy ∩ profile ∩ requested`, берёт бюджет
ребёнка не больше остатка родителя и вызывает только read-only инструменты через
существующий `ToolRegistry`. Lifecycle, accepted report, projection и handoff
пишутся общим `child_workflow`-writer'ом, который использует и legacy IPC. Отчёт
проходит `Validating → WaitingParentAcceptance → Accepted`, сохраняется в child
store и попадает в fan-in как типизированный `TypedChildReport`; отдельного agent
loop, dispatch path или board не появляется. Дополнительный
делегированный hop разрешён только при queen-issued `DelegationGrant` и
глубине не больше `MAX_CHILD_DEPTH = 2`; потолок `max_children` выбирает
королева, рабочий порождает подагентов лениво в пределах этого потолка и
`HiveSettings`, параллельно не больше `max_parallel_subagents`. Глубина 3,
self-spawn и расширение потолка самим рабочим невозможны. `NodeType::Subgraph`
остаётся статическим
разворачиванием графа до старта. Произвольное редактирование графов через агентские инструменты не
разрешено. Доступ к durable run и отмене каждый раз ограничен каноническим путём
текущего workspace, а фоновые драйверы используют общий ограниченный пул задач.

Runtime (`workflow_runtime.rs`) durable: SQLite-схема 29
(`workflow_runs`, `workflow_run_nodes`, `workflow_node_attempts`,
`workflow_run_events`) ставится идемпотентно тем же способом, что receipts и
model provenance. Инварианты запуска:

- узел ставится в очередь только после получения и валидации всех обязательных
  входов; `batch` ограничивает итерацию по списку и не размножает исполнения;
- перед каждым эффектом заново сверяются graph hash, разрешимость capability по
  реестру и родительские возможности;
- dispatch marker пишется до эффекта и закрывается после него, поэтому падение
  Core даёт `unknown_outcome`, а не слепой повтор; восстановление снимает lease
  и переводит запуск в `interrupted`;
- параллельно выполняются только узлы, объявившие `concurrency = parallel` и не
  требующие approval, в пределах `budget.max_parallel_nodes`; stateful-узлы идут
  по одному;
- ошибка узла продолжает выполнение только по объявленной failure-ветви;
  неподключённая ошибка блокирует downstream и не маскируется под успех;
- исчерпанный повтор повторяемой ошибки даёт `dead_letter`, а неповторяемая
  ошибка — `failed`: это разные факты и разные терминальные состояния;
- недоступный или устаревший источник даёт `degraded`, а не уверенный ответ;
- события durable, монотонны внутри запуска и содержат только bounded
  projection.

### Workflow Package

Переносимость workflow реализована отдельным bounded JSON-контрактом
`evohime-workflow` v1 в `crates/evohime-core/src/workflow_package.rs`. Он
проецирует существующий `workflow/v1`, вычисляет deterministic SHA-256 hash и
принимает только явно отмеченные portable arguments; credential arguments
заменяются slot references, а неподтверждённые поля отклоняются fail-closed.
Runtime IDs, leases, approvals, checkpoints и secrets не являются частью
package content.

Core читает и пишет только `.evohime-workflow.json` до 1 MiB; export выполняет
atomic temp-to-final write. Package bytes не попадают в SQLite: отдельная
metadata-only таблица `workflow_package_imports` хранит hash, source
fingerprint, local identity/version, provenance и bounded phase outcome для
reconciliation. Preview не создаёт запись, capability, schedule, trigger или
run; commit повторно валидируется и deduplicates по content hash.

Authenticated IPC commands 169–172 (`Preview/Export/Commit/RebindWorkflowPackage`)
и Electron main/preload/renderer projection добавлены аддитивно. Renderer
показывает только bounded metadata/action state и не получает package storage,
SQLite или credential values.

Этот контур не следует путать с
[`../features/task-dependency-graphs.md`](../features/task-dependency-graphs.md):
там описан граф зависимостей work items проекта.

## Automation contract 16.1

Порядок зависимостей и открытые решения собраны в [`../decision-register.md`](../decision-register.md).
Общий release gate для automation и его host-boundary checks запускается из
`scripts/automation-release-gate.tests.ps1` и входит в Rust CI.
Rollback, redacted evidence, privacy/egress и license inventory собраны в
[`../release-evidence.md`](../release-evidence.md); отдельный gate проверяет их вместе
с backup/restore fixtures.
Финальный audit и rollback evidence описаны в `docs/release-evidence.md`;
проверка запускается `scripts/final-release-audit.tests.ps1`. Результат
конкретного запуска фиксируется только в release evidence и не переносится в
текущий статус без свежего CI-прогона.

Repeatable and scheduled work uses the separate Core-owned `automation/v1`
contract in `crates/evohime-core/src/automation.rs`; it does not replace the
`workflow/v1` graph or create a second lease owner. An immutable definition is
bound to `(definition_id, revision, owner_scope)` and contains bounded activity
references, trigger policy, concurrency/retry limits, capabilities, approval
mode, input schema and retention. Unknown contract versions and unsafe limits
fail closed, and its serialized definition hash is retained by every run.

`TriggerRequestV1` carries bounded correlation, scheduled-slot and input data.
`AutomationRunV1` captures permission and approval snapshots plus a fencing
generation before execution. `ActivityEventV1` and `AutomationHealthV1` are
bounded redacted projections; they never transport raw provider output. SQLite
`automation_definitions` and `automation_runs` are installed on the shared
database. The run uniqueness key is
`(owner_scope, definition_id, revision, idempotency_key)`: same key and payload
returns the original run, while a different payload is a typed idempotency
conflict. Queue, scheduler, lease and simulation behavior is deliberately
sequenced into plans 16.2 and 16.3.

Plan 16.2 adds `automation_runtime.rs` as the single Core owner of automation
FSM transitions, bounded command queue and coalesced progress, fencing leases,
operation locks and effect-boundary revalidation. Durable transitions and
events are written by `automation_store` in one SQLite transaction guarded by
`(run_id, generation, state)`. A stale runner gets `stale_generation` and
cannot publish an event; expired leases can be taken over only with a higher
generation. Provider operations have a 120-second deadline, cooperative
cancellation and an allow-list of transient retry codes.

Plan 16.3 adds `automation_simulation.rs`: bounded schema-1 snapshots carry
definition revision, fencing generation, event sequence, policy/approval
snapshots, provenance and a SHA-256 checksum. Validation rejects oversized,
corrupt, stale or incompatible snapshots before recovery. `ReplayInputV1`
produces a deterministic hash from frozen clock, seed, ordered events, inputs,
fixtures and capability/policy snapshots. The simulation effect allow-list
admits only the fake provider; host filesystem/network/process/IPC effects are
denied, and export redaction removes bearer markers and absolute Windows paths.
Snapshot records are stored separately in `automation_snapshots`; active state
and event history remain authoritative.

Acceptance fixtures A01–A08 live in `automation_acceptance.rs` and run with a
frozen trigger slot, provider fixture, replay input and policy snapshots. They
cover bounded trigger/queue behavior, stale fencing, cancellation and retry
classification, snapshot/replay equality, simulation redaction, history
limits and effect-boundary revalidation. Scheduler timezone/missed-tick,
durable cursor and additive Electron automation IPC are covered by the
automation boundary and release evidence gates.

### Continuation Policy v1 (план 26, текущий implementation slice)

Continuation Policy отделяет Goal и Workflow от решения о следующем bounded
шаге. Core-контракт находится в `crates/evohime-core/src/continuation.rs`:
`ContinuationPolicyV1` валидирует owner/workspace scope, actor, immutable
revision, typed gate arguments, limits и domain-separated SHA-256 hash. Pure
decision table имеет hard-stop precedence для user stop, approval и unknown
outcome; client-supplied completion evidence не является доверенным входом.

Durable state хранится в `continuation_policies`, `continuation_runs` и
`continuation_attempts`, которыми владеет
`crates/evohime-local-storage/src/continuation_store.rs`. Migration v36
устанавливает таблицы транзакционно. Attempt fingerprint уникален внутри run,
а turn/token/cost reservation выполняется до effect в SQLite-транзакции. Run
закреплён за task и idempotency key; после перезапуска незавершённые runs
переходят в blocked и не повторяются автоматически. Authenticated IPC добавляет
policy/run/stop/pause/resume messages tags 151–156; Electron
получает bounded response payload и отображает Core-owned run projection в
`ContinuationPanel`. Renderer не отправляет evidence и не меняет counters.

Встроенный task bridge резервирует и фиксирует каждый проход с Core-owned
decision; неизвестный/ошибочный результат fail-closed. Полная интеграция
реестров Tool/Workflow/Approval и воспроизводимые fault-injection fixtures
остаются обязательным runtime/evidence срезом плана 26; наличие storage или
панели не считается завершением задачи.

### Visual Workflow Builder v1 foundation

Builder-контракт находится в `crates/evohime-core/src/visual_workflow_builder.rs`.
Typed `workflow/v1` graph validation, независимые execution/layout hashes и
additive draft/version/handoff schema остаются Core-owned; IPC 173/event 33 и
Electron surface передают только bounded metadata. Authoring, immutable
publish, owner-scoped single-use handoff, recovery и read-only live inspection
обслуживаются Core; renderer не получает полномочий, credentials или raw
runtime payload.

### Conversational Workflow Composer v1

Composer добавляет natural-language authoring поверх Builder v1. Контракт
`composer-request/v1`/`composer-proposal/v1` находится в
`crates/evohime-core/src/conversational_workflow_composer.rs`: Core принимает
только bounded closed envelope, валидирует proposal и registry binding, а
модель не получает tool/effect authority. Команда Composer аддитивна (IPC 174,
event 34); Electron показывает только redacted proposal metadata, assumptions,
risk и typed outcome.

Proposal и unsaved session не являются отдельным durable workflow. При Save Core
создаёт Builder draft с optimistic revision; provenance хранится в общей
Builder storage атомарно и содержит только request/proposal/catalog hashes и
model route/version. Raw prompt, raw model output, credentials и hidden
reasoning не сохраняются. Handoff/publish повторно сверяет owner, revision и
execution hash до consume; stale handoff отклоняется, а Composer никогда не
запускает workflow effect автоматически.

### Event Trigger Runtime v1

Settings → «Триггеры событий» управляет versioned Core-owned правилами,
закреплёнными за точными `template_id`, версией и execution hash workflow.
Контракт и bounded admission policy находятся в
`crates/evohime-core/src/event_trigger_runtime.rs`: allowlisted mapping,
Core-local authenticity, дедупликация, лимиты очереди/частоты и typed outcomes.

`system_event` принимает только durable journal события `task.completed` и
`task.failed`; watch channel служит сигналом пробуждения, а содержимое задачи
не переносится в payload триггера. `local_workspace_event` на Windows использует
`ReadDirectoryChangesW` по явно выбранной папке, рекурсивно, с bounded каналом
128 уведомлений и лимитом 32 активных корней. При остановке/паузе правила watcher
отменяет ожидающее Win32 чтение. Повторные уведомления одного пути и типа в
750-миллисекундном debounce окне объединяются. Workflow запускается через
обычный `WorkflowRuntime`, поэтому его capability checks и approval остаются
обязательными. Core не запускает повторный файловый триггер для изменений в
рабочей папке, пока там выполняется workflow, созданный триггером; такой
пропуск получает `dropped_with_audit`.

Durable metadata schema v41 находится в
`crates/evohime-local-storage/src/event_trigger_runtime_store.rs`; она не
содержит credentials, raw task output или содержимое файлов. Authenticated IPC
commands 177–178 и event 36 подключены к Electron; Settings показывает список,
редактирование, включение/паузу, pinned workflow и bounded историю событий.
Provider webhook остаётся `unavailable` без production adapter.

## Composable Termination Conditions v1 (план 63)

Core-owned termination policies compose the thirteen bounded conditions
`MaxMessages`, `MaxTurns`, `MaxToolCalls`, `TokenBudget`, `CostBudget`,
`WallClockTimeout`, `IdleTimeout`, `StopEvent`, `SourceMatch`,
`HandoffReached`, `ExternalSignal`, `GoalStateReached` and
`WorkflowStateReached` through `Any`/`All`. Policies are schema-versioned,
canonical-hash validated and depth/node bounded; they never grant model,
tool or capability authority.

Evaluation is stateful and replay-safe: durable state keeps the event cursor,
counters, terminal outcome, stable reason code, bounded evidence references
and the first triggering condition/event. Duplicate or stale events cannot
double-count or reopen a terminal state. Hard-stop decisions take precedence
over continuation, while unknown/ambiguous external results remain explicit.
SQLite schema v61 stores policy and state with optimistic version fencing;
authenticated additive IPC command 214/event 60 and the Electron panel expose
only bounded metadata and validation/evaluation results. Renderer does not
evaluate conditions or own termination authority.

The follow-up routing contract is also Core-owned: guarded Team Coordinator
proposals evaluate the pinned termination policy/state/event before candidate
selection. A terminal decision fails closed with a typed coordinator outcome;
the evaluation projection includes only bounded reason and counter metadata.

## Runtime Intervention Pipeline v1 (план 69)

Core-owned pipeline поддерживает typed hooks для model request/response,
agent messages, tool dispatch/results, handoff, workflow commit и external
publish. Handlers упорядочиваются по phase/priority/stable id и возвращают
явные Allow/Modify/Deny/PauseForApproval/Abort decisions. Built-in policy
handlers fail closed; mutations имеют safe hashes/patch metadata без secret
значений, approval всегда revalidated Core, а intervention depth ограничивает
reentrancy. Authenticated additive IPC command 220/event 65 и Electron
developer panel передают только bounded diagnostics.

## Workflow Optimization Lab v1 (план 71)

Offline-only lab хранит versioned OptimizationRun/Candidate и declarative
mutations в SQLite schema v68. Candidate evaluation проходит через
Core-owned Agent Benchmark Matrix с frozen suite/policy; train/validation/
holdout разделены, security regression блокирует результат, а лимиты run
ограничивают rounds, candidates, cost, tokens и wall time. Promotion возможен
только явным Core-checked action после holdout и не активируется автоматически.
Authenticated additive IPC использует command 222/event 67; Electron показывает
bounded metadata projection без optimizer tools или production authority.

## Core Topic/Subscription Event Bus v1 (план 72)

Локальный Core bus маршрутизирует typed Event с first-class correlation и
causation по exact/namespace-prefix/type selectors. Ephemeral subscriptions
не обещают restart delivery; correctness-critical durable events сохраняются в
SQLite schema v69 с bounded queue/in-flight policy, ACK/NACK, максимум тремя
попытками, dead-letter и crash reconciliation в `unknown`. Publish/subscribe
проверяют capability и не дают renderer unrestricted access или внешнего broker.
Authenticated additive IPC использует command 223/event 68, Electron получает
только bounded metadata projection.

## Dependency-Aware Task Graph v1 (план 73)

Core-owned `TaskGraph` хранит versioned `ExecutionTask` и typed
`TaskDependency`, валидирует bounded DAG, unknown references, cycles и grants
ceiling, а ready-set вычисляется детерминированно. Semantic patch из bounded
операций применяется атомарно с optimistic revision fencing; completed tasks
immutable, а изменения инвалидируют только downstream-ветви. Граф хранится в
SQLite schema v70, после restart восстанавливается из durable state.
Authenticated additive IPC использует command 224/event 69, Electron получает
только metadata-only projection и не вычисляет state machine или не запускает
effects.

## Runtime Stall Guard (план 90)

`scripts/runtime-stall-guard.ps1` выполняет только чтение исходников и строит
bounded JSON report по известным sync filesystem/process/sleep/network/database
API. Каждое finding получает стабильный fingerprint; suppression допускается
только с явной причиной. Detector не исполняет найденный код, не сохраняет
absolute paths/arguments и не является доказательством регрессии. Windows CI
отдельно запускает smoke gate; подтверждённые runtime anchors остаются
обычными focused tests production paths.

## Batch Invocation Runtime v1 (план 112)

Batch Invocation Runtime — Core-owned durable map-контур для одного immutable
definition по bounded списку validated inputs. Schema v78 хранит versioned
`BatchInvocation` и независимые `BatchItem`: input hash, стабильный item/run
ref, attempts, result/error refs и Core status. Policy ограничивает до 256
items, concurrency до 16, payload до 64 KiB и attempts; `start` выдаёт только
bounded набор Running items, а optimistic version защищает transitions.

Restart переводит незавершённый Running item в `unknown_after_restart` и не
повторяет внешний effect вслепую; Pending items можно продолжить, а явная
reconciliation-запись результата закрывает unknown. SQLite store использует
additive transactional schema и optimistic fence. Authenticated additive IPC
command 253/event 98 и Electron panel передают только redacted aggregate и
per-item metadata; raw input, credentials и capability authority в projection
не входят.

## Durable Background Execution Plane v1 (план 132, реализован 2026-09-09)

`background-execution/v1` расширяет существующий Core-owned `automation/v1` и
не создаёт второй источник истины для runs, schedules, leases или событий.
`automation_store` остаётся владельцем definitions/runs/events и получает
аддитивную schema v103; bounded metadata плоскости хранится в
`automation_queues`, `automation_waits`, `automation_wakeups` и
`automation_attempts`, а run snapshot — в дополнительных колонках
`automation_runs`. Definition, environment, execution/approval policy,
queue/concurrency refs и idempotency identity фиксируются до запуска; raw
prompt, provider output, credentials и executable paths не сохраняются.

Контракт типизирует lifecycle `Accepted`/`Scheduled`/`Queued`/`Dispatching`/
`Running`/`Waiting`/`RetryScheduled`/`Blocked` и terminal states, absolute и
duration/run-state/human waits, `OneShotAt`/`Interval`/`Cron`, timezone,
missed-fire/overlap/overflow/priority policies, bounded queue limits и
canonical snapshot hash. Wakeups восстанавливаются из SQLite после restart;
transition использует фактическую generation и не теряет wakeup при конфликте.
Crash в dispatch/running переводит immutable attempt в explicit
`reconcile_required`/`unknown_after_restart`; отсутствующий runtime adapter
возвращает `runtime_adapter_unavailable` и не выдаётся за успешный эффект.
Фоновый poll рассчитывает deterministic fire slots для `OneShotAt`/`Interval`/
`Cron`, применяет bounded missed-fire policy, сохраняет idempotent schedule
fire в тот же `automation_runs` и выставляет cursor CAS-операцией. Операция
`upsert_queue` публикует queue limits, priority и overflow policy; при
переполнении новые admissions отклоняются или возвращают typed backpressure,
а coalescing не превращается в повторный эффект.

Authenticated additive IPC использует command tag 262 и event tag 107 с
schema/request/owner scope, expected revision, idempotency и bounded redacted
projection. Electron не предоставляет отдельную вкладку `Background Execution`;
renderer запрашивает Core-owned runs, schedules,
queues, waits и attempts, но не владеет storage или verdict. Существующие
`WorkflowRuntime`, Agent, Goal, Human Work Queue и Durable Remote Task Bridge
остаются единственными effect owners.
