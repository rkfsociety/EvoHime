# EvoHime — Windows desktop architecture

Статус: текущая утверждённая архитектура продукта. Фактическое состояние реализации см. в [`current-state.md`](current-state.md).

EvoHime — локальное Windows-приложение.
Пользовательское короткое имя агента — «Ева».

```text
EvoHime.exe               Electron main + bundled renderer
        │ preload/contextBridge → desktop-ipc-v1 / named pipe
evohime-core.exe          agent loop, model gateway, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, restart, JSONL diagnostics
        │
evohime-transaction.exe   transactional update worker
```

Renderer не имеет node integration, не выполняет shell-команды и не открывает базу. Electron main ограничен окном, lifecycle, локальным состоянием оболочки и IPC adapter. Core владеет workspace, инструментами, моделью, секретами и локальным состоянием. Supervisor запускает core в Job Object и завершает дочернее дерево при остановке.

Ревью планов — отдельный read-only pipeline Core. Electron main выбирает и
ограниченно читает Markdown-файл через native dialog, затем передаёт его Core.
Core вызывает 2–8 моделей текущего provider catalog по очереди, по одному
запросу за раз, чтобы не упираться в лимиты провайдера, и затем отдельную
synthesis-модель. Per-request model overrides сохраняются. Состав и порядок
рецензентов фиксируются на момент запуска: неудачное обновление каталога
возвращает пустой список и трактуется как «нет новостей», а не как «нет
моделей», поэтому уже выбранные модели не теряются. Исходный Markdown
ограничен 512 КБ, ответ каждого рецензента — 256 КБ. На ревью можно подать
несколько файлов сразу (диалогом или перетаскиванием): оболочка склеивает их в
один документ с нумерованными разделами и проверяет суммарный размер. Перед
запуском объём запроса сверяется с окном каждой выбранной модели: заведомо не
влезающий план блокирует запуск, а худший случай синтеза (план плюс все ответы
рецензентов) остаётся предупреждением. Review не получает tools,
не изменяет workspace и сохраняется в локальном event journal без credentials;
история ревью очищается отдельной командой и исчезает из UI сразу, а не после
перезапуска.

Правка плана по ревью — второй шаг того же pipeline и такой же read-only на
стороне модели: один вызов synthesis-модели получает исходный план и текст
ревью и возвращает переписанный план целиком. Диффа не запрашивается — модели
надёжнее воспроизводят документ, чем адресуют куски. Текст ревью Core берёт из
своего кэша или журнала, а не из запроса оболочки, поэтому выдать за ревью
произвольный текст нельзя. Результат живёт в памяти Core до отдельной команды
сохранения, показывается пользователю целиком и записывается только по его
решению — поверх оригинала или в новый файл; расширение `.md` проверяет Core.
Правка работает по одному файлу: склеенный из нескольких планов документ
нельзя однозначно разложить обратно.

WinUI 3 больше не является пользовательской оболочкой пакета. Он сохранён как
временный compatibility runtime и oracle для совместимости IPC до отдельного
решения о его удалении.

## Оболочка

Renderer состоит из панели проектов и чатов, ленты диалога и инструментальных разделов.

| Поверхность | Назначение |
| --- | --- |
| `ProjectSidebar` | проекты (workspace) и чаты внутри проекта; аккаунт и вход в настройки внизу |
| `HomeScreen` | стартовый экран; первый запрос сам создаёт чат |
| `TaskTimeline` + `ActivityLine` + `transcript.ts` | ход задачи, свёрнутый в читаемую ленту; ответы агента рендерятся Markdown |
| `tool-names.ts` | русские подписи инструментов вместо служебных идентификаторов |
| `RepositoryBar` | ветка и счётчики изменений открытого репозитория |
| `ModelPicker` | выбор модели в чате; каталог разделён на free и paid |
| `ProviderForm` | единственная поверхность настроек провайдера (ключ, модель, base URL) |
| `PlanReviewPanel` | коллективное read-only ревью Markdown-плана несколькими моделями и synthesis-моделью; итог копируется в буфер или экспортируется в Markdown, история очищается кнопкой |
| `RecoveryBanner` + `recovery-state.ts` | состояние восстановления, выведенное только из подтверждённых Core событий |
| `OperationsPanel` | очередь подтверждения памяти и конфликты (только metadata), плюс read-only проекция child- и schedule-событий |
| `OverviewPanel`, `TracePanel` | сводка событий запуска и фильтруемая трасса |

Бизнес-логики в renderer нет: он отображает состояние, полученное через IPC, и отправляет команды.

## Специализированные child workflows

Child workflow является Core-owned orchestration boundary. `TypedChildTaskRequest`
и `TypedChildReport` имеют версию контракта, correlation ID и parent sequence;
Core повторно проверяет JSON Schema, размер, provenance и grant subset при
создании, каждом tool-call и при fan-in. Роли ограничивают capability loadout:
implementer может писать только в выданной области, tester — выполнять тесты,
reviewer получает summary-only и не имеет права записи. Отказ сохраняется в
audit без исходного payload.

Состояния проходят только через `Created → Queued → Running → Validating →
WaitingParentAcceptance → terminal`; lease хранит wall-clock и monotonic
deadline, boot id и holder process id. Transport retry ограничен тремя
попытками, revision — тремя, а dead-letter хранится 30 дней. SQLite migration
24 добавляет `coordinator_child_checkpoint` и атомарный per-parent sequence;
терминальные lease очищаются идемпотентным sweep. После перезапуска checkpoint
повторно проверяется по lease/boot и provenance, а fan-in детерминированно
отмечает superseded evidence и unknown conflicts.

Контекст child передаётся только по выбранному allowlist. Большой report можно
вынести в Core-owned `ArtifactStore` только с явным флагом; Sensitive/Secret
offload запрещён. Чтение artifact каждый раз проверяет текущую parent chain,
scope grant и выбранный context, поэтому renderer не получает raw transcript.
`OperationsPanel` отображает typed projection timeline (role, state, revision,
budget, lease, reason, dead-letter) и отделяет trace projection от audit.

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
— поддержан только remote JSON-RPC поверх существующего Core tool `mcp.call`),
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

Адаптеры (`workflow_adapters.rs`) ведут узлы в уже существующие контуры:
`child` — в `TypedChildTaskRequest`/report, `tool` и `mcp_tool` — в
`ToolRegistry` с тем же approval path, `context_provider` и `research` — в
read-only источники с проверкой свежести, `condition`/`transform` — в
deterministic-операции Core.

Approval узла решается существующей командой `ResolveApproval` и тем же
approval registry, что и у инструментов: отдельного workflow-approval нет.

Этот контур не следует путать с
[`features/task-dependency-graphs.md`](features/task-dependency-graphs.md):
там описан граф зависимостей work items проекта.

## Automation contract 16.1

Порядок зависимостей и открытые решения собраны в [`decision-register.md`](decision-register.md).
Общий release gate для automation и его host-boundary checks запускается из
`scripts/automation-release-gate.tests.ps1` и входит в Rust CI.
Rollback, redacted evidence, privacy/egress и license inventory собраны в
[`release-evidence.md`](release-evidence.md); отдельный gate проверяет их вместе
с backup/restore fixtures.

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
limits and effect-boundary revalidation. The fixtures do not claim scheduler
or Electron IPC behavior that is not wired to automation yet; those remain
explicit integration follow-up before a release gate can be called green.

## IPC

Контракт находится в `crates/desktop-ipc/proto/evohime.desktop.proto`.

- major-версия несовместима, minor-расширения совместимы;
- фреймы ограничены 4 MiB;
- события имеют монотонный `sequence_id`;
- UI может запросить replay после последнего sequence ID;
- cancellation передаётся отдельной командой `StopTask`;
- `SelectModelRequest` меняет модель следующего запроса без перезапуска Core: gateway разрешает модель на каждый вызов, пустое значение возвращает модель маршрута;
- `CancelDatabaseOperation` кооперативно отменяет выполняющийся backup или restore;
- `ClearPlanReviewHistory` удаляет сохранённые ревью планов из локального журнала; Core отвечает подтверждением, и UI перестаёт показывать историю немедленно;
- `RevisePlan`, `StopRevision` и `SaveRevisedPlan` правят план по готовому ревью. `RevisePlan` подтверждается сразу, результат приходит событием `task.completed` с `task_id` вида `revision-<uuid>`, прогресс — событием `revision.progress`. `SaveRevisedPlan` пишет файл сам и принимает только путь с расширением `.md`: запись — единственный шаг, где правка покидает память Core. Отказ сохранения приходит событием `plan.save_failed`, а не ошибкой кадра: ошибка кадра рвёт соединение с оболочкой, и опечатка в имени файла читалась бы как падение ядра. Правку, которой уже нет в памяти, Core ищет в журнале — перезапуск Core при обновлении не должен отнимать возможность сохранить готовый текст;
- `StartPlanReview` и `RevisePlan` принимают пути проверяемых файлов (`source_paths`, `source_path`). По ним Core сам читает соседние планы, на которые проверяемый ссылается Markdown-ссылками, и кладёт их в промпт отдельным блоком: план этапа почти никогда не самодостаточен, инвариант соседа в нём только упомянут ссылкой, и модель, не видя соседа, уверенно переписывает план вразрез с ним. Обход идёт вширь на `MAX_CONTEXT_DEPTH` шагов (этапы связаны не напрямую, а через обзорный файл), берёт только `.md` по относительным ссылкам внутри каталога исходного плана (канонизированные пути сверяются с каталогом, поэтому симлинк наружу не проходит) и ограничен `MAX_CONTEXT_DOCUMENTS` файлами и `MAX_CONTEXT_BYTES`. Читает ядро, а не оболочка: иначе за соседний план можно было бы выдать произвольный текст. Единственное место этой файловой операции — `crates/evohime-core/src/plan_context.rs`; `plan_review.rs` остаётся без файловой системы и часов. Пустой путь — не ошибка: план мог прийти перетаскиванием из источника без файловой системы, и правка тогда идёт по одному файлу, а `RevisionResult.context_files` показывает пользователю, что сверки не было;
- команды workflow `ListWorkflowTemplates`, `GetWorkflowDefinition`, `StartWorkflow`, `GetWorkflowRun`, `CancelWorkflow` и `ListWorkflowEvents` аддитивны: клиент, который их не знает, согласовывает ту же major-версию и просто их не шлёт. `StartWorkflow` требует ключ идемпотентности — повтор возвращает первый запуск, а не создаёт второй. Ответы несут только bounded projection: идентификаторы, состояния, роли, коды ошибок и номера событий; prompt, цель child-узла, сырой вывод инструмента и содержимое контекста через IPC не проходят. Approval узла решается общей командой `ResolveApproval`;
- команды памяти `GetMemory`, `ListMemoryPending`, `GetMemoryConflicts`, `ConfirmMemory`, `RejectMemory`, `SupersedeMemory`, `ReviseMemoryCandidate` аддитивны. `ListMemory`, `SearchMemory` и `ListMemoryPending` возвращают только metadata; тело записи доступно исключительно через явный `GetMemory` и маскируется для `sensitive` и забытых записей. Confirm/reject/supersede требуют approval-токен и idempotency key: повтор безопасен и возвращает фактическое состояние записи.

