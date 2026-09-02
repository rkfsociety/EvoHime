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
| `ProviderForm` / `CodexPanel` | единая поверхность настроек источника моделей с внутренними вкладками API и Codex CLI |
| `PlanReviewPanel` | коллективное read-only ревью Markdown-плана несколькими моделями и synthesis-моделью; итог копируется в буфер или экспортируется в Markdown, история очищается кнопкой |
| `RecoveryBanner` + `recovery-state.ts` | состояние восстановления, выведенное только из подтверждённых Core событий |
| `OperationsPanel` | очередь подтверждения памяти и конфликты (только metadata), плюс read-only проекция child- и schedule-событий |
| `OverviewPanel`, `TracePanel` | сводка событий запуска и фильтруемая трасса |

Бизнес-логики в renderer нет: он отображает состояние, полученное через IPC, и отправляет команды.

### Визуальный контракт оболочки

Текущий renderer использует единую тёмную палитру: фон приложения `#101218`,
sidebar `#181b23`, поверхности `#191c25`, hover/active `#252b39`, границы
`#343b4d`, основной акцент `#9b8cff`, вторичный акцент `#7595ff`. Sidebar
содержит только бренд, выбранный workspace и чаты; постоянной навигации по
глобальным инструментам нет. Пользовательские разделы «Обзор», «Ревью планов»,
«Память и Pulse», «Составные задачи», «Продолжения», «Анализ», «Слух» и
«Задачи для человека», а также «Настройки» открываются из выпадающего меню
пользователя вверх. Технические разделы (Workflow Package, бенчмарки,
middleware, structured response, политики защиты и выполнения, среды,
симуляция инструментов, профили ролей, Team SOP и Collaboration Bus) не
показываются в пользовательском списке: они доступны только внутри свёрнутого
раздела «Интерфейс разработчика».

Меню закрывается по Escape, клику вне меню и после выбора раздела. Проект
выбирается существующей поверхностью `ProjectSidebar`, а чат остаётся
привязанным к выбранному workspace. Это изменение только presentation/state
оболочки: IPC-команды, typed bridge, Core-owned state, approvals, recovery,
updates и настройки компонентов не меняются.

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
[`features/task-dependency-graphs.md`](features/task-dependency-graphs.md):
там описан граф зависимостей work items проекта.

## Automation contract 16.1

Порядок зависимостей и открытые решения собраны в [`decision-register.md`](decision-register.md).
Общий release gate для automation и его host-boundary checks запускается из
`scripts/automation-release-gate.tests.ps1` и входит в Rust CI.
Rollback, redacted evidence, privacy/egress и license inventory собраны в
[`release-evidence.md`](release-evidence.md); отдельный gate проверяет их вместе
с backup/restore fixtures.
Финальный audit и rollback evidence описаны в `docs/release-evidence.md`;
проверка запускается `scripts/final-release-audit.tests.ps1` и
подтверждает технический PASS и release GREEN по закрытым решениям register.

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

Model Resilience Policy v1 — отдельный Core-owned слой после выбора primary
route. Он закрепляет policy snapshot/hash, bounded retry/fallback budgets,
нормализованные классы ошибок, compatibility по capability/privacy/data
residency и cancellation. Fallback принимает только заранее разрешённые
`ModelProfileRef`; provider payload пересобирается внутри gateway adapter и
credentials между providers не переносятся. Policy/run overlay ephemeral,
поэтому после restart внешний вызов не повторяется: существующий model
provenance ledger переводит dispatch в `interrupted` или `unknown_outcome`.
IPC 188/event 43 отдаёт только bounded metadata; renderer не получает prompt,
output или provider payload.

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

