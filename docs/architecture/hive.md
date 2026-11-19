# EvoHime — доменный контракт: Hive, роли и координация агентов

Канонический контракт домена улья (Hive), делегирования, ролей и координации команд. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).

## Self-Development Loop

Саморазвитие Королевы Евы включается только явно настроенным
`HiveSettings::eva_workspace_path`. Core принимает настройку, лишь если путь
существует, является Git checkout и отличается от текущего workspace задачи.
Без валидного пути Self-Tools не попадают в доступный модели каталог. Они
работают только с отдельной изолированной рабочей копией исходников и никогда
не изменяют файлы установленного или запущенного бинарника EvoHime.

Реестр публикует `eva_workspace_read_file`, `eva_workspace_write_file`,
`eva_workspace_run_command` и `eva_workspace_git` под capability
`eva_self_development`. Core перед выдачей и исполнением проверяет доверенные
role/depth: доступны только top-level Queen/coordinator, worker и sub-agent
получают `PermissionDenied`. Файловые пути разрешаются относительно checkout;
абсолютные пути, traversal, symlink escape и shell composition отклоняются.
Команды проходят узкий allow-list и ограничены пятью минутами, результат
содержит stdout, stderr и exit code.

Self-Development Loop: прочитать исходник → записать ограниченное изменение →
запустить тестовую команду → commit только изменённых Self-Tools файлов → push.
Любая следующая запись сбрасывает тестовый допуск; push отклоняется без
успешной тестовой команды в текущей сессии либо при любом незакоммиченном
состоянии checkout. Commit не использует `git add -A`, поэтому не включает
изменения сторонних процессов. Текущая работающая система остаётся неизменной
до отдельной поставки/установки нового пакета.

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
budget, lease, reason, dead-letter), глубину child-задачи (`depth=1` для
рабочего, `depth=2` для подагентов) и состояние ленивого fan-out
(`подагенты N / M`, число ожиданий отчёта, «в работе», «ожидает решения»,
«расширение grant'а запрошено»), отделяя trace projection от audit.

Глубина child nesting ограничена явной константой `MAX_CHILD_DEPTH = 2`:
королева делегирует рабочему (`depth = 1`), а рабочий может порождать
подагентов (`depth = 2`). Подагент уровня 2 не может порождать детей:
`ChildDepthExceeded` возвращается раньше любой capability-проверки. Глубина 3
и больше запрещена. `NestedChildForbidden` не снят глобально: он остаётся для
вложенного запроса без действующего runtime-контекста и для legacy-пути,
который не несёт delegation grant.

Один дополнительный hop разрешается только явным delegation grant, выданным
королевой в исходном `TypedChildTaskRequest` (`DelegationGrant`: роль,
purpose, grants, бюджет, acceptance criteria и `max_children`).
`max_children` — потолок, а не план: королева выбирает его по форме задачи
(явное решение или детерминированная эвристика
`HiveSettings::choose_delegation_ceiling` от длины цели, числа context items и
независимых областей), а рабочий порождает подагентов по требованию —
запускает одного, ждёт accepted report, оценивает прогресс по
`DelegationProgressPolicy` и либо завершается раньше потолка с
неиспользованным бюджетом, либо запускает следующего. Одновременно в работе не
больше `max_parallel_subagents`, поэтому несколько детей идут только в этих
границах.

Рабочий не может расширить grant или синтезировать новый: executor берёт
только исходный grant, проверяет `grants ⊆ grants рабочего` на каждом spawn,
выделяет подагенту детерминированную долю остатка
(`remaining / max(1, slots_left)` с нижней границей 1), возвращает
неиспользованное время после отчёта и очищает delegation у подагента. Без
grant попытка вложенного запуска отклоняется типизированной ошибкой
`delegation_not_granted`. Когда потолок исчерпан, а подзадачи остались, рабочий
формирует `DelegationExtensionRequest`; увеличить потолок может только
queen-side `lazy_delegation::extend_delegation` и только в пределах
`HiveSettings`. Отказ королевы (`DelegationAuthority`, по умолчанию
`DenyDelegationExtensions`) завершает fan-out частичным результатом, а
`LazyDelegationState::extend_ceiling` всегда возвращает
`DelegationExtensionNotAllowed`. `BoundedTaskGroup` продолжает ограничивать
общий пул. Fan-in двухуровневый: рабочий принимает accepted
`TypedChildReport` каждого подагента, объединяет bounded findings/sources с
собственным отчётом и возвращает агрегированный отчёт королеве через тот же
`child_workflow` writer.

`HiveSettings` (`hive_settings.rs`, scope `hive` в существующем
`schema_agent_configurations`) — жёсткий страховочный потолок, а не план
порождения: per-worker 16, parallel 4, workflow-wide 64. Превышение
отклоняется (`DelegationCeilingAboveHiveLimit`,
`DelegationWorkflowGuardExceeded`), но дефолт никогда не означает «породи
столько». `0` для per-worker и parallel означает «без отдельного потолка»
(действует workflow guard), а `max_total_subagents_per_workflow = 0`
недопустим. Ceiling и счётчики принадлежат состоянию рабочего, а не UI:
`ChildProjection` несёт `spawned_subagents`, `planned_ceiling`,
`extensions_granted`, `lazy_waiting`, `in_flight_subagents` и
`extension_requested`; старые projection без этих полей воспроизводятся с
нулями.

Визуализация улья — отдельная read-only проекция: `hive_state::HiveStateSnapshot`
(`crates/evohime-core/src/hive_state.rs`) агрегирует последние `child.workflow`
записи журнала (окно не больше 512 событий), настройки `HiveSettings` и роли
активных записей `persistent_agent_registry`. Snapshot содержит статус королевы,
счётчики (`total_agents_online`, `active_tasks`, `efficiency_percent`), департаменты
по ролям, активных агентов с глубиной и прогрессом подагентов, `task_flow` и
текущие лимиты, настроенный `daily_budget_usd`, необязательный проверенный
`eva_workspace_path`, число начатых задач за UTC-сутки и среднюю длительность
завершённых задач, а также дневную оценку стоимости DeepSeek. Task metrics
вычисляются SQL-агрегацией по durable `events`-журналу; при ошибке чтения
показываются нули. Это производная проекция, а не второй источник правды: она не
запускает и не останавливает агентов, не содержит prompt, отчётов или вывода
инструментов, а пустой журнал или реестр дают пустые массивы и нули вместо
ошибки. Electron запрашивает её командой `core.getHiveState` (proto command 286,
ответ — событие `hive_state.snapshot`, schema version 5) раз в 2 секунды и
показывает её на отдельной вкладке «Улей». Snapshot также несёт
`pulse_status` (`active`/`disabled`/`sleeping`) и `next_wake_at_ms`
Core-демона `EvaPulse`; значение берётся у самого демона, а не вычисляется в
UI. Budget-секция агрегирует за
скользящие минуту/час/сутки только committed dispatch-маркеры выбранной модели
и RPM/RPH/RPD из активной route configuration; нулевой cap означает
«без настроенного лимита». Провайдер в проекции — это выбранный профиль
(`deepseek`, `literouter`, `openrouter`, …); счётчики запросов запрашиваются по
транспорту и точной модели, потому что именно транспорт записывает immutable
provenance ledger. Для DeepSeek Core сохраняет
prompt/completion и provider-reported cache-hit tokens в append-only
`context_ledger_usage`; snapshot суммирует текущие UTC-сутки по модели
`deepseek-flash` или `deepseek-v4-pro` (retired-написания `deepseek-v4-flash` и
`deepseek-v4-flash-vision-exp` считаются по тарифу Flash), учитывая
peak/off-peak по времени отправки каждого запроса и публичный тариф. Строки
выбираются по провайдеру и точной модели из immutable ledger, поэтому смена
маршрута не смешивает расходы. Это оценка
по фактическому usage, а не billing statement; отсутствующий cache-hit считается
cache miss, unknown model или недоступная история не отображаются как нулевой
расход. Peak hours: 01:00–04:00 и 06:00–10:00 UTC в будни, кроме официальных
праздников Китая (текущий проверенный календарь 2026, [официальное уведомление](https://www.gov.cn/zhengce/zhengceku/202511/content_7047091.htm)); для других лет стоимость намеренно недоступна, пока календарь не обновлён. Тариф берётся из
[документации DeepSeek](https://api-docs.deepseek.com/quick_start/pricing/).
Статус и точный `wake_at_ms` появляются только когда
достигнут настроенный cap и известен старейший dispatch в окне. Ошибка провайдера
сама по себе не объявляется паузой или cooldown: gateway может повторить запрос
или выбрать другой разрешённый маршрут. Approval banner и чат используют один
renderer-state решённых ID, но решение всегда отправляется только через
`core.resolveApproval`, а Core остаётся источником полномочий. Число активных
запросов отображается на пункте «Улей» в навигации. Если чтение dispatch-history
не удалось, snapshot явно отдаёт `budget.history_available = false`: UI показывает
прочерк вместо фиктивных нулевых счётчиков и не объявляет cap исчерпанным.

Настройка пути передаётся командой `core.setEvaWorkspacePath` (proto command
288). Core принимает только существующий корень отдельного Git checkout,
который проходит `EvaWorkspace::open` и не совпадает с каталогом установленного
или запущенного продукта. Некорректное или пустое значение очищает настройку и
возвращает отказ с диагностикой; UI показывает ошибку и не считает инструменты
включёнными.

### Фоновый цикл EvaPulse

`eva_pulse::EvaPulseDaemon` — единственный фоновый цикл улья, запускаемый
bootstrap-ом Core один раз на процесс. Интервал берётся из
`HiveSettings.pulse_interval_secs` (по умолчанию 1800 с), бюджет — из
`HiveSettings.daily_budget_usd`. Перед каждым пробуждением демон читает те же
committed-наблюдения, что и snapshot: dispatch-окна, дневную оценку DeepSeek и
bounded-окно `child.workflow`. Порядок решений фиксирован: явное окно 429 →
дневной бюджет → исчерпанный provider cap → новые упавшие воркеры → `all good`.
В состоянии `all good` Core пишет `EvaPulse: all good` в `core.jsonl` и
`tracing`; при падении воркера создаётся одна ограниченная задача через
`TaskCoordinator` (не больше восьми id). При настроенном
`eva_workspace_path` prompt recovery-задачи направляет Королеву в checkout и
`eva_workspace_*`; Core предлагает инструменты только этой задаче после
валидации пути. Capability `eva_self_development` ограничена queen/coordinator
на глубине 0. Разрешения каждого инструмента проходят обычный preflight/approval,
workspace tools запрещают изменение файлов вне изолированного checkout, а их
тестовый gate не допускает commit/push до успешного теста и чистого репозитория.
DeepSeek автоматически не выбирается: фоновый маршрут остаётся primary
LiteRouter, платный fallback не включается. Без workspace создаётся обычная
read-only investigation-задача.

Бюджет защищён на уровне демона, а не UI: `daily_budget_usd = 0` блокирует
создание задач, достигнутая оценка расхода останавливает цикл до ближайших UTC-суток,
исчерпанное окно провайдера Улья — до точного `wake_at_ms`. Если бюджет или usage
недоступны, демон не выдумывает ноль и не включает платный маршрут. Фоновые
задачи EvaPulse и системный чат Улья (`hive-system-chat`) строго изолированы на
собственном маршруте `"hive"` (создаётся из отдельного `HIVE_API_KEY` либо при
явном выборе локального `ollama`) и опциональном резервном маршруте Улья
`"hive_fallback"` (`HIVE_FALLBACK_API_KEY`, `hive_fallback_provider_profile`,
`hive_fallback_model`): если Улей не настроен, он не использует глобальных
провайдеров чата и их API-ключи, `ModelGateway` разрешает fallback только
внутри изолированного пула Улья (`"hive"` → `"hive_fallback"`, запрещая переход
на глобальные маршруты и выбор `"hive"`/`"hive_fallback"` обычными чатовыми
задачами), а статус EvaPulse без настроенного `"hive"` остаётся `sleeping`
(`primary_route_unavailable`). Выбор основной и резервной модели в настройках
Улья выполняется из списка моделей выбранного провайдера Улья через каталог
Core (`core.listModelCatalog` в режимах `hive_catalog` / `hive_fallback_catalog`).
При 429 gateway публикует process-local cooldown
в `rate_limit_signal`, поэтому демон не повторяет запрос и не вызывает fallback —
он просто спит до конца окна.

Тумблер «Авто-режим (EvaPulse)» вызывает `core.togglePulse` (proto command 287,
ответ `pulse.status`). `false` немедленно останавливает цикл (`pulse_status =
disabled`), `true` будит его и запускает цикл без ожидания интервала. Статус и
`next_wake_at_ms` приходят обратно в `hive_state.snapshot`; renderer только
отображает их и не вычисляет бюджет или окно сам. Бюджет и task metrics также
читаются из этой проекции, а не из фиксированных значений UI.

Поле `parent_is_child` само по себе — утверждение вызывающей стороны, а не
доказательство. Доверие к нему проверяется по runtime-маркеру
`child_execution_context::ChildExecutionContext`: `RegisteredChildExecutor`
устанавливает task-local контекст только вокруг выполнения воркера, вычисляя
`depth = parent.depth + 1` (верхний уровень — 1), а `validate` читает его.
Маркер не сериализуется, не входит в IPC-контракт, не наследуется другим task
и несёт глубину вместе с признаком разрешённой делегации, поэтому command
handler и IPC никогда не могут подделать `depth`. Если `parent_is_child = true`,
а маркера нет, запрос отклоняется типизированной ошибкой
`child_context_missing`; если маркер есть, но глубина превышает
`MAX_CHILD_DEPTH`, возвращается `child_depth_exceeded`. Запрос без маркера и без
child-признака остаётся обычным верхнеуровневым вызовом.

Lifecycle и accepted-report запись имеют один writer:
`child_workflow::persist_child_checkpoint` и
`child_workflow::record_accepted_child_report`. Executor и legacy-команды
`SubmitChildRequest`/`SubmitChildReport` используют его для создания
checkpoint, bounded projection, принятия `TypedChildReport`, записи
`ChildReportRecord`, проверки `CollaborationMessage` и durable handoff-строки.
Поэтому одинаковый отчёт даёт одинаковые состояние, revision, budget,
`report_status`, `report_summary` (не длиннее 512 символов), provenance и
evidence locators независимо от входного пути; полный отчёт по-прежнему
ограничен 8192 символами на текстовое поле и 32 KiB сериализованного вывода и
остаётся в child store.

Pinned TeamSession загружается ограниченно: не более 64 сессий по последнему
обновлению и не более 64 KiB на сериализованный snapshot; листинг усекается, а
snapshot сверх лимита отклоняется до записи (`save_session` возвращает `false`).
Список протоколов ограничен 256 строками по 64 KiB. Отчёт ребёнка ограничен
8192 символами на текстовое поле и 32 KiB сериализованного вывода; в
parent-facing projection попадает только `report_status` из фиксированного enum
и `report_summary`, усечённый до 512 символов, — полный отчёт остаётся в child
store и не раздувает событие/журнал.

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

## Team SOP Protocols v1 (plan 48)

`evohime-core::team_sop_protocols` provides bounded versioned TeamProtocol
contracts with Agent Role Profile refs, phases, handoffs, review policies and
immutable TeamSession snapshots. Schema v49 and authenticated IPC commands
195–196/event 48 are additive; Electron receives metadata-only projections.
The boundary excludes prompts, transcripts, credentials and executable tools.

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

Authenticated additive IPC command 203/event 52 и inline-карточка вопросов в
чате показывают bounded metadata (только когда есть активные вопросы); `get`
передаёт только typed user-visible instructions/response schema. Raw model prompt, hidden reasoning, credentials и approval payloads
исключены из projection.

## Task Worktree Isolation v1 (план 61)

Task worktree registry хранит Core-owned binding `task_id → worktree_id` с
versioned lifecycle `planned → ready → integrating → cleanup_pending` и
idempotency fence в SQLite schema v59. Approved tool `git_worktree_create`
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

## Message Intervention Policies v1 (план 110)

Before a Collaboration Bus delivery reaches recipient context, Core may evaluate
a versioned metadata-only hook policy. Hooks use deterministic priority ordering
and validate route, sensitivity, phase and idempotency guards. Verdicts are
typed (`allow`, `block`, `redact`, `redirect`, `escalate`); projection patches
are allowlisted strings, and failures are fail-closed. No interceptor can grant
capabilities, change sender identity, execute imported code or receive secret
payloads. The authenticated command/event and Electron panel expose only the
bounded verdict projection.

## Team Coordination Policies и Strategies v1 (планы 65 и 95)

TeamSpec и coordination policy принадлежат Core и versioned. Поддерживаются
RoundRobin, Selector, DirectedHandoff и RoleRouter. Core валидирует роли,
selector output, target и event refs; DirectedHandoff дополнительно требует
текущего owner. Loop/repeated-selection и team-turn limits являются частью
state machine. Policy state отдельно от transcript сохраняется в SQLite schema
v63 с optimistic version/idempotency; selection event — bounded metadata-only.
Coordination не запускает tools/shell и не расширяет child grants,
permissions или capability set. Authenticated IPC command 216/event 60 и
Electron developer panel показывают только Core projection.

Поверх этого policy-контракта план 95 добавляет versioned
`TeamCoordinationStrategy`: `RoundRobin`, `RuleSelector`, `ModelSelector`,
`HandoffSwarm` и `GraphDirected`. Core сначала фиксирует eligible roster из
TeamSpec, затем принимает от model selector только typed `ParticipantIdentity`
и отклоняет неизвестные роли. Для selector failure допускается только явно
записанный deterministic `fallback_role`; неявного выбора участника нет.
Swarm/graph переходы проходят проверку producer/consumer handoff из
immutable `ProtocolSnapshot` до dispatch. Strategy содержит
`session_id`/`protocol_hash`/revision, а `StrategySessionState` проверяет их
при каждом переходе; strategy и state сохраняются fenced/idempotently в
`team_coordination_strategy_snapshots`. Повторный выбор, последовательные
ходы и общий бюджет ограничены; route denial и stale snapshot не запускают
effect. Операция `select_strategy` использует существующий authenticated
command 216/event 60 и metadata-only Electron projection, поэтому новая
поверхность не создаёт capabilities и не переносит authority в renderer.

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

## Team Coordinator v1 (план 79)

Team Coordinator — Core-owned bounded orchestration surface поверх уже
разрешённых team/workflow boundaries. `TeamWorkItem`, `DelegationProposal`,
`SpecialistQuery`, `ParticipantCandidate`, `CoordinationReview` и
`DecompositionProposal` имеют schema v1, IDs до 128 bytes, максимум 64 work
items, 32 candidates, 16 consultations/children, 3 reassignments и 32 active
assignments. Coordinator только предлагает решение: Core повторно проверяет
capability/output compatibility, roster, revision и gates, не создаёт роли,
grants или внешний runtime. Unknown/stale/busy/incompatible результаты
типизированы и не превращаются в blind retry.

Durable SQLite schema v75 хранит work items, assignments, consultations и
managerial decisions. Authenticated IPC command 229/event 74 и Electron Team
Coordinator panel передают только bounded projection; queue, roster/load,
assignments, consultations и escalation не вычисляются в renderer. Raw
prompts/outputs, credentials, grants и executable identities запрещены.

## Persistent Agent Organization Registry v1 (план 118, реализован 2026-09-04)

`PersistentAgent` — Core-owned durable identity, отделённая от
`AgentRoleProfile` и от конкретного runtime `run`. Запись содержит только
bounded metadata: стабильный id/revision, display name, exact role-profile
reference (id/revision/hash), application/workspace/workspace-set/project
scope, reporting parent, responsibility selectors, exact Goal references,
policy references, lifecycle status и content hash. Credentials, prompts,
transcripts, raw outputs, capabilities/grants и process handles не входят в
контракт и не сохраняются.

Lifecycle — `draft → active → paused|suspended → active` либо terminal
`retired`; история revisions и reporting changes сохраняется append-only.
Reporting graph проверяется до записи: self-link, cycle, missing/retired
parent и cross-scope edge отклоняются. `AgentGoalBinding` фиксирует exact Goal
revision и одну из ответственностей `owner`, `contributor`, `reviewer`.

`AgentAssignment` ссылается только на уже существующий task/run/team-session/
handoff. Новый scheduler, runtime или процесс из этого контракта не создаётся.
Перед execution Core строит `ExecutionAgentSnapshot` с pinned agent revision,
role-profile reference, reporting chain, optional exact Goal revision и
accountability hash; missing or mismatched references дают typed broken
binding, без fallback. Availability/activity — derived projection из
assignment и существующих `runs`; cost остаётся `unavailable`, пока в Core не
появится authoritative agent-keyed source.

SQLite schema v92 добавляет metadata tables, immutable revision history,
goal-bindings, assignments и idempotency outcomes. Migration 90→92 сначала
активирует ранее неисполнявшуюся migration 91 для Guided Calibration, затем
устанавливает registry v92 в одной транзакции с backup до миграции.

Authenticated additive IPC использует command 259/event 104, protocol major
1, bounded JSON, correlation/request id, owner scope, expected revision и
idempotency key. Shell принудительно передаёт actor `user`; Core принимает
только `user`/`core` и повторно проверяет actor, scope, revision, graph и
references. Renderer не предоставляет отдельную вкладку для этой служебной
проекции. При старте Core assignments с потерянным
source переводятся в `unknown_after_restart`; неизвестное состояние не
объявляется успешным и retired history не удаляется.

## Task Ownership & Lease Fencing v1 (план 130, MVP реализован 2026-09-09)

`task_ownership_lease_fencing` формализует проверку owner, generation и
deadline поверх существующих `run_leases`, `agent_run_leases` и child
checkpoint leases. Wrong owner, stale generation и expiry fail closed; новый
ownership store или blind retry не добавляются.