### IPC CoreInfo, adapter boundary и target lifecycle

`Ready.core_info` — аддитивное поле `CoreInfo`; старый Core без него остаётся
валидным legacy peer, а старый consumer игнорирует неизвестное поле. При
наличии CoreInfo Rust и Electron валидируют bounded identity/capabilities и
вычисляют effective limits как `min(local, peer)`; до Ready действуют только
локальные hard limits. `core_instance_id + session_epoch` — Core generation,
`sequence_id` — journal revision, `target_generation` — target revision.

Core-owned provider/worker вызовы проходят через внутренний Rust-only
`adapter/v1` contract (`crates/evohime-core/src/adapter_contract.rs`):
descriptor, immutable session, bounded request/result, typed status и opaque
`SecretRef`. Это не второй wire transport и не provider catalog. Реальный
`CoreNodeAdapter` валидирует descriptor/session и bounded tool input/output до
и после dispatch; raw secret, path и prompt не входят в adapter projection.

Target identity использует существующий canonical `workspace_scope_id`, route,
backend и Core generation; raw path/secret в target ID не попадают. Atomic
`TargetManager` сериализует switch, отклоняет stale expected generation и не
принимает late result старого target. Retry/fallback разрешены только в
immutable same-target snapshot; после switch/restart результат получает
`stale_session` либо `unknown_outcome` без blind retry. Обновление provider
credentials остаётся shell-local `provider.save/clearKey` → encrypted persist →
supervisor/Core restart → `{ summary, restarted }`.

## Model gateway и routing

Core является владельцем model gateway и принимает routing-решения после
проверки privacy, offline, approval/tool policy, capability, health, evaluation
gate и budget. User preference передаётся только как hint внутри разрешённого
множества; renderer не выбирает provider и не может обойти Core policy.

Целевой контракт плана 02 состоит из четырёх последовательных этапов:

1. provider contract — capability metadata, immutable policy snapshot, health и
   bounded retry/circuit contract;
2. local provider — loopback-only модель под lifecycle и Job Object supervisor;
3. routing и budget — подключение selection/execution к agent run, fallback,
   versioned redacted trace и terminal result;
4. UI — read-only проекция фактического route и trace-состояния через desktop
   IPC.

Все этапы сохраняют tool permissions, approval requirements, sandbox и privacy
границы при fallback. Trace и enum-контракт для UI версионируются; malformed
payload или несовместимая major-версия не интерпретируются renderer частично.
Реализация этапа 02 завершена. Snapshot/overlay selector, loopback local
provider, supervisor-owned adapter lifecycle в Job Object, authenticated
Core↔supervisor command channel, bounded fallback/approval workflow,
versioned runtime catalog, redacted replayable trace и typed routing UI
подключены к agent loop. Временные планы этапа удалены после переноса
контракта сюда и подтверждённого состояния в [`current-state.md`](current-state.md).

## Signed receipts

Canonical Receipt v1 реализован в `crates/evohime-receipts` и Electron main
consumer `desktop/evohime-electron/src/main/receipt-crypto.ts`. Нормативные
JCS bytes, envelope `receipt_hash`, Ed25519, result domain, schema, limits,
stable error codes и cross-language vectors находятся в
`contracts/receipts/v1/`; подробное правило — `docs/security/receipt-canonical-v1.md`.
Этап 01.1 фиксирует bytes и проверку контракта. Key lifecycle реализован в
`crates/evohime-receipts`: Windows DPAPI CurrentUser, owner-only DACL,
SQLite-источник переходов и audit, journaled rotation/recovery, explicit
trusted genesis, signed checkpoint contract и `evohime-verify.exe`. Core
публикует renderer только bounded status/key metadata; private material не
выходит из Core. JSONL history является post-commit snapshot с manifest и
статусом stale при ошибке экспорта. Runtime orchestration 01.3 теперь
выполняется Core-owned `ReceiptRuntime`: mutation path использует durable
`pre_action` до dispatch, approval хранится как bounded one-shot intent, а
terminal post/refusal append-ятся в SQLite hash-chain. Startup recovery
устанавливает guard, истекает старые approval intents и переводит незакрытые
вызовы в `pending_recovery`; raw input/result в receipt runtime не сохраняются.
Для восстановления результата Core предоставляет authenticated
`ReconcilePendingReceiptAction`: он создаёт новый read-only action с собственным
hash/call binding и атомарно связывает его с историческим action; исходный tool
повторно не запускается. `ClosePendingReceiptAction` закрывает только explicit
unknown-result как signed refusal, а authenticated `UnquarantineReceiptAction`
проверяет trusted signed checkpoint и закрывает только invariant violation как
refusal. Protected recovery rows шифруются
AES-256-GCM, а storage-key rotation выполняется возобновляемыми bounded batch с
durable cursor. Read-only sampling, recovery state и bounded runtime counters
доступны только через Core diagnostics.
Retention/compaction receipt chain по-прежнему относится к отдельному этапу
01.4.

## Policy, capability snapshots и approval gate

План 09 реализован поверх `ReceiptRuntime`, без второго execution runtime или
отдельного approval store. `evohime_receipts::capability::CapabilitySnapshotV1`
создаётся для действия как bounded immutable contract: он содержит session/run/
task identity, policy и manifest hashes, operation/tool scopes, network и adapter
references, opaque secret purposes и budgets. Snapshot получает
domain-separated canonical `snapshot_hash`; child snapshot допускается только как
subset родителя. Raw secret values, prompt и необрезанный tool input в snapshot
не попадают.

`crates/evohime-core/src/policy_gate.rs` предоставляет единственный Core-owned
`preflight`/`recheck_before_effect` gate. Перед effect он повторно сверяет
canonical call hash, normalized scope, tool identity, snapshot hash и policy
version. Terminal IPC, обычный ToolAgent и workflow adapter проходят этот gate;
прямой production-вызов Tool Runtime после approval использует только
`execute_after_durable_approval` после atomic durable claim. Старый in-memory
approval wrapper существует только в unit-test compatibility code.

SQLite receipts дополнены additive таблицами
`receipt_capability_snapshots` и `receipt_policy_decisions`, а также nullable
session/snapshot/policy/hook linkage columns в action и approval intent. Это
сохраняет чтение старых баз и позволяет durable различать `allowed`,
`approval_required`, `denied`, `unavailable`, `expired`, `cancelled`,
`policy_error` и `unknown_outcome`.

Approval rejection записывается как terminal durable state и не может быть
повторно повышен до grant. Claim проверяет session, snapshot и policy version;
изменение canonical call, scope или policy делает approval stale. Bounded
preflight/postflight hooks получают только action/tool/input/snapshot hashes и
typed outcome metadata. Acceptance покрывает отказ до side effect, повторную
доставку, drift, redaction и recovery; старые compatibility clients продолжают
читать прежние receipt decision values.

## Core-owned execution ledger

Единая typed-история выполнения поверх существующего append-only `events`
журнала, `receipts_v1` и workflow runtime (план 08) реализована в
`crates/evohime-local-storage/src/execution_ledger.rs` (чистый contract-слой),
storage-методах `LocalDatabase` (`crates/evohime-local-storage/src/lib.rs`) и
IPC-проекции в `crates/evohime-core/src/ipc_bridge.rs`.

Контракт: `ExecutionEventV1` — versioned typed событие с `event_id`,
`(run_scope, run_id)` (`workflow`/`work_item`/`standalone`/`system`/`legacy`,
однозначно указывает на `workflow_runs.run_id` либо `runs.id`), `session_id`,
correlation-полями (`action_id`, `tool_call_id`, `receipt_id`,
`workflow_run_id` и др.) и bounded typed `body`
(`ActionRequest`/`ToolCall`/`Observation`/`ToolReceipt`/`TypedFailure`/
`ApprovalDecision`/`Cancellation`/`RecoveryDecision`). Состояния действия
(`ActionState`) один в один совпадают со словарём
`workflow_store::NodeState` плюс новое нетерминальное `Cancelling`; терминал
достижим ровно один раз — `assert_single_terminal`/`ensure_single_terminal_outcome`
отклоняют вторую терминальную запись для того же `action_id` уже на уровне
записи в SQLite, а не только как in-memory проверка.

SQLite schema поднята до v30 идемпотентными installer'ами (тем же путём, что
receipts/model provenance/workflow store — без отдельной ветки `migrate()`,
которая не выполняется для уже смигрированных v26+ баз): `events` получила
nullable typed-колонки (`event_id`, `run_scope`, `run_id`, `session_id`,
`action_id`, `effect_id`, `workflow_run_id`, `state_after`) с partial unique
индексом на `event_id`; `workflow_run_nodes.state` CHECK пересобран под
`cancelling`; `workflow_run_events` получила `ledger_sequence_id`/
`ledger_event_id`, атомарно связывающие bounded per-run projection с
глобальной durable последовательностью. `LocalDatabase::append_ledger_event`
и `append_ledger_event_with_node_transition` публикуют typed event и (во
втором случае) переводят `workflow_run_nodes.state` и добавляют строку
`workflow_run_events` одной транзакцией — незаконный переход или устаревшее
исходное состояние откатывают всё целиком, включая уже выполненный UPDATE.

Startup reconciliation (`reconcile_ledger_on_startup`, вызывается из `main.rs`
сразу после конструирования `IpcBridge`, вместе с bounded `core_start`
событием под текущим `core_instance_id`) классифицирует незавершённые typed
actions по наличию dispatch marker в `run_effects`: marker отсутствует —
action остаётся как есть (уже resumable по контракту); marker открыт
(started, не completed) — публикуется новое `unknown_outcome` событие,
блокирующее слепой повтор; исходная строка никогда не переписывается вторым
терминальным исходом.

IPC: additive `ExecutionEvent` (oneof поле 14 в `EventEnvelope`,
`crates/desktop-ipc/proto/evohime.desktop.proto`) проецирует typed ledger
rows в основном replay-пути (`push_journal_tail`) без изменения generic
`event_type`/`payload` — старый клиент безопасно игнорирует незнакомое поле
oneof. `ReplayGap` заполняется честно (`sequence_retention_exceeded` и новый
`stale_generation`, обнаруживаемый сверкой `core_instance_id`/`session_epoch`
из `CommandEnvelope` с текущей identity моста), `FullSnapshot.snapshot_json`
несёт versioned action-проекцию (`schema_version`, `core_instance_id`,
`session_epoch`, `snapshot_sequence_id`, bounded `actions`). Electron
(`desktop/evohime-electron/src/main/ipc/pipe-client.ts`) дедуплицирует
доставку typed событий по durable `event_id` (`LedgerEventDedup`,
переживает смену Core generation, в отличие от `sequence_id`).