Для плана 26 SQLite schema была поднята до v36 идемпотентными installer'ами
(тем же путём, что
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
- **Корень доверия к рантайму — хеш, подпись продукта не входит в текущий release scope.** Единственный локальный корень доверия — SHA-256 каждого файла из `listener-runtime.json`, полученного тем же релизным каналом GitHub, что и установщик продукта (по образцу `release-installer.ts`, с теми же потолками размера и таймаутом). Authenticode signing для собственного installer и shipped binaries явно отложен и не является release gate’ом текущего цикла. Для `onnxruntime.dll` сохраняется отдельная проверка подписи Microsoft: отсутствие или недоверенная подпись означают отказ; собственный `whisper.dll` остаётся unsigned штатно. Манифест приходит по сети, поэтому путь из него проверяется на выход за каталог, а любая необъявленная `*.dll` рядом с `whisper.dll` блокирует загрузку: иначе загрузчик Windows подтянул бы её как зависимость мимо проверки хеша. Раскладка `whisper_full_params` зеркалится в коде и сверяется с размерами из манифеста (`48`/`304`) до первого вызова; чужой ABI — `abi_unsupported`, а не попытка вызвать.
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
| `provider.json` | выбранный провайдер, профили API-провайдеров, модели, base URL и зашифрованные ключи | режим `600`, запись через временный файл и `rename`; старый единый формат мигрирует при чтении |

Повреждённый файл не роняет оболочку: он читается как пустой.

## Бюджет запуска

`evohime_core::run_policy` описывает неизменяемый snapshot политики одного запуска: `max_iterations`, `max_wall_clock_ms`, `max_tool_calls`, `max_tokens`, `max_cost_micros` и `approval_required`. Core проверяет счётчики перед отправкой эффекта; превышение любого из них останавливает запуск с `BudgetExceeded`. Renderer может показать snapshot, но не может поднять лимит в середине запуска.

`evohime_supervisor::pulse` описывает контракт локального digest расписаний: dead-letter даёт `Failed`, пропуски и неуспехи — `Degraded`; успешный счётчик никогда не маскирует отказ. Модуль пока никем не вызывается: пользователь видит статус Pulse в `OperationsPanel`, где он выводится из событий `runtime.schedule_failed`/`runtime.schedule_dead_letter`.

## Ключ провайдера

Ключ вводится в `ProviderForm` и остаётся в main-процессе. Значение шифруется ОС через Electron `safeStorage` (DPAPI на Windows) и сохраняется в профиле выбранного провайдера в `provider.json`; renderer получает только summary с признаком `configured` и без секретов. Core собирает model gateway из окружения при старте, поэтому сохранение ключа перезапускает supervisor вместе с Core, а pipe client переподключается к новой сессии. В окружение попадают только переменные выбранного профиля, чтобы ключ другого провайдера не дошёл до gateway. Codex CLI не является записью в этом списке: его ChatGPT-аутентификация принадлежит локальному CLI, а панель Евы не показывает для него API-ключ. Если ОС отказывается шифровать, ключ не записывается вовсе.

В композере выбирается ровно один источник для следующей задачи: активный API-профиль
(LiteRouter, OpenAI Compatible или OpenAI Responses) либо `codex_cli`. Для API
выбор активного профиля перезапускает supervisor/Core с его изолированными
credentials; каталог моделей рядом с композером запрашивается у выбранного
провайдера. Выбор Codex передаёт отдельный IPC intent, Core принимает Codex только
при явном этом режиме и запускает bounded `codex exec` в каноническом workspace.
Обычные dialogue-задачи не меняют backend. Codex stdout/stderr приходят как bounded `tool.output`, а
отсутствующий CLI, пустая модель, отмена и ненулевой exit дают terminal failure
без silent fallback.

Base URL принимается только по `https` либо по `http` на loopback: ключ отправляется на этот адрес, и произвольный http-хост означал бы его утечку.

## Packaging и запуск

```powershell
.\scripts\build-windows-native.ps1
```

Для разработки используется `start-dev.ps1`; он читает `.env` по allow-list имён из `.env.example` и передаёт их только дочерним native-процессам. Для пользователя GitHub Actions собирает единственный `EvoHime-Setup.exe`. Установщик размещает внутренние `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`, `evohime-transaction.exe` и manifest в каталоге приложения и создаёт ровно один ярлык `EvoHime` на рабочем столе.

Пакет x64 предназначен для Windows 10 2004+ и Windows 11 и содержит bundled Electron runtime, Rust runtime и локальные компоненты; отдельная установка Node.js или браузера не требуется.

## Обновления из исходников

Production-обновление использует постоянный GitHub Release и installer того же
зелёного commit; локальная пересборка остаётся dev-only fallback. Клиент
сравнивает коммит своей сборки с вершиной отслеживаемой ветки.

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
- пользовательский repair-run доступен в `OperationsPanel` после bounded digest из трёх ошибок задач. Сама ошибка только показывает кнопку: `repair.start`, `repair.commit`, `repair.push`, `repair.refreshCI` и обновление запускаются отдельными кликами;
- repair-run работает в `%LOCALAPPDATA%\\EvoHime\\repair\\<repair-id>`, проверяет origin выбранного workspace и канонический URL EvoHime, а изменения `AGENTS.md`, `.codex`, workflows, updater, supervisor, receipt, security и `.env*` останавливает до ручного review;
- transaction worker сохраняет backup до post-restart health handshake. Новая оболочка пишет `%LOCALAPPDATA%\\EvoHime\\update-state\\health.json` только после authenticated Core connection; отсутствие marker за 90 секунд вызывает rollback;
- health-marker принимается только с точным bounded JSON-флагом `healthy:true`;
  при timeout worker возвращает прежнее дерево установки, оставляет причину в
  transaction error и не считает обновление применённым. Без `--health-file`
  сохраняется совместимость старого локального тестового режима;
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

TaskCheckpoint v1 добавляет bounded immutable continuity record поверх этих
источников. Контракт живёт в `evohime-core::task_checkpoint` и
`evohime-local-storage::task_checkpoint`, сериализуется детерминированным JSON
и получает SHA-256 `content_hash` без самого hash-поля. Core-derived evidence
(завершённые пункты, blockers, файлы, тесты, gates, approvals и typed refs)
отделено от model-proposed remaining/open/next/narrative/semantic decisions;
модель не может подтвердить effect, approval, test или completion.

Migration v32 добавила SQLite storage для TaskCheckpoint: она хранит только
canonical JSON и bounded metadata в append-only `task_checkpoints`. Повторная запись того же id и payload
идемпотентна, другая запись с тем же id отклоняется. Parent checkpoint обязан
принадлежать тому же workspace и иметь более ранний event sequence. Чтение
`latest_valid` пропускает повреждённую последнюю запись и возвращает предыдущую
валидную для последующего replay; paths, secret-shaped text, secret refs,
traversal и превышение лимитов отклоняются до persistence.
Публичный `TaskCheckpointV1` сериализуется, но не принимает произвольный JSON:
storage декодирует запись через приватный wire-тип и сверяет все SQL metadata с
canonical JSON. Parent projection также проходит документированную таблицу
допустимых state transitions; нарушения возвращаются стабильными typed error
codes, а не строковым контекстом.

Runtime использует этот контракт в Core-owned `TaskCheckpointRuntime`: перед
получением durable agent lease выполняется bounded recovery, затем сохраняется
checkpoint `run_started`. Перед крупной compaction и после проекции контекста
сохраняются throttled checkpoints с hash-ссылкой на ledger; terminal, paused и
conflicted результаты также фиксируются до публикации финального состояния run.
Recovery возвращает только bounded metadata event replay и не повторяет внешний
effect вслепую: усечённое окно, неизвестный outcome, stale/conflicted/blocked
checkpoint и повреждённая цепочка требуют явной reconciliation. Событие
`task.checkpoint.saved` содержит только идентификатор, hash, причину и sequence,
без prompt, ledger payload или другого сырого содержимого.

Desktop surface этапа 23.3 остаётся additive в `desktop-ipc-v1`: команды
`GetTaskCheckpoint` и `ResolveTaskCheckpoint` используют tags 137–138, а
`TaskCheckpointProjection` и `TaskCheckpointActionResult` — typed EventEnvelope
oneof tags 15–16. Проекция содержит bounded counts, статус, blockers, typed refs,
policy id, recovery disposition и event-type metadata; prompt, credentials,
hidden reasoning и raw payload отсутствуют. `ResolveTaskCheckpoint` принимает
только `acknowledge_recovery` или `request_resume`, проверяет checkpoint id и
`source_event_seq`, пишет action вместе с `command_dedup` одной SQLite-транзакцией
и никогда не запускает внешний effect. Electron main/preload только валидирует
форму и пересылает команды, а renderer отображает projection и посылает явное
действие; reconnect/resync продолжает использовать существующий sequence cursor.

### Workspace State Checkpoints (plan 58)

Workspace file state is a separate Core-owned contract from `TaskCheckpointV1`.
`evohime_core::workspace_state_checkpoints` captures only bounded regular files
under a canonical workspace root, excludes VCS metadata and build/dependency
directories, rejects symlink/reparse entries and validates SHA-256 hashes before
use. The defaults are 4096 files, 64 MiB total, 1 MiB per file, 512 path bytes
and 128 components. Snapshot metadata and restore journal tables are additive
schema v57; bytes are not placed in the renderer or user `.git`.

The existing build snapshot IPC remains a compatibility transport. The
dedicated additive `workspace_state_checkpoint` command (desktop IPC tag 209)
exposes create, compare, workspace restore, task-projection restore and
combined restore. Electron shows it only in the collapsed developer interface;
the renderer receives bounded metadata and never reads files. Restore is
preflighted, journaled and conflict-safe: user/external edits produce a typed
conflict and no overwrite. Task restore is an independent projection action;
combined restore executes both explicit actions. Neither path mutates
credentials or external effects.

### Agent Skills

Agent Skills v1 — Core-owned локальный registry для bounded `SKILL.md`. Для
workspace используются roots `.agents/skills`, `%APPDATA%/EvoHime/skills`,
совместимые `.codex/skills`/`.claude/skills` и bundled resources; явный session
root доступен через тот же registry API. Приоритет детерминирован: explicit,
project-native, global, compatibility, bundled. Collision не скрывается:
победившая запись выбирается по приоритету, а диагностика остаётся bounded.

Discovery читает только bounded frontmatter и metadata; тело и
`references/` загружаются только отдельной typed-командой. Пакеты с unsafe
path, prompt-injection/secret-shaped metadata, запрещёнными executable или
permission-полями, неизвестной схемой и превышением лимитов становятся
`invalid` и не исполняются. Capability grant — пересечение с родительским
grant; требование вне него возвращает `capability_escalation`. Registry не
запускает scripts, install/network hooks или model invocation и не сохраняет
тело skill: cache живёт только в процессе Core, а durable trace содержит лишь
skill id, version, hash и source reference.

Skill Trust Pipeline v1 находится между discovery и enable. Offline scanner
`skill-scanner-v1` выдаёт стабильные finding codes, severity и только masked
SHA-256 fingerprints. Trust record keyed by `skill_id + content_hash +
scanner_version + review_policy_version`; изменение package или policy не
наследует старое решение. `SkillRegistry::load`, `load_reference` и
permission selection отклоняют всё, кроме `trusted`/`enabled`, через
Core-owned gate. Contextual review — optional read-only typed report; malformed
или unavailable report не повышает доверие. Metadata records и audit хранятся
в SQLite schema 48, raw body/secrets/credentials не сохраняются.

В `desktop-ipc-v1` добавлены additive commands `ListSkills`, `LoadSkill` и
`LoadSkillReference` (tags 139–141), а typed projections
`SkillCatalogProjection`, `SkillContentResult` и `SkillReferenceResult` имеют
oneof tags 17–19. Electron main валидирует bounds и traversal, renderer
показывает metadata и запрашивает полный документ только по явному клику;
generic payload и authority в UI не используются.

### Persistent Goals

Persistent Goal v1 — Core-owned durable projection цели, переживающая model
turn, чат и перезапуск Core. Контракт и `GoalStore` находятся в
`evohime-local-storage::goal`, runtime facade — в `evohime-core::goal`; общая
SQLite schema — v33. Projection хранит только bounded objective, criteria,
статус, progress, blockers, next action, budgets и ссылки на workflow/child/
checkpoint. Workspace path используется только как входной scope selector и
преобразуется в стабильный SHA-256 `workspace_id`; сам путь, credentials,
capabilities, prompt и hidden reasoning в Goal не сохраняются. Link-команда
принимается только для существующего Core-owned runtime-объекта; отсутствующий
optional backend даёт typed `reference_not_found`, а не успешную ссылку.

Изменения objective/criteria получают новую immutable revision и append-only
event. Повтор команды с тем же idempotency key возвращает прежний typed result,
а устаревший `expected_version` отклоняется. `Completed` разрешён только после
Core-подтверждения всех criteria с evidence ref и verifier identity/version; для
manual criterion клиент передаёт только явное решение пользователя, а
evidence/verifier mint-ит Core. Текст модели или completion одного workflow/
child этого не делает. Исчерпание доступного бюджета представляется статусом
`BudgetLimited`, а recovery сверяет связанные durable ссылки, выдаёт только
bounded warning и не повторяет неизвестный внешний effect.

В authenticated `desktop-ipc-v1` Goal добавлен additive-комплект команд
`CreateGoal`, `GetGoal`, `ListGoals`, `GoalAction`, `UpdateGoal`,
`VerifyGoalCriterion` и `LinkGoalReference` (tags 142–150). Typed
`GoalProjection`, `GoalListProjection` и `GoalActionResult` используют
EventEnvelope oneof tags 20–22; список ограничивается фактическим protobuf
размером и сообщает `projection_truncated` до записи во frame. Electron
main/preload только проверяет форму и маршрутизирует команды; `GoalPanel` в
Overview отображает Core projection и посылает явные
create/pause/resume/cancel/verify actions. Goal не является
scheduler: автоматического создания или продолжения задач из одного сообщения
нет.

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

## Local telemetry и deterministic evaluation

`telemetry/v1` — bounded derived projection над Core event journal, receipts и
model-request provenance; эти источники остаются source of truth. Projection
проверяет correlation/event IDs, attempt, bounded event count, redaction и
размер report. `judge_signal` хранится отдельно от deterministic gate verdict.
Offline evals используют literal fixtures, frozen inputs и не имеют доступа к
production filesystem, network, tools или SQLite; malformed traces получают
typed diagnostics, а неизвестный результат не считается pass.
## Agentic Browser Session v1 (план 55, реализован 2026-09-01)

`evohime-core::agentic_browser_session` владеет versioned lifecycle, session
scope, page revision/fingerprint, bounded snapshots и exclusive
`Agent/Human` control generation. Любая mutation проверяет exact session/page
revision; takeover и возврат control увеличивают revision и потому
инвалидируют старые refs. Durable SQLite schema v54 хранит только lifecycle
metadata, без DOM, cookies, credentials, CDP endpoint или host paths.

Authenticated IPC additive command 204/event 53 возвращает только bounded
metadata projection. Raw `EVOHIME_BROWSER_CDP_URL`, CSS selector и прямой
workspace screenshot path не являются production contract; legacy path получает
typed `legacy_disabled`, отсутствие packaged backend — `unavailable`. Browser
network policy обязана проверять каждый redirect и фактический resolved IP,
включая DNS rebinding, а default profile — `ephemeral_clean`.

Native manifest содержит `browser_backend = EvoHime.exe`. Supervisor передаёт
Core путь к этому executable, Core запускает его в родительском Job Object и
обменивается bounded typed JSON через stdio. Electron backend использует
ephemeral partition, sandboxed BrowserWindow и блокирует каждый HTTP(S)-запрос
по hostname/resolved IP; arbitrary CDP endpoint не поддерживается. При
отсутствии packaged executable capability возвращает typed `unavailable`.
Snapshot, screenshot и download возвращаются как bounded metadata/ArtifactStore
objects; upload читает только разрешённый artifact locator. UI получает только
session/revision/control/error/artifact metadata.

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
## Reliability, recovery и diagnostics

Electron main maintains only a bounded diagnostic projection. The additive shell command `shell.exportDiagnostics` opens a native save dialog and writes `evohime-support-bundle-v2` as a local ZIP with manifest, health, runtime, errors, bounded events/logs, issue draft and redaction report. It never reads workspace files, prompts, tool output or credential payloads; a final whole-archive scan fails closed before saving. ZIP entries are store-only and the destination is created with restrictive permissions. Log collection remains bounded to at most four files, 64 KiB from each file, 120 total lines and a 512 KiB v1-compatible shell projection.

The authenticated additive `CreateDiagnosticsSnapshot` command (tag 202) asks
Core for an ephemeral, bounded JSON snapshot. It maps existing Doctor checks to
`PASS/WARN/FAIL/SKIPPED`, includes measured collection duration, redaction
omissions, bounds and optional conversation/run metadata references, and
computes a SHA-256 fingerprint. It does not create a store or migration, does
not include raw prompts/files/tool payloads/credentials, and does not perform
repair or any external effect. Electron main may include the result in the ZIP
only after the user reviews the preview; no upload or automatic issue
publication exists.

Recovery UI consumes Core events as the source of truth and preserves typed `reason_code`, correlation, sequence and `UNKNOWN_OUTCOME`. Terminal task IDs are indexed once per projection, cancellation is offered only when Core explicitly marks `can_cancel`, and user-visible recovery details use a bounded allowlist of non-secret fields. Database operations use `core.cancelDatabaseOperation`, task operations use `core.stopTask`.

Repair and update statuses retain bounded stage evidence. Commit, push, CI refresh, update preparation and restart remain separate user actions; CI or an unknown outcome never triggers a blind effect.

The Security settings panel selects backup/restore paths in main and forwards only the selected path to Core. Core remains responsible for checksum, preview, approval, progress, cancellation, safety backup and rollback. DPAPI via Electron `safeStorage` remains the canonical provider credential contract. Provider persistence applies a defense-in-depth size/type check to stored ciphertext and writes through a mode-600 temporary file with flush, atomic rename and cleanup on failure.

## Retained child contexts и mailbox

Retained child metadata принадлежит Core и scoped по parent. `RetainedChildV1`,
bounded follow-up и mailbox entries проходят fail-closed validation, canonical
hash и additive SQLite schema v37. Уникальность `(parent_id, child_id)`,
idempotency и parent sequence enforced SQLite; transcript не сохраняется.
Lifecycle retained отделён от terminal child run states. После рестарта
dispatched mailbox entries переходят в `unknown` и не повторяются вслепую.

IPC использует additive command tags 157–161 и event tags 25–26. OperationsPanel
получает только metadata: role/name, lifecycle, revision, activity, pending count
и stale/invalidated/delivery outcome.

## Persistent Analysis Kernel (план 28, реализован)

Analysis Kernel остаётся bounded вычислительной средой и не является security
authority: Core владеет `AnalysisKernelSessionV1`, лимитами, object metadata,
идемпотентностью и host-request validation. Durable metadata хранится в
`analysis_kernel_sessions`, `analysis_kernel_objects`,
`analysis_kernel_events` и `analysis_kernel_idempotency`; большие значения не
дублируются и должны адресоваться существующим ArtifactStore. Migration v38
добавляет эту схему транзакционно поверх текущей v37.

`KernelRuntime` поддерживает только `json_parse`, `json_select` и
`csv_summary`; `filesystem`, `network`, `shell`, `credentials`, raw tool и
прямое чтение artifact отклоняются typed error до эффекта. Runtime state
ephemeral и не сериализуется. Лимиты включают lifetime/idle timeout, request
rate и output budget; чувствительные inline markers fail-closed.

Supervisor имеет allowlisted `evohime-analysis-worker` launch contract и
аутентифицированный Core command channel для `kernel_launch`, `kernel_execute`
и `kernel_stop`: фиксированный sibling executable, `trusted-local-1`,
очищенное окружение и отдельный kill-on-close Job Object с memory/CPU limits.
IPC добавляет commands 162–165 и events 27–28; Electron main/preload
маршрутизирует bounded payload, а вкладка `Анализ` показывает только metadata
projection.

Core отправляет worker-запрос только после повторной admission-проверки,
использует typed result и переводит runtime в `Crashed` при transport failure;
на рестарте running manifests fencing-ятся без blind retry, а supervisor worker
останавливается. TaskCheckpoint/child handoff получают только явно выбранные
immutable checkpointable refs; ephemeral memory в handoff запрещена. Для
optional artifact/tool surfaces отсутствие capability даёт typed
`forbidden_capability`/`unavailable`, а не обход host bridge.

План 28 закрыт 30 августа 2026 года. Release evidence включает migration,
storage/runtime/IPC/replay/approval-boundary, real worker protocol, packaged
worker manifest и Core/supervisor fault smoke; raw values, credentials,
transcripts и абсолютные пути в bundle не попадают. При сбое reset/stop,
recovery fencing и optimistic revision не допускают blind retry.

### Visual Workflow Builder v1 foundation

Builder-контракт находится в `crates/evohime-core/src/visual_workflow_builder.rs`.
Typed `workflow/v1` graph validation, независимые execution/layout hashes и
additive draft/version/handoff schema остаются Core-owned; IPC 173/event 33 и
Electron surface передают только bounded metadata. Authoring, immutable
publish, owner-scoped single-use handoff, recovery и read-only live inspection
обслуживаются Core; renderer не получает полномочий, credentials или raw
runtime payload.

## Plan Artifact v1 (план 57)

`evohime-local-storage::plan_artifact` — единственный mutable authority для
versioned planning contract. SQLite schema 56 хранит append-only revisions и
execution snapshots; canonical camelCase JSON без `content_hash` хешируется
SHA-256. Contract limits: 128 steps, 64 criteria, 32 references, 4 KiB text
fields и 64 KiB canonical artifact. Состояния проходят только явные переходы
`draft → accepted → executing → paused|replan_required|completed|failed|unknown_outcome`;
re-plan создаёт новую revision. `ExecutePlan` фиксирует exact revision/hash и
policy snapshot, но не добавляет shell/network authority: реальный effect
остаётся за существующим Core task/workflow boundary и повторной policy,
capability и approval проверкой. После restart неизвестный dispatch не
повторяется вслепую. Legacy `TaskPlanSpec`, plan context и plan review — только
read-only inputs.

Authenticated desktop IPC расширен additive command tags 206–208 и event tag
55; Electron получает bounded projection и отправляет только явные create/read/
transition/execute actions. Raw prompts, model output, secrets, absolute paths и
executable identities в Plan Artifact boundary запрещены.

## Incremental Change Protocol v1 (план 59)

`evohime-core::incremental_change_protocol` связывает bounded
`RequirementDelta` и `ImpactAnalysis` с exact revision/hash существующих Plan
Artifact и Workspace State Checkpoint. SQLite schema 58 хранит только
versioned run metadata, fingerprints, redacted evidence и idempotency key;
исходные prompts/output, absolute paths и capability grants не сохраняются.
До перехода в `applied` Core сверяет optimistic version и observed scope
fingerprint; drift даёт `stale` без записи. `cancelled` и
`unknown_reconciliation_required` terminal и не ретраятся автоматически.

Authenticated IPC command 210/event 57 и Electron `IncrementalChangeProtocolPanel`
передают bounded JSON и показывают metadata-only projection. Базовый executor
не выполняет внешний effect; будущие adapters обязаны добавить отдельный
policy/approval contract.

## Artifact Handoff Registry v1

`ProjectArtifactRegistry` is a Core-owned semantic layer over `ArtifactStore`.
SQLite schema v55 stores immutable artifact revisions, lineage edges, typed
handoffs, acceptance decisions and idempotency outcomes; it never duplicates
content bytes. Contract `artifact-handoff/v1` validates bounded metadata,
`artifact://` refs, producer/consumer role snapshots and lifecycle transitions.
Workspace and parent revision fingerprints drive selective freshness; unknown
scope is `possibly_stale`, and historical revisions are not rewritten.

Authenticated additive IPC command 205/event 54 and the Electron panel expose
bounded metadata only. Renderer cannot read SQLite/ArtifactStore, mint identity,
change capabilities, or supply secrets, prompts, raw outputs and executable
identities. Restart/recovery preserves durable registry state and does not
blindly repeat an optional consumer effect.

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

### Continual Refinement v1

План 29 закрыт 30 августа 2026 года. Core-контракт находится в
`crates/evohime-core/src/refinement.rs`: proposal создаётся только после
bounded independent evidence admission, получает immutable revision,
owner scope, provenance, policy snapshot hash и canonical content hash.
Authority-bearing тексты отклоняются, а Skill/PromptRule не активируются без
отдельного Core-owned activation adapter. Durable metadata/event storage
находится в `crates/evohime-local-storage/src/refinement_store.rs` и добавлен
транзакционно в schema v39; renderer не получает candidate body, transcript,
secret или hidden reasoning. Authenticated IPC commands 166–168 дают bounded
list/get/action projection с optimistic version checks. Electron OperationsPanel
только отображает состояние и отправляет явные действия; policy и effect
остаются в Core.

### Integration Provider SDK v1

План 33 реализован как Core-owned metadata contract. Typed manifest/action/
trigger/credential/binding/fixture types и bounded schema validator находятся в
`crates/evohime-core/src/integration_provider_sdk.rs`; production external
adapters не включены, а offline `fixture.echo` доступен через
`integration_provider_runtime.rs`. Metadata хранится в schema v40 в
`integration_provider_store.rs`; secret bytes, raw prompts и provider output не
записываются. Workflow binding использует version-pinned `integration_action`,
а неизвестные provider/action дают unresolved outcome. IPC остаётся
authenticated/additive: commands 175–176 и event 35; Electron показывает
metadata-only Integrations в `SettingsModal`. Unknown/unavailable outcomes
fail closed и не повторяют внешний effect вслепую.

### Event Trigger Runtime v1

План 34 реализован как bounded Core-owned ingress для `local_workspace_event` и
`system_event`. Контракт находится в
`crates/evohime-core/src/event_trigger_runtime.rs`: immutable workflow binding
с pinned version/execution hash, normalized envelope, allowlisted mapping,
Core-local authenticity, дедупликация, rate/queue bounds и typed outcomes.
Provider webhook остаётся честным `unavailable` без production adapter.

Durable metadata schema v41 находится в
`crates/evohime-local-storage/src/event_trigger_runtime_store.rs`; она не
содержит credentials или raw prompt/output. Authenticated IPC additive commands
177–178 и event 36 подключены к Electron; Settings → «Триггеры событий»
показывает только bounded projection и unavailable provider state. Runtime не
повторяет unknown external effect вслепую и не выдаёт renderer authority.

### Invocation Presets v1

`crates/evohime-core/src/invocation_presets.rs` содержит Core-owned typed
контракт version-pinned preset, canonical SHA-256 redacted payload, allowlist
валидацию и completed-run sanitizer. Durable immutable revisions находятся в
`crates/evohime-local-storage/src/invocation_presets_store.rs`; authenticated
IPC использует additive commands 179–180/event 37, а Electron показывает
metadata-only список, ручное сохранение текущих workflow inputs и запуск
preset. Automation schedule хранит immutable preset revision/hash/workspace
snapshot; due slot проверяет drift/rebinding и передаёт inputs в обычный
WorkflowRuntime. Migration — explicit Core operation с bounded mapping,
preview и новой immutable revision; temporary overrides не изменяют storage.

### Agent Benchmark Matrix v1

Benchmark Matrix находится в `crates/evohime-core/src/agent_benchmark_matrix.rs`
и является Core-owned metadata-only контуром для versioned synthetic
challenges, ModelProfile × AgentProfile matrix и повторных attempts. Встроенный
`BenchmarkExecutor` разделяет воспроизводимый deterministic executor и явно
включаемый provider/nightly режим; при отсутствии provider результат остаётся
typed `unavailable`. На каждую комбинацию считаются pass-rate, timeout/failure
classes и P50/P95/P99 latency/cost. Unknown, skipped и unavailable не считаются
успешными и не продвигают baseline.

`benchmark_store` добавляет schema v42 с immutable suite/run/attempt/baseline
metadata и idempotent keys. В report входят contract/suite/profile hashes,
bounded metrics, comparison verdict и `redacted` status; raw prompt/output,
credentials, transcripts, absolute paths и production data запрещены. Security
regression — hard failure независимо от среднего score; baseline меняется
только отдельным Core-approved optimistic-version action. Authenticated IPC
использует additive commands 181–182 и event 38. Electron имеет
metadata-only `AgentBenchmarkMatrixPanel`, а deterministic PR gate
`cargo eval --mode deterministic` остаётся отдельным от nightly/manual matrix.

### Agent Middleware Pipeline v1

`crates/evohime-core/src/agent_middleware_pipeline.rs` содержит versioned
Core-owned contract для восьми фаз agent/model/tool loop. Встроенные policies
могут только наблюдать, сузить bounded request или заблокировать его; typed
immutable override сохраняет input hash и provenance, а capability snapshot
проверяется ещё раз перед effect. Ordering определяется `(priority, id)` и
snapshot run привязывает definition revision, contract/policy hash и capability
snapshot hash. `observability.rs` и `PolicyGate` остаются низкоуровневыми
примитивами и единственной final authority перед внешним effect.

`agent_middleware_pipeline_store` добавляет schema v43 с immutable definition и
run snapshot metadata; raw request/result и transient hook payloads не
сохраняются. Duplicate idempotency возвращает typed `Duplicate`, drift и
oversized input fail closed. Authenticated IPC использует additive commands
183–184/event 39, а Electron `AgentMiddlewarePipelinePanel` получает только
bounded metadata и отправляет действия через Core.
## Adaptive Tool Catalog v1 (план 38)

Adaptive selection — это Core-owned narrowing layer между `ToolRegistry` и
`ToolAgent`. Registry manifests и permission preflight формируют authoritative
snapshot; selector не может добавить capability, а перед моделью остаются
только schemas выбранных ids. Deterministic selector ранжирует bounded compact
metadata по нормализованным токенам; optional semantic/model selector возвращает
только stable ids и проходит ту же Core allowlist validation.

Catalog projection ограничен 32 tools (default 8), описанием 256 Unicode
символов и input schema 64 KiB. Empty/invalid/unavailable selection применяет
явный deterministic fallback. Cache живёт только в Core process, ключ включает
revision/registry/policy/grant/query/selector/limit и инвалидируется при любом
изменении этих входов или рестарте. Durable storage не добавляется; в журнал
попадает лишь bounded redacted selection metadata. Existing authenticated
`model.context` event — единственная client projection; Electron panel не имеет
доступа к Core, storage или full schemas.
## Structured Response Contract v1

Model Gateway предоставляет Core-owned schema-first structured response path.
`ResponseContract` содержит contract id, revision, JSON object schema, strategy
(`auto`, `provider_native`, `synthetic_tool`) и deterministic SHA-256 hash.
Размер schema ограничен 64 KiB; результат проходит локальную Core validation
по root type, required и property types. `Auto` выбирает native только для
маршрутов с capability descriptor, иначе использует synthetic output-tool.
Output-tool не является capability, не попадает в ToolRegistry и не выполняет
side effects. Общий лимит — 3 model attempts (не более 2 repair retries).

Lifecycle snapshot, policy hash, attempts и unknown-after-restart состояние
ephemeral; новые SQLite tables/migrations не добавляются. Provenance хранит
только redacted contract hash, strategy, provider capability и typed outcome.
Незавершённый provider request после restart не повторяется автоматически.

Desktop IPC остаётся authenticated и additive: `StructuredResponseCommand` —
tag 185, `StructuredResponseEvent` — tag 40. Electron получает только
bounded metadata projection через generic shell bridge; raw schema, prompt,
output и repair text в renderer не передаются.

## Sensitive Data Guardrails v1

`sensitive_data_guardrails.rs` — Core-owned bounded detector/redactor на
contract version 1. Правила имеют deterministic SHA-256 policy snapshot и
precedence `block > hash > mask > redact`; detectors покрывают email,
secret/bearer tokens и private keys. Structured JSON обходится рекурсивно с
лимитами depth 16/nodes 512, stream держит bounded carry и обрабатывает
совпадения между chunks. Unknown/oversized/malformed input fail closed.

Admission применяется к provider model messages, tool input/output, streaming
API и `model-trace.jsonl`. Local authoritative records отделены от redacted
provider/renderer projection; tool permissions, approval и effect ledger не
ослабляются. Policy и stream state process-local, schema v43 не изменяется.

Authenticated additive IPC использует `SensitiveDataGuardrailsCommand` tag 186
и `SensitiveDataGuardrailsEvent` tag 41 для bounded `status`/`evaluate`.
Electron показывает только destination, policy hash, action/count/status;
raw input/output, credentials и rule bodies в renderer не попадают.

## Execution Policy Profiles v1

`evohime-tool-runtime::execution_policy_profiles` — единственный resolver для
зарегистрированных `shell.execute` и `process.run`. Versioned profile содержит
explicit network/environment policy, bounded timeout/output, mandatory process
tree cleanup и backend requirement; command text и user `env` не выбирают
policy. Environment наследуется только через scrubbed allow-list.

На Windows restricted backend использует Job Object с `KILL_ON_JOB_CLOSE`;
timeout/cancel/drop очищает дерево. Required backend failure происходит до
dispatch success. На других системах разрешён только portable bounded режим
без заявления OS-level sandbox guarantee. Canonical JSON profile hash и
storage schema v44 хранят validated catalog, version/hash/json; handles,
output, leases и cancellation state ephemeral.

Authenticated additive IPC — command tag 187/event tag 42. Electron получает
только profile id/version/hash, backend, policies и bounds через metadata-only
панель; raw command, environment, credentials и process output не пересекают
boundary.

## Execution Backend Registry v1 (план 43)

`evohime-core::execution_backend_registry` — Core-owned registry execution
окружений. Встроенный `local.core` является безопасным default; remote-записи
содержат только canonical HTTPS endpoint и credential reference. Реального
remote transport или запуска внешнего процесса этот контракт не добавляет:
remote handshake возвращает typed `transport_unavailable`.

Контракт version 1 ограничивает 64 backend и 64 capability ids, валидирует
lowercase ids и запрещает userinfo/query/fragment/private или loopback endpoint.
Lifecycle health — `registered/probing/healthy/degraded/unavailable/disabled`,
ошибки типизированы (`invalid_endpoint`, `incompatible_contract`,
`capability_denied`, `transport_unavailable` и др.). Advertised capabilities
пересекаются с Core policy и не могут расширить grants.

Metadata хранится в SQLite schema v45 (`execution_backends` и bounded event/meta
tables) через additive backup-before-migrate path. Mutations используют
optimistic version и idempotency envelope; UI получает только bounded list,
health, capability count и наличие auth ref. Active run affinity фиксируется
immutable `{backend_id, registry_version, handshake_hash, policy_hash}` snapshot
и не меняется при смене default. Unknown external outcome не retry/failover.

Authenticated additive IPC использует command 189/event 44. Electron panel
«Среды выполнения» является projection/action surface; prompt, output, secret
material, executable identity и raw endpoint payload в renderer не передаются.

## Tool Simulation Runtime v1 (план 44)

`tool_simulation_runtime.rs` — Core-owned interception boundary для безопасного
fixture/emulated dry-run. Explicit modes — `real`, `fixture`, `emulated` и
`dry_run`; Real не перехватывается этим runtime, а simulation modes никогда не
вызывают `ToolRegistry` effect adapter и не fallback-ятся в Real. Exact fixture
matching использует schema v1, tool id и SHA-256 hash нормализованного JSON input.
Fixture и Emulated output проходят bounded limits и optional Structured Response
validation. Provenance типизирован как `fixture` или `synthetic`, поэтому
synthetic evidence не является observed effect.

Run, fixture и policy state process-local и исчезают после restart; SQLite
schema остаётся v45 без новых таблиц. В журнал допускаются только bounded
redacted metadata. Duplicate delivery idempotent, missing fixture и invalid
schema fail closed. `CoreNodeAdapter::with_simulation` intercepts tool nodes
после Core policy recheck и возвращает только fixture output; benchmark matrix
использует `FixtureToolBenchmarkExecutor` только для `fixture:` references.

Authenticated additive IPC использует command 190/event 45. Electron получает
только metadata-only status с mode/state/provenance и счётчиками ephemeral
runtime; raw fixture/input/output, prompts, credentials и executable identities
не пересекают boundary. Панель всегда показывает, что simulation не
подтверждает реальный эффект.

## External Coding Agent Adapter v1 (план 45)

`evohime-core::external_coding_agent_adapter` определяет bounded protocol
`evohime.external-agent/v1`: newline-delimited JSON frames `hello`, `hello_ack`,
`run`, `event`, `result`, `cancel`. Core валидирует manifest, capability
intersection, declared credential slots, timeout и immutable `AgentSnapshot`;
raw prompts, outputs, credential values и executable paths не входят в desktop
IPC.

Preset/conversation/event metadata хранится additive в SQLite schema v46;
process handles, streams и transient run state остаются ephemeral. Core
передаёт supervisor только validated opaque run spec. Supervisor разрешает
executable через allowlisted environment mapping, запускает без shell, назначает
отдельный Windows Job Object и уничтожает дерево при cancel/timeout. Unknown
outcome после restart не retry-ится вслепую.

Authenticated additive IPC использует commands 191–192/event 46. Electron
получает только bounded metadata с `core_control_level`
(`full`/`supervised_opaque`/`unavailable`); raw external frames не пересекают
desktop boundary.

## Agent Role Profiles v1 (план 46)

`evohime-core::agent_role_profiles` определяет versioned bounded profile с
objective, constraints, skills, tools, strategy, typed input/output contracts,
budget defaults и режимом `human`/`ai`. Profile revision и SHA-256 canonical
hash фиксируются в `ProfileSnapshot` на каждом runtime instance; stale revision,
duplicate и unknown outcomes остаются typed non-success.

Requested grants не являются authority. Перед effect Core вычисляет только
`parent grants ∩ policy ∩ registry ∩ requested`; human mode сохраняет обычную
approval boundary. Profile catalog хранится metadata-only в SQLite schema v47
(`agent_role_profiles` и immutable revisions), migration v46→v47 транзакционна
и использует общий backup-before-migrate path. Runtime instances и proposals
не персистируются; при restart catalog восстанавливается, а transient run не
повторяет side effect вслепую.

Authenticated additive IPC использует commands 193–194/event 47. Electron
показывает metadata-only «Профили ролей»: число профилей, Core state и
revision/hash metadata; raw prompts, credentials, executable code и hidden
reasoning не пересекают boundary. Profile operations ограничены `list/get/
create/revise/start/cancel` и проверяются Core.

## Resumable Conversation Event Log v1 (план 49)

История диалога принадлежит Rust Core и хранится в SQLite schema v50 отдельно
от глобального transport/audit-журнала. `conversation_id` имеет собственный
монотонный `sequence`; envelope v1 содержит UUID события, kind/category,
correlation/causation/task/run/turn refs, persistence class и sensitivity.
Authoritative payload остаётся в Core, а renderer получает только bounded
64 KiB projection после Sensitive Data Guardrails. Timestamp не используется
как cursor.

`StartTask` принимает additive `conversation_id` и стабильный
`client_message_id`. Core одной транзакцией записывает
`user_message_accepted`, task binding и SHA-256 content hash до dispatch.
Повтор того же id/hash возвращает прежнее принятие и не запускает вторую
задачу; повтор с другим content hash даёт typed `idempotency_conflict`.
Task/model/tool/approval/child/workflow/storage события проецируются в общий
conversation log. Streaming delta имеет `transient_stream`, а завершение или
ошибка создают отдельное durable finalized/failed событие. Tool projection
различает command/file/browser/tool, child payload ограничен summary-полями,
usage содержит purpose/source для раздельной агрегации.

History API поддерживает mutually exclusive before/after cursor, limit 1…200,
bounded kind filter, `has_older`/`has_newer` и retention metadata. Логическая
compaction фиксирует snapshot ref/hash и сдвигает earliest available sequence;
старый cursor получает `cursor_expired`, а audit-строки физически не удаляются.
Authenticated IPC использует commands 197–198 и event 49 без изменения major.
Subscribe сначала возвращает catch-up page после `after_sequence`; дальнейшие
typed live events идут существующим journal tail и дедуплицируются по event id.

Electron main/preload только валидирует и маршрутизирует contract. Чистая
`conversation-projection` обнаруживает gap/conflict, игнорирует exact duplicate,
reconciliate-ит optimistic bubble только по `client_message_id`, агрегирует
usage и batch-ит delta исключительно для отображения. `TaskTimeline` сначала
показывает bounded `chats.json` presentation cache, затем заменяет его Core
history, продолжает cursor catch-up и показывает sending/retry/failed. При
переключении conversation projection сбрасывается; global Core events остаются
compatibility fallback. Renderer не читает SQLite, не решает recovery и не
запускает effect.
# Team SOP Protocols v1 (plan 48)

`evohime-core::team_sop_protocols` provides bounded versioned TeamProtocol
contracts with Agent Role Profile refs, phases, handoffs, review policies and
immutable TeamSession snapshots. Schema v49 and authenticated IPC commands
195–196/event 48 are additive; Electron receives metadata-only projections.
The boundary excludes prompts, transcripts, credentials and executable tools.
### Memory Governance v1

План 50 закрыт 1 сентября 2026 года. Durable
`evohime-local-storage::memory_store::MemoryRecord` остаётся единственной
персистентной authority; Core `memory_domain` используется только как
bounded validation/DTO-слой. `MemoryExtractionFields` дополнен typed
governance metadata: `authority` (`user_asserted`, `system_defined`,
`model_proposed`, `imported`), `durability` (`ephemeral`, `session`,
`durable`) и bounded independent `confidence` в диапазоне 0..=1.

`MemoryWriteGate` в `crates/evohime-core/src/memory_governance.rs` выполняет
fail-closed проверку непосредственно перед каждым Core durable insert:
неизвестная authority/durability, secret, ephemeral/session bypass,
невалидная confidence и непроверенная model/imported запись не доходят до
SQL. Reinforcement требует минимум двух различных evidence refs. Existing
pending-confirmation, dedup/conflict, approval, provenance, retention и
tombstone semantics сохраняются; retrieved metadata не расширяет capability.

Storage schema — v51, миграция additive и backup-before-migrate выполняется
общим storage ladder. Legacy rows получают user-confirmed durable defaults,
а extraction/ambient candidates сохраняются как `model_proposed`. Existing
authenticated memory IPC commands и metadata-only Electron OperationsPanel
показывают governance metadata; raw body/provenance payload, credentials и
hidden reasoning не передаются в renderer. Restart/recovery не повторяет
внешние эффекты вслепую.

**Causal Collaboration Bus v1 (план 51).** Core-owned typed messages are
scoped to an active TeamSession and its pinned protocol hash. Sender identity
and peer routes are derived by Core; the bus extends the retained-child
sequence substrate with metadata-only `collaboration_messages` in schema v52.
Payloads are bounded to 32 KiB, each session has at most 128 pending messages,
and subscriptions are ephemeral. Authenticated IPC commands 199–200/event 50
and the Electron panel expose only redacted metadata. Compare-and-set delivery
and terminal `unknown` recovery prevent blind retries; subscriptions do not
grant artifact, tool or provider authority.

## Conversation Workbench v1 (план 52, реализован 2026-09-01)

Workbench — bounded read-only Core projection рядом с `TaskTimeline`, а не новый
runtime или store. Core composer агрегирует существующий redacted conversation
event log: cursor, число событий/задач и usage counters. Typed registry содержит
Files, Diff, Tasks, Terminal, Browser и Usage; Tasks/Usage доступны по Core
evidence, остальные до соответствующих capabilities дают `unavailable`.

Authenticated IPC additive использует command 201 и event 51, contract v1;
SQLite schema остаётся v52. Запрос scoped к conversation/workspace и bounded
cursor/limit, projection содержит snapshot refs. Renderer получает metadata-only
JSON без raw content, credentials или executable identity. Existing event replay
drives live refresh; switching conversation clears the projection first.

Presentation state (`activeTab`, `splitRatio`, `collapsed`) хранится только в
shell `chats.json` per conversation. Workbench не запускает effects, не добавляет
capabilities и не повторяет unknown outcomes.

## Human Work Items v1 (план 54)

`evohime-core::human_work_items` владеет durable typed Inbox в schema v53.
`HumanWorkItem` имеет versioned state machine `Draft → WaitingForHuman →
InProgress → Submitted → Accepted` с явными `NeedsRevision`, `Cancelled` и
`Expired`; optimistic revision, bounded instructions и `Text`/`Choice` response
schema проверяются в Core. Ответ, принятый от authenticated shell actor, —
только typed data: он не является approval, capability grant или tool identity.

Опциональная привязка к Team SOP использует pinned session/protocol hash и
принимается только для Agent Role Profile в `ExecutionMode::Human`; отсутствующий,
устаревший или AI-слот отказывается fail-closed. SQLite хранит текущее JSON
состояние и append-only metadata transition rows; общий migration ladder делает
backup-before-migrate. В v1 нет provider/tool/child dispatch: timeout даёт
`Expired`, restart восстанавливает лишь durable state и не создаёт completion.

Authenticated additive IPC command 203/event 52 и Electron Inbox показывают
bounded metadata; `get` передаёт только typed user-visible instructions/response
schema. Raw model prompt, hidden reasoning, credentials и approval payloads
исключены из projection.

## Revision-Safe Workspace Files v1 (план 60)

Core-owned file tools используют typed namespaces `uploads`, `workspace`,
`outputs` и run-scoped `scratch`. `FileRef` публикует только logical path,
namespace, размер, SHA-256 content hash и revision; host path не входит в
tool/UI projection. Read/write/patch/delete/move/copy проходят через общую
canonical containment boundary с bounded payloads и traversal/reparse checks.
Existing-file mutations требуют expected hash; fuzzy patch fallback удалён,
uploads immutable. Внешнее изменение обнаруживается при следующей mediated
operation и даёт stale outcome. UI surface показывает только bounded ref/preview
по project scope; мутации остаются в approval-пути Core tools.

## Task Worktree Isolation v1 (план 61)

Task worktree registry хранит Core-owned binding `task_id → worktree_id` с
versioned lifecycle `planned → ready → integrating → cleanup_pending` и
idempotency fence в SQLite schema v59. Approved tool `git.worktree.create`
создаёт bounded detached worktree только внутри repository workspace; ref
injection, arbitrary destination, merge, force/reset и push запрещены. Cleanup
не удаляет dirty worktree. При старте задачи Core использует binding только
после проверки `ready` и фактического существования root, поэтому checkpoint
и filesystem semantics применяются к конкретному worktree backend, а не к
primary checkout.
## Team Resource Budget v1 (план 62)

Team Resource Budget — отдельный Core-owned слой над существующими goal/run
лимитами. TeamBudgetPolicy содержит bounded total limits, per-role/per-phase
allocations, protected reserve, reallocation mode, wall-clock mode и
unknown-cost policy; canonical hash и version обязательны. TeamBudgetState
и append-only ResourceUsageEvent хранятся в SQLite schema v60. Повторная
запись usage idempotent, обновление state fenced optimistic version; restart
не сбрасывает counters. Unknown usage маркируется uncertain и не считается
нулевым.

Preflight складывает текущий spend, reservations и conservative estimate и
возвращает Allowed, UnknownCost либо BudgetBlocked; reserve доступен только
policy-разрешённым ролям/фазам. Reallocation не расширяет grants, requester
не может выдать budget себе, а downgrade provider/model идёт только через
существующие compatibility/resilience policies. IPC team_resource_budget и
Electron panel передают bounded JSON/metadata; renderer не владеет state,
accounting или effect.
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
## Workspace Bootstrap Manifest v1 (план 64)

Core принимает versioned `WorkspaceBootstrapManifest` только через
authenticated additive IPC command 215/event 61. Manifest имеет bounded schema,
SHA-256 content hash и явное описание шагов, executable identity,
idempotency и network requirement. Discover читает только
`.evohime/bootstrap.json` из уже выбранного проекта; Core повторно проверяет
workspace scope, относительные пути и размер.

SQLite schema v62 хранит manifest, trust decision и preparation cache по
`workspace_id + manifest_hash + fingerprint`. Новый hash/fingerprint не
наследует доверие или результат. Запуск возможен только для exact trusted hash;
running lease single-flight, истёкший lease переводится в `unknown_outcome`, а
повтор подготовленного exact cache возвращает bounded metadata. Процессы идут
через существующий `ExecutionPolicyProfile`/Job Object boundary, с timeout,
cancel-on-drop и scrubbed environment; network effects и неподдержанные file
effects fail closed. Electron показывает только developer metadata projection.
## Team Coordination Policies v1 (план 65)

TeamSpec и coordination policy принадлежат Core и versioned. Поддерживаются
RoundRobin, Selector, DirectedHandoff и RoleRouter. Core валидирует роли,
selector output, target и event refs; DirectedHandoff дополнительно требует
текущего owner. Loop/repeated-selection и team-turn limits являются частью
state machine. Policy state отдельно от transcript сохраняется в SQLite schema
v63 с optimistic version/idempotency; selection event — bounded metadata-only.
Coordination не запускает tools/shell и не расширяет child grants,
permissions или capability set. Authenticated IPC command 216/event 60 и
Electron developer panel показывают только Core projection.
## Typed Agent Handoff Contract v1 (план 66)

`HandoffPacket` — Core-owned bounded transfer of task ownership between
logical agents/roles/child contexts. It carries objective, reason, summary,
checkpoint/artifact/evidence refs, open questions, blockers, run refs,
`ContextTransferSpec`, expiry and provenance; capabilities, credentials and
authority are never inherited. State transitions are
`Proposed -> Accepted -> Active -> Completed` with typed `Rejected`, `Expired`,
`Failed` and `Returned` outcomes. Every transition is actor/reason/version
fenced, duplicate delivery is idempotent, and pending state survives restart
in SQLite schema v64. Authenticated additive IPC command 217/event 62 and the
Electron developer panel expose bounded lifecycle metadata only.

## Schema-Driven Agent Configuration v1 (план 67)

Core публикует versioned `ConfigurationSchema` с пятью слоями
`ApplicationDefaults`, `WorkspaceDefaults`, `AgentProfile`,
`ConversationDefaults`, `RunOverride`. Поля имеют typed kinds, Core registry
source, sensitivity и apply/restart semantics; semantic patches проверяются
Core, а неизвестные executable-like fields и недоступные references отклоняются.
`ConfigurationSnapshot` содержит effective values, source layers, redacted
secret states и deterministic `sha256` hash. Snapshot revision fenced
optimistically и сохраняется metadata-only в SQLite schema v65; обновление не
мутирует active run и не запускает side effects. Authenticated additive IPC
использует command 218/event 63, Electron получает только bounded projection.

## Experience Replay Library v1 (план 68)

Core-owned `ExperienceRecord` хранит bounded episodic trajectory summaries,
typed outcome и evidence-backed score отдельно от Memory/Refinement. Write Gate
требует evidence, scope, redaction, hash и не принимает `UnknownOutcome`;
credentials/raw outputs и chain-of-thought не сохраняются. Опыт хранится в
SQLite schema v66 с duplicate-safe записью. Bounded context projection остаётся
untrusted advisory; Core сохраняет scope/retention policy и не расширяет
capabilities. Authenticated additive IPC command 219/event 64 и Electron
developer panel показывают только metadata/action projection.
## Runtime Intervention Pipeline v1 (план 69)

Core-owned pipeline поддерживает typed hooks для model request/response,
agent messages, tool dispatch/results, handoff, workflow commit и external
publish. Handlers упорядочиваются по phase/priority/stable id и возвращают
явные Allow/Modify/Deny/PauseForApproval/Abort decisions. Built-in policy
handlers fail closed; mutations имеют safe hashes/patch metadata без secret
значений, approval всегда revalidated Core, а intervention depth ограничивает
reentrancy. Authenticated additive IPC command 220/event 65 и Electron
developer panel передают только bounded diagnostics.

## Code Diagnostics Feedback Loop v1 (план 70)

Core нормализует зарегистрированные provider diagnostics в versioned
revision-bound snapshots. Canonical `sha256:` hashes и workspace-relative file
refs дают deterministic introduced/resolved/persisting delta; stale bindings
отклоняются как evidence. Registry, snapshots и deltas сохраняются в SQLite
schema v67, а quality gate возвращает только typed passed/blocked outcome.
Только Core может зарегистрировать provider или сохранить snapshot; raw output,
commands, credentials и arbitrary code actions не получают capabilities.
Authenticated additive IPC использует command 221/event 66, Electron получает
bounded metadata projection и не вычисляет state machine.

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

## Declarative Agent Component Registry v1 (план 74)

Core-owned registry использует stable public provider IDs, typed
`ComponentDescriptor`, отдельные spec/component versions и built-in trusted
allowlist. Loading выполняет provider/type/version/schema/dependency validation;
unknown providers, cycles, missing migrations и raw secret values fail closed.
Descriptor diff/dump и explicit one-step migration остаются deterministic и
bounded; dynamic code loading и marketplace отсутствуют. Durable registry
хранится в SQLite schema v71, authenticated additive IPC использует command 225
/ event 70, а Electron показывает только metadata/action projection.

## Typed Context References v1 (план 75)

Core-owned typed resolver принимает только bounded built-in reference kinds
для file/folder/workspace, git, diagnostics, terminal, artifact/task/plan,
goal/workflow, browser snapshot и read-only URL. Resolver валидирует scope,
safe locator, SSRF/path boundary, exact revision/hash и sensitivity; refs не
содержат raw content и не расширяют capabilities. Lazy projection budget
детерминированно отбрасывает или откладывает контекст сверх лимита.
Durable reference metadata хранится в SQLite schema v72, authenticated
additive IPC использует command 226/event 71, Electron показывает только
metadata/chip projection.

## Safe UI Extension Framework v1 (план 76)

Core-owned `UiExtensionManifest` описывает только bounded declarative
contributions: pages, conversation panels, sidebar items, status cards,
artifact visualizers, themes и settings sections. Host renderer строит их
собственными компонентами; arbitrary JavaScript/native code, shell,
filesystem, network и прямой доступ к Core DB не допускаются. Data sources и
actions разрешаются только через известные Core-owned bindings, а unknown
binding, path traversal и oversized manifest fail closed.
Установленный lifecycle durable и scoped: install всегда создаёт
`InstalledDisabled`, enable/disable разделены optimistic revision fence, а
restart не включает расширение автоматически. SQLite schema v73 хранит только
Core-authoritative metadata и serialized manifest; ephemeral render errors не
становятся authority. Authenticated additive IPC использует command 227/event
72, Electron получает metadata-only projection и не вычисляет lifecycle или
capabilities.