Реальные production writers: оба пути диспетчеризации терминальных tool call
(`execute_terminal_with_receipt`, `dispatch_terminal_execute` в
`ipc_bridge.rs`) публикуют полную цепочку `ToolCall` (Running) →
`Observation` (bounded digest вывода) → терминальный `ToolReceipt`
(Succeeded, со ссылкой на реальный `receipt_hash` из `receipts_v1`) или
`TypedFailure`/`UnknownOutcome`; `ResolveApproval` и обнаружение истечения
approval-окна внутри `grant_approval` публикуют `ApprovalDecision`
(`Approved`/`Rejected`/`Expired`) под тем же `action_id`, что и
`receipt_approval_intents`. Redaction-метаданные (`secrets_present`)
вычисляются реальным сканом запроса теми же маркерами, что уже использует
audit log (`crate::audit::contains_secret`), а не всегда пустой заглушкой.

Не входит в реализованный контракт: живой cancellation-триггер для
`dispatch_terminal_execute` (его `CancellationToken` не подключён ни к какой
команде — существовавший до плана 08 пробел, не расширение полномочий
задачей плана; сам ledger-контракт `Cancelling`/`Cancelled`/`Cancellation`
полностью реализован и покрыт storage- и IPC-уровневыми тестами, просто
дожидается живого источника события).

## Context Budget Manager

Сборка контекста реализована в `crates/context-budget` (контракты и детерминированная логика), `crates/evohime-local-storage` (ledger, scratchpad, artifact store, команды) и `crates/evohime-core/src/context_budget.rs` (интеграция в agent loop). Этот раздел — канонический контракт: исходный план удалён из `docs/plans/` после реализации, как того требует правило каталога.

**Контур.** Перед каждым model call Core выполняет `selection -> compress/offload -> финальная проверка бюджета -> событие ModelContext -> вызов модели`. Финальная проверка обязательна и выполняется до формирования события; при её невыполнении Core проходит оставшиеся уровни лестницы, а после их исчерпания завершает вызов через `BudgetUnavailable` без обращения к модели.

**Бюджет и профиль.** `ModelContextProfile` версионируется и выбирается по provider/model из каталога `crates/context-budget/profiles.json`, который можно перекрыть пользовательским конфигом того же формата. Профиль обязан удовлетворять правилам валидности `0 < target < soft < hard <= max`, `target + reserves <= soft` и `absolute_mvc_max_limit + reserves <= hard`; невалидный профиль отклоняется при загрузке, а неизвестная модель получает fallback-профиль (60% / 75% / 85% окна) и не может обойти эти ограничения. `target_tokens` — цель сокращения, `soft_limit_tokens` — порог его запуска, `hard_limit_tokens` — граница отказа; резервы считаются сверх контекста и не могут быть заняты историей или схемами.

**Обязательный минимум.** `minimum_viable_context` вычисляется детерминированно и всегда включает safety/system policy и текущий user prompt; approval semantics, незавершённый tool-call и cancellation добавляются при наличии таких состояний. Порядок частей фиксирован и задаёт как порядок в собранном контексте, так и выбор `missing_part`. Safety- и approval-часть не сокращается никогда: конфликт «safety не влезает в бюджет» разрешается отказом от вызова, а не урезанием safety.

**Лестница сокращения** конечна и упорядочена: L1 expired/duplicate/superseded, L2 low-priority optional, L3 самые старые завершённые tool outputs, L4 offload крупных item в artifact store, L5 сжатие истории, L6 отказ от необязательных резервов (`retry` → `streaming` → `tool_schema`; `tool_call` и `final_answer` не сокращаются никогда). Каждый уровень применяется не более одного раза и обязан строго уменьшать размер, поэтому цикл завершается всегда. Внутри уровня порядок детерминирован: pinned последним, затем по возрастанию `effective_priority`, `created_at`, `content_hash` и `id`. Недоступные artifact store или summarizer пропускают L4/L5 с diagnostic, а не роняют сборку.

**Отказ сборки.** `BudgetUnavailable` — терминальный результат со стадиями `mandatory_overflow`, `drops_exhausted`, `estimator_unavailable` и `provider_replan_failed`. Автоматический retry запрещён на всех уровнях; context-length error провайдера даёт ровно один deterministic re-plan с уменьшенным `hard_limit_tokens`, повторный отказ каскада не порождает. До UI отказ доходит bounded причиной с кодом, стадией, требуемым и доступным объёмом и указанием непоместившейся части — не молчаливым обрывом ответа.

**Оценка токенов.** Estimator версионируется и обязан быть консервативным: занижение считается дефектом. При недоступности основного используется fallback-estimator (`ceil(utf8_bytes / 2) + 16`) с порогами профиля, масштабированными на 0.70; при недоступности обоих сборка завершается отказом, а не оценкой по умолчанию. Оценка кэшируется по `content_hash` вместе с версиями tokenizer, нормализатора и chat-template, поэтому смена любой из них не даёт стухший кэш-хит.

**`content_hash`** — SHA-256 в строчном hex от `normalizer_version`, разделителя `0x00`, `kind`, `0x00` и нормализованного содержимого. Текст нормализуется в фиксированном порядке: UTF-8, NFC, перевод CRLF и CR в LF, удаление завершающих пробелов в строках и завершающих пустых строк. JSON приводится к канонической форме с сортировкой ключей и фиксированным представлением чисел, двоичное содержимое хешируется как есть. Правила зафиксированы эталонными векторами в тестах, и версия нормализатора входит в hash input, а не только в кэш-ключ.

**`context_ledger`** — одна immutable запись на один model call. Её hash покрывает ids и порядок выбранных item, версии profile, tokenizer, нормализатора и стратегии, обязательный набор, отброшенные item с причинами, применённые compression- и ladder-решения, fallback-флаг и loadout. Hash считается один раз после фиксации состава и публикуется до вызова модели, поэтому потребители сравнивают его с записью, а не пересчитывают контекст. Фактический usage провайдера пишется в отдельную append-only таблицу `context_ledger_usage`, чтобы запись оставалась hash-стабильной. Ротация хранит записи моложе 30 дней или принадлежащие последним 200 сессиям и не удаляет записи, на которые ссылается неэкспортированный receipt.

**Scratchpad задачи** делится на `facts`, `open_questions`, `decisions`, `tool_findings` и `next_actions`. Внешний вывод инструмента помещается в `data_not_instructions` envelope и проверяется на prompt-injection; `confirmed` запись появляется только после provenance/policy-проверки Core, явного подтверждения пользователя или завершённой policy-операции — успешный tool result сам по себе фактом не становится. Подтверждённая запись не перезаписывается на месте, только новой ревизией. После restart в рабочий контекст возвращаются только `confirmed`; остальные изолируются как `recovered` с `trust=unverified` и пониженным приоритетом и удаляются через час или 10 шагов. При переполнении категории бюджета самые старые `confirmed` записи выгружаются в artifact store и остаются в контексте bounded ссылкой с hash и locator; `open_questions` и обязательный минимум не вытесняются, молчаливое усечение запрещено.

**Artifact store** адресует содержимое по `content_hash`: повторный offload переиспользует артефакт и добавляет ссылку, а не копию. Пространство имён per-task, доступ по locator ограничен задачей-владельцем и её детьми, чтение заново сверяет hash и помечает ссылку `invalid` при расхождении. Вытеснение идёт по TTL и последнему обращению; ссылка живого ledger entry или confirmed записи scratchpad помечается `expired` с сохранением hash и размера, а удалённое содержимое оставляет tombstone, который не считается доступным dedup-hit.

**Compression и pruning.** `duplicate` — совпадение `content_hash`, `superseded` — новая ревизия того же ключа при другом содержимом, `expired` — истёкший TTL или retention. Иерархия прав: safety и approval выше system instructions, те выше явных ограничений пользователя, далее confirmed facts, history и данные инструментов, ниже всего recovered и unverified. Recency и trust решают исход только внутри одного уровня. Summarizer — отдельный Core-вызов того же model gateway с собственным `summary_budget` и входным лимитом на prompt, без инструментов и без повторов; недоступность, превышение бюджета или невалидный результат дают deterministic fallback без каскадного повтора. Исходные item остаются source of truth, а summary хранит связь `summary_id -> source_ids`.

**Tool loadout.** Инструменты делятся на обязательные, read-only и mutation. Deterministic intent router нормализует prompt и активные `open_questions`, сопоставляет их с versioned таблицей capability keywords и применяет deny-правила; при конфликте правил выбирается более безопасный read-only результат, при неопределённом intent — read-only fallback loadout. Обязательные инструменты входят всегда и расходуют отдельный `mandatory_schema_reserve`, остальные ограничены `tool_schema_reserve`. Permission и approval semantics выбранного инструмента остаются видимыми, а вызов вне loadout Core отклоняет до эффекта с bounded diagnostic `loadout_miss`.

**IPC и UI.** Событие `ModelContext` расширено additive-полем `context` с bounded projection: бюджет, ids выбранных item, причины сокращения, `context_ledger_hash`, compression summary, loadout и отказ сборки. Старый клиент игнорирует неизвестное поле, major bump не требуется. Команды `GetContextLedger`, `ListTaskScratchpad`, `ClearTaskScratchpad`, `SummarizeContextNow`, `PinContextItem` и `ReadContextArtifact` аддитивны; каждая mutation получает запись аудита и подчиняется rate limit, посчитанному по журналу и потому переживающему перезапуск Core. `SummarizeContextNow` действует только на текущую task-scoped сборку и не меняет долговременную память. `PinContextItem` повышает приоритет, но не гарантирует включение: при нехватке бюджета pinned item отбрасывается последним и с явной причиной. `ForgetMemory` каскадно удаляет производные заметки и task artifacts, сохраняя redacted факт удаления. UI не получает prompt, тело памяти, raw tool output и неограниченные списки ids.

## Local Agentic RAG

Локальный индекс workspace реализован в `crates/evohime-core/src/workspace_rag.rs`, миграции и данные находятся в общей Core-owned SQLite (schema v19), а команды проходят через authenticated desktop IPC. Исходный план 01 удалён после реализации; этот раздел является каноническим контрактом.

**Граница безопасности.** Перед чтением scanner канонизирует workspace и каждый путь, запрещает абсолютные/parent/UNC escapes, symlink/reparse traversal, встроенные secret paths (`.env*`, ключи, `secrets/`, `.git`, build/vendor directories) и patterns из `.ragignore`. Canonical path и metadata проверяются до и после bounded read. Секретный, binary-looking, oversized, minified или нестабильный файл не создаёт chunks. Renderer не читает filesystem/SQLite, не выбирает embedding backend и не может расширить scope; retrieval никогда не является разрешением на действие и не ослабляет sandbox, permission или approval.

**Публикация индекса.** `workspace_index_runs`, `workspace_documents`, `document_chunks` и `workspace_chunks_fts` хранят отдельные поколения. Run получает производный от canonical root `workspace_key`, строит новое поколение в состоянии `running`, проверяет отсутствие orphan/ghost FTS rows и короткой транзакцией переводит прежнее `published` поколение в `superseded`, а новое — в `published`. Отмена, timeout, crash, unstable snapshot или ошибка не меняют published pointer; незавершённый run при следующем старте становится `failed`. На workspace разрешён один run: параллельная команда получает bounded lease error, `CancelWorkspaceIndex` кооперативно отменяет scanner/vector build. После успешной публикации остаются текущее и одно предыдущее поколение.

**Scanner и chunker v1.** Defaults валидируются до run и ограничивают размер/число файлов, длину строки, chunks на документ/run, размер chunk, retry, timeout и частоту progress. Поддержаны README/Markdown, Rust, TypeScript/JavaScript, JSON, TOML, YAML и plain text; UTF-8 и UTF-16 распознаются явно, lossy decode разрешён только для неструктурного текста. Markdown режется по заголовкам, код — по структурным boundary с детерминированным fallback, structured text — по ключам/блокам, остальное — bounded recursive chunks. `file_hash` — SHA-256 исходных bytes; `chunk_hash` — SHA-256 versioned payload из текста, parent context, language и chunker version, без offset. Incremental run копирует неизменившийся snapshot, а изменившийся файл перестраивает; byte offsets относятся к исходным bytes, line range всегда перепроверяется по свежему файлу.

**FTS5 retrieval.** SQLite FTS5 использует trigram tokenizer для content, normalized symbol, path и parent context; metadata scope по workspace/generation/path/language применяется отдельными индексами. Три лимита удовлетворяют `max_retrieval_chunks >= max_evidence_chunks >= max_context_chunks`; целый файл никогда не попадает в prompt из-за одного совпадения. Ranking использует фиксированные веса BM25 и tie-break `score -> path bytes -> document id -> byte start -> chunk id`. Каждый результат содержит path, byte range, свежие lines или `null`, file/chunk hash, score explanation, `stale` и redaction status. Перед возвратом Core заново проверяет canonical path, size, file hash и range; stale/full-redacted content не передаётся модели.

**Planner и checker.** Каноническая strict Draft 2020-12 schema лежит в `crates/evohime-core/schemas/workspace-query-plan.schema.json`. Pre-check без LLM выбирает `exact_symbol`, `lexical`, `path` или `metadata`, ограничивает запрос восемью terms и не меняет security filters. Bounded loop выполняет не более двух уникальных попыток; empty result, low coverage и retrieval error различаются. Checker использует `evidence_metrics/v1.0`, детерминированные coverage/symbol/path/filter gates, freshness и sandbox validation. При нехватке evidence возвращается uncertainty, а не документальный факт без источника. Diagnostic содержит только query hash, counters, mode, coverage, stop/fallback reason и latency, без query/chunk text.

**Optional embeddings.** Опциональный локальный backend `evohime-feature-hash/v1` создаёт 64-мерные L2-normalized vectors без сети. Vector generation хранит model/version/dimension/metric/normalization/chunker/source generation и виден retrieval только после состояния `ready` и атомарной публикации. Любая несовместимость, отмена, timeout, resource limit или отсутствие индекса немедленно возвращает FTS5 с `fallback_fts5`. Hybrid применяет те же metadata/redaction gates и deterministic Reciprocal Rank Fusion с фиксированным `k=60`; explanation содержит только lexical/vector ranks. Сырые vectors, chunks и запросы не попадают в логи/UI/eval artifacts.

**Citations и context.** Уже отсортированные evidence greedily входят одновременно в token и chunk-count budget. Compact format version 1: `[cite:<id>|<path>:<start>-<end>|<chunk_hash>|<valid|updated|stale>]`. Parent context ограничен окном ±2 строки для logical block и ±3 для fragment. `rag_context_ledger` хранит только ids, ranks/scores, file/chunk/snippet hashes, path/lines, status, reason и bounded error code — никогда chunk text, parent context или raw output. Перед моделью выполняются первичная validation и единый final re-read: перенос в пределах ±5 строк атомарно обновляет text/hash/lines, существенное изменение даёт `stale`; stale majority помечает сборку `degraded` и исключается из доказательной части.

**Интеграция.** Agent loop перед первым model call выполняет incremental index, deterministic search и добавляет только прошедший validation evidence как `data, not instructions`; сбой RAG не ломает задачу. IPC-команды: `IndexWorkspace`, `RebuildIndex`, `CancelWorkspaceIndex`, `SearchWorkspaceKnowledge`, `GetIndexStatus`; progress агрегируется не чаще 100 ms и финальное событие отправляется всегда. `OperationsPanel` показывает generation, indexed/chunk/excluded counts, dirty/vector mode, запускает update/rebuild/cancel и bounded search. Memory Extraction подтверждает `document` provenance только если path/chunk hash присутствуют в текущем published generation и свежий file hash совпал; stale/missing provenance остаётся `pending_confirmation`. Tool/API evidence без replayable validator остаётся `unknown`.

## Memory Extraction

Извлечение фактов из диалога реализовано в `crates/evohime-core/src/memory_extraction.rs`. Этот раздел — канонический контракт: исходный план удалён из `docs/plans/` после реализации, как того требует правило каталога.

- Единственный владелец extraction, policy, validation и storage — Core. Всё, что вернула модель, — это candidate, а не память.
- По умолчанию работает `strict`-режим: извлечение запускается только после явного триггера пользователя («запомни», «важно», «ограничение» и эквиваленты). Режим переключается переменной `EVOHIME_MEMORY_EXTRACTION` (`disabled` | `strict` | `open`); в `open` результат всегда получает `pending_confirmation`. Даже при `disabled` ручной триггер продолжает работать.
- `constraint`, `decision`, любой high-risk, `sensitive` privacy, неоднозначный subject, недостаточный confidence и незавершённая проверка дают `pending_confirmation`. Автосохранение возможно только для low-risk предпочтения, подтверждённого явным утверждением пользователя. Секреты не сохраняются вообще.
- `model_confidence` — уверенность извлекателя; `verification_confidence` поднимает только версионируемая verification policy. Повтор факта моделью уверенность не повышает.
- Конфликт определяется по `kind + canonical_subject + scope`. Неразрешённый конфликт оставляет старую запись активной, а новую — pending; supersede происходит только по явному выбору пользователя и хранит причину из закрытого набора.
- Extraction выполняется после отправки ответа, поэтому не добавляет задержки к ходу задачи, а недоступность модели или валидатора не ломает задачу.
- Кандидата можно изменить до подтверждения или оставить только на текущую сессию (`ReviseMemoryCandidate`). Правка делает запись пользовательским утверждением и сбрасывает прошлую проверку, но ничего не подтверждает; session-only не создаёт persistent row и живёт до автоматического expiry.
- `forget` — logical deletion с tombstone из одних metadata и digest; он же вращает backup-контейнеры старше 7 дней, потому что стёртое утверждение остаётся в снимках, снятых до удаления.
- **Ambient как источник кандидатов.** `SourceTrust::Ambient` — пятое значение доверия, строго более слабое, чем остальные: `can_ground_strict_save()` для него ложно, `requires_validation()` истинно, а `evaluate` возвращает `pending` с причиной `ambient_never_auto_confirms` сразу после проверки на секрет — раньше любых порогов, kind и scope. Перебор всех комбинаций `kind × scope × privacy × confidence × subject` показывает, что `AutoConfirm` для ambient недостижим. Секрет по-прежнему отвергается раньше: услышанный ключ не сохраняется даже в pending.
- **Ambient-точка входа отдельная.** `run_memory_extraction` принимает пару (реплика пользователя, ответ агента) одного хода, поэтому речь входит своим путём `run_ambient_memory_extraction`, а не подделанным «ходом»: подмена реплики пользователя ambient-текстом сломала бы смысл `user_asserted`. Триггером служит закрытие эпизода, `TurnContext::ambient` несёт `user_asserted = false`. Диалоговый `check_can_extract` не ослаблен ни в одной ветке — он отверг бы ambient как `no_explicit_trigger`, поэтому у ambient свой гейт `check_can_extract_ambient`: circuit breaker общий, а режимы и бюджеты свои.
- **Режим и бюджеты ambient.** `EVOHIME_AMBIENT_MEMORY` (`off` | `pending`, по умолчанию `pending`); аналога `open` нет, а неизвестное значение даёт `off`, а не молчаливое включение. Общий выключатель старше частного: при `EVOHIME_MEMORY_EXTRACTION=disabled` ambient-извлечение не запускается вовсе. Бюджеты отдельные — 6 кандидатов и 12 эпизодов в час и собственный часовой лимит токенов; переполнение даёт `ambient_candidate_limit`/`ambient_episode_limit`, а не `hourly_limit` диалогового пути, иначе два троттлинга стали бы неразличимы в трассах.
- **Что принимается из речи.** Только `preference`, `entity` и `lesson`. `constraint` и `decision` отбрасываются до persistence: они влияют на действия, и ошибиться в них слишком дорого. `session_summary` не принимается потому, что у речи нет диалоговой сессии, а session-note не проходит через очередь подтверждения. Говорящий — всегда `unverified`: диаризации и голосового профиля в v1 нет намеренно, потому что голосовой шаблон это биометрия, а ошибка диаризации приписала бы пользователю чужое утверждение. Если в высказывании распознан субъект не в первом лице, `privacy_class` поднимается минимум до `sensitive`, что даёт pending и скрытое тело записи. Валидатора у речи нет, поэтому `validation_status` — `unknown`, а не `valid`.
- **Связь с эпизодом.** `provenance_source_id` ambient-кандидата — это `episode_id`: `RawEvidenceLocator` получил поле `episode_id`, и `memory_provenance_source_id` проверяет его первым. Именно по этому значению `ambient_store` при удалении эпизода переводит его кандидатов в `rejected` с причиной `source_deleted`; без этой правки условие осталось бы холостым. `content_hash` для ambient пуст: по правилу 04.1 хеш короткой фразы приравнивается к её содержимому. Живут ambient-записи в собственном scope `workspace/ambient` — речь у стола не принадлежит рабочему каталогу, — но очередь подтверждения одна: `ListMemoryPending` добавляет их к записям текущего воркспейса, а `OperationsPanel` показывает бейдж «услышано», подпись «говорящий не подтверждён» и фильтр по источнику.
- Модель извлекателя задаётся `EVOHIME_MEMORY_EXTRACTION_MODEL`; при отсутствии используется модель маршрута. Пользовательская файловая evidence сверяет полный content hash; `document` evidence дополнительно проходит published RAG generation + chunk hash + свежий file hash. Tool/API-валидация без replayable validator возвращает `unknown`, поэтому такие записи остаются pending, а не подтверждаются вслепую.

Часть команд renderer не доходит до Core и обслуживается main-процессом: `workspace.*`, `chat.*`, `provider.*`, `identity.get`, `repository.get`. Это локальное состояние оболочки, а не права: Core заново проверяет capability, policy и approval для каждой команды, которая до него доходит.

## Ambient listening

Контракт постоянного слушания живёт в `crates/evohime-listener-contract` и не имеет побочных эффектов: ни файловой системы, ни часов, ни процессов. Это канонический источник состояний, лимитов, схемы политики, потолка проактивности и правил логирования; остальные части плана 04 (хранилище, процесс листенера, движок распознавания, UI, мост в память, проактивность) обязаны ссылаться на него, а не заводить второй набор значений. Транспортные копии типов в `evohime.desktop.proto` — именно транспорт, а не второй источник истины.

- **Capability.** `Permission::MicrophoneListen` (serde `microphone_listen`) по умолчанию `Deny`, и этот дефолт прописан в карте `PermissionEngine::new()` явно: `mode()` для отсутствующего ключа возвращает `Ask`, поэтому «просто не добавить вариант» означало бы «спрашивать». `set_all_modes` перечисляет остальные разрешения поимённо и микрофона не касается — Electron переотправляет сохранённый режим воркспейса при каждом открытии, а ветка `PermissionMode` в `ipc_bridge.rs` вызывает `set_all_modes` для любого значения, так что без исключения общий режим молча открывал бы микрофон. Глобальные режимы Core не персистит, поэтому единственный долговечный канал запрета — `permissions.json`; правило `{"permission": "microphone_listen", "pattern": "*", "mode": "deny"}` входит в `permissions.json.example`. Capability проверяет Core перед выдачей разрешения листенеру, а не только UI.
- **Состояния.** `ListeningState`: `Stopped`, `Starting`, `Listening`, `PausedByUser`, `PausedByPolicy`, `DeviceConflict`, `DeviceDisconnected`, `EngineUnavailable`, `Denied`. Поток захвата открыт только в `Listening`; пауза, тихие часы и чёрный список закрывают поток, а не фильтруют кадры. Переходы заданы таблицей: самопереходов нет (повтор состояния не является изменением и не публикует `ambient.state`), в `Listening` можно попасть только через `Starting`, в `PausedByUser` — в том числе прямо из `Stopped` и `Starting` (пользователь может включить слушание уже на паузе, и показывать ему «выключено» в этом случае было бы неправдой; микрофон при этом всё равно закрыт, потому что `is_capturing` истинно только для `Listening`), в `Denied` — из любого состояния, а из `Denied` — только в `Stopped`, поэтому отозванный микрофон не возобновляет работу без полного пути старта с проверкой capability. `DeviceConflict`, `DeviceDisconnected` и `EngineUnavailable` помечены как degraded: UI показывает «проверка состояния» и не утверждает, что слушание выключено.
- **Лимиты.** `AmbientLimits`: кадр 30 мс, pre-roll 300 мс, hangover 700 мс, минимум высказывания 400 мс, потолок высказывания 20 с, эпизод 10 минут, окно дедупликации 60 с. Snapshot неизменяем по образцу `run_policy`: renderer его показывает, но не поднимает.
- **Политика.** `AmbientPolicy` v1 — пауза, тихие часы, чёрные списки процессов и заголовков окон, retention. В тихие часы поток захвата закрыт полностью: ничего не распознаётся и не сохраняется. Политика валидируется целиком до применения: не более 16 окон тишины и 64 шаблонов, шаблон — glob не длиннее 128 байт и не более 8 wildcard, метасимволы регулярных выражений отвергаются, retention от 1 до 90 дней. Невалидная политика отклоняется целиком и никогда не деградирует в «слушать всё».
- **Проактивность.** `ProactivityBudget` — неизменяемый снимок потолка: 3 предложения в час, 10 в сутки, не менее 10 минут между предложениями. Текущие счётчики в snapshot не входят: они живут в `AmbientProactivityRegistry` в Core по образцу `RoutingApprovalRegistry` и передаются в решение как значение. Часы, ушедшие назад, интервал не открывают.
- **Логирование.** `StructuredLogger::write` принимает произвольный JSON, поэтому allow-list на него навесить нельзя; ambient-путь пишет не в сырой логгер, а через типизированный фасад `AmbientLogEvent` с фиксированным набором полей и через `AmbientLogSink`. Свободного текста в фасаде нет по типам: идентификаторы — bounded-newtype с ограниченным набором символов, так что протащить фразу через поле `id` невозможно. Текст речи, хеш текста, имя процесса и заголовок окна не попадают в логи никогда: короткую фразу перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому. Имена записей: `ambient.state`, `ambient.engine`, `ambient.transcript`, `ambient.retention`, `ambient.proposal`, `ambient.error`.
- **Ошибки.** Закрытый набор кодов: `LISTENER_UNAVAILABLE`, `DEVICE_CONFLICT`, `DEVICE_DISCONNECTED`, `PERMISSION_DENIED`, `POLICY_INVALID`, `ENGINE_NOT_READY`, `STORAGE_FAILED`, `CONFIRMATION_REQUIRED`, `INVALID_ARGUMENT`. Неизвестный код показывается как generic-ошибка слушателя и не трактуется как успешная смена состояния.
- **Хранение транскриптов.** Схема v25 добавляет `ambient_episodes`, `ambient_utterances` и `ambient_tombstones`; операции над ними живут в `crates/evohime-local-storage/src/ambient_store.rs`, а часы, сроки и файл политики — в `crates/evohime-core/src/ambient.rs`. Колонок для аудио нет по конструкции: тест проверяет, что `PRAGMA table_info` ambient-таблиц не содержит ни одной BLOB-колонки, поэтому «PCM не пишется на диск» — свойство схемы, а не дисциплины кода. Говорящий в v1 всегда `unverified`, и стор отвергает любое другое значение. Дедупликация идёт по `text_hash` в окне лимитов 04.1; `text_hash` считает Core, он живёт только в таблице и не покидает хранилище — короткую фразу перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому.
- **Retention.** Текст транскриптов живёт `EVOHIME_AMBIENT_RETENTION_DAYS` суток (по умолчанию 7, потолок 90; мусор в переменной даёт дефолт, а не «вечно»), метаданные эпизода и tombstone — 30 суток. Purge выполняется при старте Core и раз в час: `spawn_ambient_retention` делает стартовый прогон **до** первого `sleep`, в отличие от `spawn_approval_gc` и `spawn_receipt_retention`, где `sleep` стоит перед работой. `CancellationToken` эти задачи сегодня не используют, и ambient не вводит отмену в одиночку.
- **Удаление.** Транзакция фиксирует metadata-only tombstone (`episode_id`, время, причина из закрытого набора, число высказываний — без текста и без хеша) до того, как исчезает первое высказывание, затем удаляет высказывания и эпизод, отклоняет производных memory-кандидатов с `provenance_source_id = episode_id` причиной `source_deleted` и вычищает ambient-строки журнала. `forget_window(minutes)` работает по замкнутому окну `[now - minutes, now]`: эпизод, лишь пересекающий границу, не удаляется целиком, у него пересчитываются счётчики, а опустевший уходит в той же транзакции. Кандидаты отклоняются у всех задетых эпизодов, а не только у удалённых: provenance ведёт к эпизоду, а не к высказыванию.
- **Ambient-строки в durable journal.** События публикуются через `append_event` в таблицу `events`, у которой нет retention вообще. Поэтому: в ambient-событии нет ни текста, ни `text_hash` (это гарантирует типизированный фасад 04.1); `task_id` ambient-события — это `episode_id`, а у событий без эпизода — `ambient-session`, чтобы удаление шло по существующему индексу `idx_events_task_sequence`, а не сканом BLOB-payload; удаление эпизода и `forget_window` удаляют соответствующие `ambient.%`-строки в той же транзакции; для всех `ambient.%`-строк действует собственный срок хранения в 30 суток, равный retention метаданных эпизода. Это первый в кодовой базе `DELETE` из `events` — до сих пор оттуда не удаляли ничего, даже при очистке истории ревью (`review.history_cleared` — маркерное событие, а не `DELETE`). Для читателей журнала это безопасно: курсор `push_journal_tail` монотонен по `sequence_id` и дырки переносит.
- **Остаточное окно в бэкапах.** Удаление ambient-эпизода вращает backup-контейнеры той же продовой константой, что и forget памяти (7 суток). Это чистит только состарившиеся контейнеры: в снимке моложе семи суток удалённый транскрипт физически остаётся на диске. Окно называется пользователю прямо в тексте про удаление, а не заметается под «удалено безвозвратно».
- **Файл политики.** `ambient-policy.json` в data dir пишется атомарно: временный файл, `sync_all`, owner-only ACL, `rename`, ACL ещё раз. Отсутствующий файл означает «ещё не настраивали» и читается как дефолт; повреждённый или невалидный — как дефолт **с включённой паузой**: fail-safe в пользу тишины. Невалидная политика не сохраняется вовсе, иначе следующий старт молча встал бы на паузу.
- **Ошибка записи.** Отказ SQLite не ретраится: Core возвращает `STORAGE_FAILED`, не создаёт ложную запись и best-effort публикует `ambient.error` с этим кодом (отдельного имени `ambient.storage_error` нет — набор имён записей закрыт контрактом 04.1). Листенер помечает высказывание как потерянное и продолжает со следующего сегмента.
- **Процесс listener (04.3).** `evohime-listener.exe` запускается supervisor в отдельном Windows Job Object с bounded memory/CPU и собственным restart budget. Job Core создаётся на каждое поколение Core, поэтому падение или рестарт Core не закрывает listener; завершение supervisor закрывает его job и не оставляет сироту. Listener подключается к отдельному owner-only pipe `<core-pipe>-listener`, а shell продолжает использовать основной pipe. Оба endpoint используют отдельную nonce/HMAC-аутентификацию роли `listener`.
- **Аудио и сегментация.** `evohime-listener-audio` открывает cpal/WASAPI shared input, держит PCM только в bounded ring buffer, best-effort применяет `VirtualLock`, ресемплирует 32/48 кГц в 16 кГц и выполняет RMS/zero-crossing VAD. Фикстурный сегментатор использует три voiced-кадра, pre-roll 300 мс, hangover 700 мс, минимум 400 мс и потолок 20 с; длинная речь получает `continued`. Крейт не содержит filesystem API.
- **Движок распознавания (04.4).** Локальный whisper.cpp загружается из проверенного каталога инструментов: `EVOHIME_LISTENER_TOOLS_DIR` → `EVOHIME_TOOLS_DIR\listener` → `%LOCALAPPDATA%\EvoHime\tools\listener`. Первый каталог с валидным манифестом побеждает; недоступный — переход к следующему кандидату, а не ошибка. Биндинги сделаны на `libloading` вручную, потому что сборка продукта не должна требовать CMake: self-update ставит только Git, Node, Rustup и MSVC Build Tools. Отсутствие DLL или модели даёт `EngineUnavailable` с кодом (`tools_dir_missing`, `manifest_missing`, `manifest_invalid`, `manifest_path_escapes`, `file_missing`, `size_mismatch`, `hash_mismatch`, `unexpected_file`, `signature_missing`, `signature_untrusted`, `abi_unsupported`, `load_failed`, `model_load_failed`), а не тишину и не панику.
- **Корень доверия к рантайму — хеш, а не подпись.** Единственный корень доверия — SHA-256 каждого файла из `listener-runtime.json`, полученного тем же релизным каналом GitHub, что и установщик продукта (по образцу `release-installer.ts`, с теми же потолками размера и таймаутом). Подпись Authenticode — дополнительная проверка и только там, где она бывает: `onnxruntime.dll` подписан Microsoft, и для него отсутствие или недоверенная подпись означают отказ; собственный `whisper.dll` подписан **не будет**, пока в проекте не появится настоящий signing pipeline — в CI нет ни `signtool`, ни сертификата, а в `electron-builder.yml` нет `certificateFile`. Требовать подпись у своих артефактов сейчас означало бы предъявить требование, которого не выполняет и сам продукт. Это записано здесь без приукрашивания: до появления code-signing манифест плюс релизный канал остаются единственным корнем доверия. Манифест приходит по сети, поэтому путь из него проверяется на выход за каталог, а любая необъявленная `*.dll` рядом с `whisper.dll` блокирует загрузку: иначе загрузчик Windows подтянул бы её как зависимость мимо проверки хеша. Раскладка `whisper_full_params` зеркалится в коде и сверяется с размерами из манифеста (`48`/`304`) до первого вызова; чужой ABI — `abi_unsupported`, а не попытка вызвать.
- **Поставка рантайма.** Скачивает только Electron main (`listener-runtime.ts`): TLS, потолки размера, таймаут, SHA-256, staging-каталог и атомарный `rename` манифеста последним шагом. Пока манифест не переименован, листенер видит прежний рабочий набор. Неудача даёт ограниченный backoff (15 с → 1 мин → 5 мин) и видимое сообщение; файлы прежних версий удаляются только после успешного переключения, а занятый (ещё отображённый в память листенера) файл остаётся на месте, а не роняет установку. Ни агент, ни Core в сеть за рантаймом не ходят, ничего не применяется незаметно: обновление предлагается пользователю на вкладке «Распознавание речи».
- **Дедупликация и бюджет.** Повтор подавляется в листенере, до отправки в Core: нормализация NFKC + нижний регистр + без пунктуации, точное совпадение в окне 60 с и near-dup по мере Сёренсена–Дайса на множествах слов ≥ 0.9 против пяти предыдущих высказываний. Подавленное считается счётчиком, а не пишется. RTF измеряется на каждом высказывании; пять подряд выше 0.5 переключают модель на следующую в лестнице `small → base → tiny`, каждая смена публикует `ambient.engine`. После `tiny` листенер переходит в `PausedByPolicy` с причиной `engine_degraded`. Обратного хода нет: улучшение нагрузки не возвращает тяжёлую модель в той же сессии, иначе на границе порога модель перезагружалась бы туда-сюда. Ступень, которой нет в поставке, — это сразу деградация, а не тихое «оставим как было». Хранилищная дедупликация по `text_hash` остаётся второй линией.
- **Политика и privacy.** Core передаёт listener валидированную политику; пауза, quiet hours и blocklist закрывают поток захвата, а не отбрасывают кадры. Foreground process/title проверяются раз в listener loop через Windows API. При отсутствии Core listener остаётся `PausedByPolicy`, а сброс `reset_buffers` очищает только ring/VAD/незавершённый сегмент. `scripts/ambient-privacy.tests.ps1` сканирует аудио-крейт на filesystem I/O и запускает детерминированные аудио-тесты.
- **Контроль, IPC и UI (04.5).** Десять additive-команд `oneof command` с тегами 107–116: `SetAmbientListening`, `GetAmbientStatus`, `ListAmbientEpisodes`, `GetAmbientEpisode`, `DeleteAmbientTranscripts`, `ForgetAmbientWindow`, `GetAmbientPolicy`, `SaveAmbientPolicy`, `ResolveAmbientProposal`, `ListAmbientProposals` (116, этап 04.7). Ответы уходят JSON-полезной нагрузкой существующего `EventEnvelope` под именами `ambient.listening`, `ambient.status`, `ambient.episodes`, `ambient.episode`, `ambient.deleted`, `ambient.forgotten`, `ambient.policy`, `ambient.policy_saved`, `ambient.proposal_resolved`, `ambient.proposals`; новых сообщений в `oneof event` не заводится. Имя ответа на решение — `ambient.proposal_resolved`, а не `ambient.proposal`: последнее занято durable-записью журнала, и подмена списка карточек ответом на команду сломала бы проекцию renderer'а. Инвариант полей `SetAmbientListening`: `enabled=false` — `Stopped`, `enabled=true, paused=true` — `PausedByUser`, `enabled=true, paused=false` — запуск или продолжение; `device_id` меняет устройство только после проверки bounded-контракта идентификаторов и наличия устройства в снимке.
- **Единственный источник истины о состоянии.** `AmbientListeningRegistry` в `crates/evohime-core/src/ambient.rs` — по образцу `RoutingApprovalRegistry`, потому что общего `CoreState` в проекте нет. Три точки входа — трей, глобальный хоткей `Ctrl+Alt+M` и вкладка «Слух» — отправляют одну и ту же команду и не хранят локальной копии состояния: их обновляет только событие `ambient.state`. Оптимистичное состояние ставится в реестре сразу после успешной отправки команды листенеру, но реальное всегда приходит от листенера и перекрывает его. Ambient-события пишутся прямо в журнал, поэтому после записи вызывается `TaskCoordinator::notify_journalled` — без этого сигнала `push_journal_tail` не разбудил бы открытое окно.
- **Fail-visible индикатор.** Стартовое состояние реестра — `EngineUnavailable`, а не `Stopped`: пока листенер не подключился, Core не знает, читается ли микрофон. Оболочка спрашивает `GetAmbientStatus` при старте и при открытии панели; молчание дольше пяти секунд, ошибка ответа и незагруженный рантайм дают «Слушание: проверка состояния…» с предупреждением. Утверждение «выключено» делается только по известному состоянию.
- **Три точки входа.** Хоткей регистрируется через `globalShortcut` при готовности приложения и снимается на `will-quit`. Занятая комбинация (`register` вернул `false`) объявляется недоступной отдельной командой оболочки `ambient.hotkeyStatus` — это единственная ambient-команда, которая не ходит в ядро, потому что ответ знает только main-процесс. Трей показывает вариант иконки `evohime-agent-listening.ico` и заголовок состояния, а пункт паузы выключен там, где микрофон и так закрыт.
- **Устройства.** Renderer'у отказано во всех разрешениях и в доступе к устройствам (`security.ts`), поэтому `navigator.mediaDevices` для него закрыт по построению, и единственный источник списка — процесс листенера. `evohime-listener-audio` перечисляет входы cpal и держит message-only окно с подпиской `RegisterDeviceNotificationW` на `KSCATEGORY_CAPTURE`; `WM_DEVICECHANGE` перечитывает список и отправляет его снимок в Core. Идентификатор устройства выводится из его имени и приводится к charset bounded-идентификаторов 04.1, поэтому через поле `device_id` нельзя протащить фразу. Отказ подписки уезжает в `watching=false`, и панель говорит, что список не обновится сам, — вместо того чтобы показывать снимок как живой. Смена устройства закрывает прежний поток и открывает новый без перезапуска процесса; пропажа выбранного устройства даёт `DeviceDisconnected`, а не тихий откат на умолчание.
- **Тихие часы в рантайме.** `PolicyUpdate` несёт окна тишины парой параллельных списков минут суток, `enabled` и выбранное устройство. Листенер пересматривает желаемое состояние каждые 20 секунд по локальным часам (`GetLocalTime`), поэтому наступившее окно тишины закрывает поток само, без команды снаружи.
- **Удаление из UI.** «Забыть последние 5 минут» и «удалить всё» требуют подтверждения в модальном диалоге, а Core независимо отвергает команду с `confirmed=false` кодом `CONFIRMATION_REQUIRED`: обход UI не даёт больше прав. Текст высказываний пересекает границу IPC только в ответе `GetAmbientEpisode`, который отправляется по явному клику «Показать текст»; список эпизодов несёт лишь время, длительность речи, число высказываний и состояние извлечения.
- **Границы эпизода.** Сообщения «эпизод кончился» в протоколе листенера нет — он присылает высказывания и флаг продолжения, — поэтому границу проводит Core: эпизод закрывается началом следующего, минутой тишины (проверка раз в 20 секунд) и разрывом связи с листенером. Закрытие и есть триггер ambient-извлечения: закрытый эпизод уходит в Core командой `ExtractAmbientMemory`, и `extraction_state` эпизода проходит `pending` → `done`/`failed`. Без этой границы извлечение не наступало бы никогда.
- **Ограниченная проактивность (04.7).** По услышанному Ева может произвести ровно два эффекта: карточку-предложение в очереди и неисполняемое напоминание. Список закрыт типом `ProactiveEffect` в `crates/evohime-core/src/ambient_proactivity.rs`, и `authorize_proactive` отказывает `StartTask`, `ToolCall`, `FileWrite` и `NetworkRequest` **до** любого эффекта, раньше mute и раньше бюджета. Это инвариант, а не настройка: новый проактивный эффект — правка перечисления и его негативных тестов, а не значение конфигурации.
- **Откуда берётся предложение.** 04.6 отбрасывает из речи `constraint` и `decision` до persistence именно потому, что они влияют на действия. 04.7 не воскрешает их как память: `decision` становится предложением задачи (`suggestion`), `constraint` — напоминанием (`reminder`), и оба ждут клика. Остальные виды остаются кандидатами в память и предложением не становятся: предпочтение или факт действия не требуют.
- **Два ключа, а не один.** Дедупликация идёт по `proposal_key` = вид + тема + округлённый до часа момент; он и стоит под `UNIQUE`. Постоянный mute («больше не предлагать такое») идёт по `mute_key` = вид + тема, **без времени**. Один ключ на обе роли не работает: со временем внутри ключа mute заглушил бы ровно одну временную корзину и молча перестал бы действовать через час, а без времени `UNIQUE` запретил бы любое повторное предложение по той же теме после истечения предыдущего. Тема в обоих ключах — bounded-токен `SubjectKey`: ASCII-слаг, а для кириллицы короткий отпечаток; пробелов в нём нет по построению, поэтому через это поле нельзя протащить фразу.
- **Потолок доказуем и не обходится очередью.** `ProactivityBudget` из 04.1 неизменяем: не больше 3 предложений в час и 10 в сутки, не чаще одного раз в 10 минут, плюс пауза и тихие часы общей политики. Текущие счётчики живут в `AmbientProactivityRegistry` (`ambient.rs`) и персистятся строкой таблицы v26, поэтому перезапуск Core не обнуляет часовой потолок. Превышение **отбрасывает** предложение со счётчиком в трассе, а не копит его: иначе после часа тишины пользователь получил бы десять карточек разом. Счётчик поднимается только после того, как карточка действительно появилась, поэтому дубликат бюджета не тратит.
- **Схема v26.** Additive-миграция поверх ambient-хранилища v25: `ambient_proposals` (уникальные `proposal_id` и `proposal_key`, `state` из пяти состояний автомата, `occurrences`, nullable `source_episode_id` с `ON DELETE SET NULL` и пара `source_deleted_at`/`source_deleted_reason` под `CHECK` «оба NULL либо оба заполнены»), `ambient_proposal_mutes` по `mute_key` и `ambient_proactivity_counters` — одна строка на ambient-профиль. Миграция транзакционна и получает обычный backup до изменения схемы.
- **Порядок при удалении источника задан явно.** Удаление эпизода переводит связанные предложения в `expired` с причиной `source_deleted`, и этот `UPDATE` выполняется **до** удаления строки эпизода, в той же транзакции с его tombstone. Наоборот нельзя: `ON DELETE SET NULL` сработал бы первым и обнулил связь, после чего найти затронутые предложения было бы уже нечем. FK при этом удаление источника не блокирует.
- **Событие `ambient.proposal`.** Пятое ambient-событие журнала. Payload несёт только `proposal_id`, `episode_id`, `kind`, bounded `subject_key` и `proposal_state`; текста карточки и темы человеческими словами в нём нет — прецедент прямой, `memory.pending` по той же причине не несёт `statement`. `task_id` строки — это `episode_id`, поэтому `ambient.proposal` уходит вместе с эпизодом по существующему индексу `idx_events_task_sequence`, как и `ambient.transcript`. Человекочитаемый текст renderer получает командой `ListAmbientProposals`, а хранится он в `ambient_proposals`, то есть под ambient-retention.
- **Жизнь карточки.** 24 часа без ответа — это ответ «нет»: `purge_ambient` и чтение списка переводят просроченное в `expired`, поэтому вчерашнее предложение не висит как ждущее. Терминальное состояние не переигрывается: второй клик отвечает «уже решено», а не меняет решение.
- **Принятие проходит штатный путь.** Решение несёт обязательный `idempotency_key` (по образцу `ConfirmMemory`): без него двойной клик по карточке породил бы две задачи, а повтор с тем же ключом возвращает первое решение. Принятое создаёт обычную запись `work_items` в проекте `ambient-proposals` со статусом `backlog` — то есть ничего не запускающую саму по себе; `source_ref` несёт `episode_id`, тот же провенанс, по которому удаление эпизода находит своих кандидатов памяти. У напоминания признак неисполняемости записан в данных полем `non_goals`, а не подразумевается. Отдельного ambient-receipt нет: запуск такой задачи идёт обычным путём Core со всеми его approval-гейтами.
### Голосовые команды и каталог приложений

По услышанному Ева умеет открыть приложение. Это не расширение проактивности 04.7, а отдельный путь: проактивность — это то, что Ева делает **без** просьбы, а здесь просьба произнесена вслух. Закрытый список эффектов 04.7 при этом не тронут — карточка и напоминание остаются единственным, что порождает сама Ева.

- **Обращение обязательно.** `crates/evohime-core/src/voice_command.rs` разбирает высказывание детерминированно, без модели: сначала имя (`ева`, `эва`), затем — не дальше двух служебных слов — глагол открытия, затем название. Фраза без обращения командой не является вообще: рядом с микрофоном разговаривают люди, и «открой окно» в их разговоре ничего не запускает. Обращение после глагола («открой хром, Ева») тоже не считается — иначе им становился бы любой пересказ. Цель ограничена четырьмя словами и 64 символами: название приложения — это не остаток разговора.
- **Каталог вместо пути.** Открыть можно только запись каталога `crates/tool-runtime/src/app_catalog.rs`; ни услышанная фраза, ни ответ модели не доходят до `CreateProcess` в виде пути. Источников три, по возрастанию доверия: встроенные системные приложения Windows (путь проверяется на существование), `App Paths` реестра (HKCU, HKLM и WOW6432Node) и пользовательский `app-catalog.json` в data dir. Найденное автоматически дополняет выверенную запись только синонимами: реестр не знает, что `Notepad` по-русски «Блокнот». Исполняемый файл MSIX-пакета подменяется его alias'ом в `%LOCALAPPDATA%\Microsoft\WindowsApps` — прямой запуск из `Program Files\WindowsApps` запрещён ACL. Каталог перечитывается не чаще раза в пять минут.
- **Догадок нет.** Совпадение ищется по точному синониму, затем по границе слова, затем по категории («браузер» — фиксированный приоритет, а не случайный выбор). Неизвестное название и название, подходящее сразу нескольким приложениям, дают одно и то же: не происходит ничего. Подставить за пользователя один вариант из трёх было бы хуже молчания.
- **Клик, а не микрофон.** По умолчанию услышанная команда становится карточкой в очереди `VoiceCommandRegistry` (в памяти, 5 минут жизни, не больше 8 штук, повтор заменяет карточку вместо второй). Открывает приложение клик. Автозапуск существует, но только как явный выбор в ambient-политике: поля `voice_commands` и `voice_commands_autorun`, по умолчанию «распознавать, но спрашивать». Пауза слушания и выключенные команды останавливают разбор целиком.
- **Событие и заголовок врозь.** Шестое ambient-событие журнала — `ambient.voice_command` с полями `command_id`, `kind`, `app_id`, `command_state`. Ни фразы, ни её обрывка в нём нет: `app_id` — ключ каталога, то есть выбор Core из заранее известного списка. Человекочитаемый заголовок renderer получает командой `ListVoiceCommands` — тем же способом, каким читается текст карточки предложения.
- **Две additive-команды.** `ListVoiceCommands` (127) и `ResolveVoiceCommand` (128); ответы — `ambient.voice_commands` и `ambient.voice_command_resolved`. Карточка снимается с очереди до запуска, поэтому двойной клик не открывает два окна: второй ответ — `not_found`. Голосовые поля `AmbientPolicy` объявлены `optional bool`: клиент, не знающий о них, их не шлёт, и Core подставляет сохранённое значение вместо того, чтобы выключить настройку молчанием.
- **Инструменты `app.open` и `app.list`.** Тот же каталог доступен агенту из чата. Вход `app.open` — название, а не путь; разрешение — `shell_execute`, риск — `Medium` (произвольной командной строки, в отличие от `shell.execute`, здесь нет), превью подтверждения — `app_open`. `app.list` только читает каталог и классифицирован как `None`.
- **Открытое приложение переживает Core.** Запуск идёт с `CREATE_BREAKAWAY_FROM_JOB`, а job object супервизора получил `JOB_OBJECT_LIMIT_BREAKAWAY_OK`. Это не ослабление `KILL_ON_JOB_CLOSE`: `SILENT_BREAKAWAY` не выставлен, поэтому отвязывается только процесс, созданный с явным флагом, а дерево самого Core по-прежнему умирает вместе с ним. Если отвязка запрещена, запуск повторяется без флага — приложение всё равно откроется.

- **Разрешения в UI.** `SafetyPanel` перечисляет разрешения по отдельности и переключает единственное, которое не подчиняется общему режиму, — `microphone_listen`; включение идёт той же командой `SetAmbientListening`, которая в ядре делает именованный вызов `set_mode(Permission::MicrophoneListen, …)`. Общий режим по-прежнему меняется в `PermissionModePicker`, и `set_all_modes` микрофон не трогает.

## Данные, диагностика и восстановление

SQLite находится в `%LOCALAPPDATA%\EvoHime` либо в `EVOHIME_DATA_DIR`. Миграции выполняются транзакционно; перед изменением схемы создаётся `.db.bak`. Журнал событий экспортируется в JSONL. Логи core и supervisor пишутся в `%LOCALAPPDATA%\EvoHime\logs`. Permission-правила читаются из `%LOCALAPPDATA%\EvoHime\permissions.json` как упорядоченный JSON-массив PolicyRule: побеждает последнее совпавшее правило, отсутствующий или пустой файл означает встроенный набор, пустой массив `[]` означает осознанное отключение правил. Обновление использует отдельный transaction worker, backup компонентов и recovery незавершённой транзакции перед запуском Core.

Локальное состояние оболочки лежит рядом, в `%LOCALAPPDATA%\EvoHime\shell\`:

| Файл | Содержимое | Ограничения |
| --- | --- | --- |
| `workspaces.json` | список запомненных папок и последняя выбранная | нормализованные пути |
| `chats.json` | чаты, привязанные к workspace, и отправленные промпты | 100 чатов на workspace, 500 сообщений на чат, 4096 символов на промпт |
| `provider.json` | выбранный провайдер, модель, base URL и зашифрованный ключ | режим `600`, запись через временный файл и `rename` |

Повреждённый файл не роняет оболочку: он читается как пустой.

## Бюджет запуска

`evohime_core::run_policy` описывает неизменяемый snapshot политики одного запуска: `max_iterations`, `max_wall_clock_ms`, `max_tool_calls`, `max_tokens`, `max_cost_micros` и `approval_required`. Core проверяет счётчики перед отправкой эффекта; превышение любого из них останавливает запуск с `BudgetExceeded`. Renderer может показать snapshot, но не может поднять лимит в середине запуска.

`evohime_supervisor::pulse` описывает контракт локального digest расписаний: dead-letter даёт `Failed`, пропуски и неуспехи — `Degraded`; успешный счётчик никогда не маскирует отказ. Модуль пока никем не вызывается: пользователь видит статус Pulse в `OperationsPanel`, где он выводится из событий `runtime.schedule_failed`/`runtime.schedule_dead_letter`.

## Ключ провайдера

Ключ вводится в `ProviderForm` и остаётся в main-процессе. Значение шифруется ОС через Electron `safeStorage` (DPAPI на Windows) и сохраняется в `provider.json`; renderer получает только summary с признаком `configured`. Core собирает model gateway из окружения при старте, поэтому сохранение ключа перезапускает supervisor вместе с Core, а pipe client переподключается к новой сессии. В окружение попадают только переменные выбранного провайдера, чтобы устаревший ключ второго не дошёл до gateway. Если ОС отказывается шифровать, ключ не записывается вовсе.

Base URL принимается только по `https` либо по `http` на loopback: ключ отправляется на этот адрес, и произвольный http-хост означал бы его утечку.

## Packaging и запуск

```powershell
.\scripts\build-windows-native.ps1
```

Для разработки используется `start-dev.ps1`; он читает `.env` по allow-list имён из `.env.example` и передаёт их только дочерним native-процессам. Для пользователя GitHub Actions собирает единственный `EvoHime-Setup.exe`. Установщик размещает внутренние `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`, `evohime-transaction.exe` и manifest в каталоге приложения и создаёт ровно один ярлык `EvoHime` на рабочем столе.

Пакет x64 предназначен для Windows 10 2004+ и Windows 11 и содержит bundled Electron runtime, Rust runtime и локальные компоненты; отдельная установка Node.js или браузера не требуется.

## Обновления из исходников

Обновление не связано с GitHub Release: клиент сравнивает коммит своей сборки с вершиной отслеживаемой ветки и пересобирает продукт на машине пользователя.

```text
update.json          репозиторий, ветка, launchPolicy, интервал проверки
%LOCALAPPDATA%\EvoHime\source           git checkout, которым владеет обновление
%LOCALAPPDATA%\EvoHime\update-staging   собранный пакет до подмены
%LOCALAPPDATA%\EvoHime\update-state     журнал транзакции и backup
```

- `evohime.build.json` рядом с бинарниками хранит коммит и ветку сборки; без маркера версия считается неизвестной и клиент пересобирается;
- коммит, не трогающий код клиента (документация, планы, CI-конфиг), не вызывает пересборку: клиент сравнивает установленный коммит с целевым через compare API и пропускает обновление, если ни один изменённый путь не влияет на сборку. Любая неопределённость — обрезанный diff, незнакомый путь, недоступный API — трактуется как «код менялся»: лишняя пересборка дешевле устаревшего клиента;
- обновление идёт только на зелёный коммит: перед сборкой клиент читает check-runs GitHub и берёт самый свежий коммит с пройденными проверками. Пока CI гоняет вершину ветки, берётся предыдущий зелёный коммит — иначе клиент отставал бы на каждый push; если зелёного нет в окне (`greenCommitDepth`, по умолчанию 10 коммитов) или проверки не читаются, обновление откладывается, а не выполняется вслепую. Отключается `requireGreenCommit`;
- проверка обновлений ходит в GitHub API с токеном пользователя, если он есть: анонимный лимит — 60 запросов в час на IP, и выбранный чужим трафиком с того же адреса лимит останавливает обновления с `403`, тогда как с токеном лимит 5000. Источники по убыванию явности: `EVOHIME_UPDATE_GITHUB_TOKEN`, поле `githubToken` в `update.json`, `GH_TOKEN`/`GITHUB_TOKEN`, `gh auth token`. Токен не обязателен — без него проверка работает как раньше; он уходит только на `api.github.com`, не пишется в логи и не сохраняется клиентом;
- git обновления работает без интерактива: сохранённые на машине учётные данные (`gh`, credential manager) используются, но ни git, ни credential helper не могут открыть диалог или спросить пароль в терминале. Зависшее за невидимым окном обновление хуже упавшего — оно блокирует запуск;
- `update.json` пишет установщик, репозиторий принимается только по `https`, ветка и интервал проверки нормализуются — конфигурация не может увести сборку на чужой источник или превратить проверку в busy loop;
- при запуске main-процесс проводит update gate до старта supervisor: собранный пакет нельзя подменить, пока Core держит файлы открытыми. Пользователь видит шаги пересборки и может нажать «Пропустить и запустить»;
- у уже запущенного клиента фоновая проверка собирает обновление в staging и предлагает перезапуск баннером, не прерывая работу;
- недостающие Git, Node.js, Rust и MSVC Build Tools ставятся через winget по фиксированным идентификаторам пакетов;
- локальная сборка падает транзиентно (оборванная загрузка Electron, недописанный `release/`), поэтому после первой неудачи производные каталоги удаляются и сборка повторяется один раз; вторая неудача показывается как есть. Полный вывод сборки лежит в `%LOCALAPPDATA%\EvoHime\logs\update-build.log` — UI показывает только последнюю строку;
- подмену выполняет `evohime-transaction.exe --apply-staging`: он копирует себя во временный каталог, дожидается не только выхода оболочки, но и момента, когда файлы установки действительно доступны на запись (дочерние процессы Electron держат их дольше), делает полный backup установки, переносит staging и при любой ошибке восстанавливает прежнюю установку. Копирование переживает блокировки повторами, а незавершённая транзакция откатывается при следующем запуске.

Неудачное обновление не блокирует работу: установленная сборка запускается как обычно, а причина отказа показывается в UI.

Безопасностные ограничения вынесены в [`../SECURITY.md`](../SECURITY.md).
## Model request provenance v1

Каждый Core model request имеет versioned canonical envelope из
`contracts/model-request/v1/`, связь с единственным `context_ledger.id`,
durable blocks/sources и lifecycle status. `EventJournal` предоставляет Core
API `commit_model_request`, `mark_model_dispatch`, startup recovery и bounded
retention; renderer не строит и не изменяет envelope.

Новые записи проходят `FullForDispatch`; canonical hash использует JCS и
domain-separated SHA-256. Retry/fallback создают отдельный request attempt с
`parent_request_id`/`previous_request_hash`. Source capture, shadowing,
responses/tool intents и typed tombstones хранятся в Core-owned SQLite.

Offline bundle формата `evohime-provenance-export-v1` имеет allow-listed
замкнутые секции и проверяется командой `evohime-verify provenance --bundle`.

Хеш снимка gateway фиксируется до dispatch и вычисляется по реальному снимку
политики маршрутизации. В receipts ссылка на action может быть пустой, а
идентификатор request хранится отдельно; `BEGIN IMMEDIATE` сериализует
конкурирующие добавления в цепочку. Tool intents связаны с terminal effect
receipts, а redaction сохраняет неизменяемое обязательство envelope и создаёт
типизированные tombstone.

## Tool manifest, toolkit catalog и Action Console

Каждый builtin/toolkit tool описывается versioned `tool/manifest/v1` в
`crates/tool-runtime/src/manifest.rs`. Canonical hash манифеста прикрепляется
к model loadout; input schema для registry, model и recovery берётся из
единого `builtin_input_schema`, без таблицы схем в Core. `mcp.call` получает от
модели только `server_id`, `tool_name` и `params`; endpoint разрешается Core
`WorkflowRegistry` по allowlist сервера.

Установленные toolkit-версии хранятся в SQLite `toolkit_versions`, переходы и
rollback — в `toolkit_audit`. Rollback атомарно выключает прежнюю активную
версию и включает выбранную; quarantined/unavailable версии не становятся
исполняемыми.

Approval Console использует durable receipt intent, exact-call binding,
idempotency key, expiry и replay-safe решение. Electron отображает только
Core-события и передаёт grant/reject/cancel обратно через IPC.

Tool lifecycle telemetry записывается в EventJournal, экспортируется в
bounded redacted JSONL и проецируется в Operations Panel: calls, results и
approval requests. Детерминированные manifest/hash/policy evals находятся в
`crates/evohime-core/src/evals.rs`.
## Typed memory, retrieval и compaction

Memory extraction хранится в schema v31. `memory_entries` сохраняет bounded
`record_version`, JSON-массивы `evidence_refs` и `execution_event_refs`; Core
записывает их до публикации retrieval. `privacy_class=secret` отвергается до
SQLite. Retrieval выполняется через Core-owned adapter: scope/privacy
проверяются до ranking, порядок детерминирован по score/freshness/id, citations
проверяют evidence и generation и не могут пересечь workspace scope.

Forget — логическое удаление с обезличенной tombstone-строкой: содержание,
provenance, evidence refs и execution refs очищаются, а digest tombstone
сохраняется для аудита. Повторный forget не создаёт новую tombstone.

Context compaction имеет SQLite-backed operation state machine
`planned/running/cancelled/committed/failed`. Уникальный operation key
защищён SQLite, а versioned projection и provenance linkage хранят snapshot
revision, summarizer version и связь item с `sequence_id`. UI получает только
bounded metadata/projection; исходная execution/evidence история не удаляется
compaction-операциями.

## Local telemetry и deterministic evaluation

`telemetry/v1` — bounded derived projection над Core event journal, receipts и
model-request provenance; эти источники остаются source of truth. Projection
проверяет correlation/event IDs, attempt, bounded event count, redaction и
размер report. `judge_signal` хранится отдельно от deterministic gate verdict.
Offline evals используют literal fixtures, frozen inputs и не имеют доступа к
production filesystem, network, tools или SQLite; malformed traces получают
typed diagnostics, а неизвестный результат не считается pass.
## Изолированный browser backend

Browser tools остаются Core-owned и permission-gated. CDP session привязана к
task/run, URL проходит SSRF-проверку, mutation tools требуют approval, а
selector/type inputs bounded; отсутствие CDP configuration даёт typed failure,
не unrestricted fallback. Browser output и screenshot остаются внутри
workspace sandbox и не раскрывают typed text в structured output.

## Voice pipeline и ambient audio

Listener остаётся отдельным authenticated runtime с whisper.cpp и verified
manifest. Capture требует microphone capability и ambient policy; audio
таблицы не содержат PCM/blob, а transcript/utterance lifecycle ограничен
retention, forget и bounded provenance. Quiet hours, pause, deny и deletion
останавливают или удаляют данные до memory/proactivity gates; новый engine не
загружается без manifest/ABI/hash/package validation.

## Vision и document worker

Vision остаётся optional Core-owned capability. `vision/v1` принимает только
bounded явно переданный artifact с capability snapshot и возвращает typed
unsupported/resource/unknown/degraded statuses; visual output не имеет host
action authority. Worker backend отсутствует в базовом package, поэтому
`backend_unavailable` — штатный fail-closed результат. Evidence, OCR claims и
memory/RAG citations требуют redacted page/frame provenance.
