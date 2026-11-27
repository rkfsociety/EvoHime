# EvoHime — Windows desktop architecture

Статус: текущая утверждённая архитектура продукта. Обновлено: 2026-11-21.
Фактическое состояние реализации см. в [`current-state.md`](current-state.md).

EvoHime — локальное Windows-приложение.
Пользовательское короткое имя агента — «Ева».

```text
EvoHime.exe               Electron main + bundled renderer
updater\EvoHimeUpdater.exe  Electron update window inside updater module
        │ headless worker
evohime-updater.exe       Rust check/apply/rollback worker
        │ preload/contextBridge → desktop-ipc-v1 / named pipe
evohime-core.exe          agent loop, model gateway, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, restart, JSONL diagnostics
        │
evohime-transaction.exe   transactional update worker
```

Renderer не имеет node integration, не выполняет shell-команды и не открывает базу. Electron main ограничен окном, lifecycle, локальным состоянием оболочки и IPC adapter. Core владеет workspace, инструментами, моделью, секретами и локальным состоянием. Supervisor запускает core в Job Object и завершает дочернее дерево при остановке.

Улей (Hive / EvaPulse) — Core-owned контур Королевы, child-агентов и фоновых
наблюдений за репозиториями, изолированный от обычных чатов и их маршрутов.
Краткий доменный контракт и актуальное состояние реализации описаны в
[`hive-architecture.md`](hive-architecture.md); подробные границы делегирования
и typed boundary находятся в [`architecture/hive.md`](architecture/hive.md).

SQLite schema migration и idempotent installers выполняются на startup при
открытии `EventJournal`. Workspace RAG index/search/vector workers и выбранные
read-only запросы EventJournal используют общий ограниченный pool
уже подготовленных connections; обычные reads не запускают миграции и
installers. Pool ограничен четырьмя connections и сбрасывается после database
restore, чтобы новые операции подключались к заменённому файлу. Записи и
транзакционные операции остаются на основной journal connection и сохраняют
единственный порядок writer; pool не обещает параллельную запись SQLite.

Electron — единственная пользовательская оболочка пакета. Renderer и main
процессы Electron используют authenticated named-pipe transport к Rust Core;
второй desktop runtime в продукте не предусмотрен.

Исходящие IPC-кадры Core проходят через bounded `mpsc`-очередь ёмкостью 128.
`ChannelWriter` применяет backpressure и ждёт свободный слот через
`PollSender`, поэтому большой replay/resync не закрывает валидную сессию при
временном заполнении очереди. Соединение завершается только при закрытии
канала или фактической ошибке записи в pipe.

Конфигурация Core pipe закрыта по умолчанию: без authenticated launch context
Core завершается с ошибкой до открытия SQLite и создания endpoint. Legacy
handshake разрешён только при точном `EVOHIME_DEV_MODE=1`; заданный, но
повреждённый context не переключает запуск на legacy. Флаг разработки не
отключает аутентификацию supervisor context. Штатный packaged-запуск получает
контекст от supervisor; конфигурация сервера не допускает отдельного отключения
authentication. Консольные команды, не создающие pipe, не требуют context.

## Надёжная запись событий и границы payload

Обязательные события Core не используют `broadcast` как транспорт записи.
`TaskCoordinator` принимает их через bounded `tokio::mpsc`-очередь с ожиданием
свободного места; только после извлечения события из этой очереди и успешной
записи в durable journal оно отправляется в отдельный `broadcast` для
подписчиков. Потеря уведомления у медленного подписчика (`Lagged`) поэтому не
может остановить запись или скрыть событие от журнала. SQL-операция записи
выполняется в `spawn_blocking`, а backup/restore сохраняют общий database mutex
и также не выполняют синхронный SQLite на Tokio worker.

Для события, привязанного к разговору, основной journal row, все записи
conversation history и соответствующие `conversation.event` delivery rows
вставляются в одной SQLite-транзакции с одним commit. Ошибка между вставками
откатывает весь bundle; после перезапуска не возникает частичной проекции.

Ошибки journal и audit не отбрасываются: Core сохраняет последнюю ошибку в
состоянии и публикует `EventPersistenceFailed`. UI может показать этот
диагностический статус, а выполнение задачи не получает ложный успешный
результат. Для проверки есть stress-тест с очередью размера 1, быстрым потоком
событий и проверкой durable финального состояния.

В Electron bridge ограничения payload задаются явно в байтах UTF-8 для каждого
крупного поля. Общий helper коротких bounded-строк не используется для
полей, которым разрешены payload-ы от 64 KiB до 4 MiB; границы и Unicode
проверяются отдельными тестами. Доменные фасады `evohime-local-storage` вводятся
постепенно: memory bounded context уже экспортируется явно, остальные legacy
store-модули мигрируются без разрыва текущего Core API.

## Границы модулей и эксплуатационные ограничения

Core организован по доменам, а не одним набором крупных исходников. Координация
agent run разделена между routing, approval, dispatch, recovery и child
orchestration-модулями; IPC сгруппирован по workspace, memory, workflow, ambient,
review и terminal-доменам. Журнал, RAG, memory extraction и workflow runtime
также имеют отдельные модули для основной логики и тестов. IPC-слой остаётся
тонким: проверка envelope и маршрутизация выполняются на границе, а доменная
операция — в Core-owned сервисе.

Для новых изменений действует инженерная граница: исходные Rust-файлы Core
должны оставаться не больше 2000 строк. Doctor проверяет максимум строк,
количество допустимых `include!` и наличие индексов горячих запросов; это
проверка checkout, а не пользовательская настройка. Оставшиеся include-фрагменты
ограничены совместимыми prelude/header-границами, которые содержат общие
приватные определения и постепенно переводятся в обычные модули без изменения
visibility-контракта.

Context Budget Manager передаёт крупный текст и бинарные payload-ы через
`Arc`, чтобы планирование и offload не создавали повторные полные копии.
Fallback estimator сохраняет счётчик использования и предупреждает при доле
выше 5%; bounded LRU-кэш оценок использует стабильный ключ с версией
normalizer. Workflow runtime вычисляет canonical graph hash один раз на drive,
отклоняет графы глубже 64 узлов по пути и публикует bounded counters для
admission, hash-check, dispatch batch, dispatched nodes и ready-set.

Горячие SQLite-запросы typed event ledger покрыты partial/composite индексами
для terminal action, review lookup, workflow state, открытых попыток и
per-run event projection. Поддерживаются prepared statements и транзакционные
batch-вставки для журналируемых событий. Узкие Criterion-бенчмарки находятся в
`crates/evohime-core/benches/runtime_paths.rs` и покрывают context planner,
workflow hash и protobuf IPC envelope.

## Rust public API documentation

Публичный контракт Rust описывается doc-комментариями `///` непосредственно у
внешне достижимых элементов library targets. Полноту покрытия workspace
проверяет `cargo doc --workspace --lib --no-deps --locked` с `-D missing_docs`;
сборка полной документации также проверяет binary targets, а
`cargo test --workspace --doc --locked` исполняет compilable-примеры. Rustdoc
workflow дополнительно запрещает неразрешённые intra-doc links и некорректные
HTML tags. `#[cfg(doc)]` не нужен для включения rustdoc и может применяться
только для конкретного doc-only элемента, которому он действительно требуется.

## Ограничение фоновых задач Core

Detached-задачи, запускаемые IPC-мостом и координатором для agent run,
workflow drive, plan review/revision, backup/restore, extraction и индексации,
получают permit из Core-owned `BoundedTaskGroup` (`Semaphore + JoinSet`). Лимит по
умолчанию — 16 одновременно выполняемых задач на процесс Core. Новая работа не
ставится в бесконечную очередь: при отсутствии permit команда получает
`background task capacity is exhausted` (или durable task получает событие
ошибки). Permit живёт до завершения async-задачи, а завершившиеся handles
удаляются из `JoinSet` для workflow drives; остальные операции удерживают permit
до завершения своего `JoinHandle`. Короткие writer/pump-задачи конкретного authenticated
pipe-соединения и heartbeat остаются отдельными задачами с жизненным циклом
соединения.

Адаптеры (`workflow_adapters.rs`) ведут узлы в уже существующие контуры:
`child` — в `TypedChildTaskRequest`/report, `tool` и `mcp_tool` — в
`ToolRegistry` с тем же approval path, `context_provider` и `research` — в
read-only источники с проверкой свежести, `condition`/`transform` — в
deterministic-операции Core.

Approval узла решается существующей командой `ResolveApproval` и тем же
approval registry, что и у инструментов: отдельного workflow-approval нет.

## IPC

Контракт находится в `crates/desktop-ipc/proto/evohime.desktop.proto`.

- major-версия несовместима, minor-расширения совместимы;
- фреймы ограничены 4 MiB;
- события имеют монотонный `sequence_id`;
- UI может запросить replay после последнего sequence ID;
- cancellation передаётся отдельной командой `StopTask`;
- `SelectModelRequest` меняет модель следующего запроса без перезапуска Core: gateway разрешает модель на каждый вызов, пустое значение возвращает модель маршрута;
- провайдер `ollama` использует loopback-only OpenAI-compatible chat endpoint; `ModelCatalogRequest` возвращает установленные модели и Core-owned рекомендации по CPU, RAM, свободному диску и VRAM, а скачивание проходит через redacted `ollama_pull` projection;
- установка самого Ollama остаётся main-owned desktop-операцией: только allowlisted `https://ollama.com/download/OllamaSetup.exe` с разрешёнными официальными redirect-hosts, потоковая запись bounded installer во временный каталог, запуск без shell и последующая проверка `http://127.0.0.1:11434/api/version`; при `net::ERR_BLOCKED_BY_CLIENT` Chromium-загрузка повторяется через Node transport, а все URL/размер/header-проверки сохраняются; renderer получает только typed status/progress, а найденная Ollama не переустанавливается автоматически;
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

`sequence_id` монотонен, но не непрерывен: retention удаляет истёкшие
`ambient.%` строки и оставляет дырки в нумерации, поэтому дырка — не признак
потери состояния. `ReplayGap` означает только одно: запрошенный курсор старше
самого раннего сохранённого события. Оболочка не входит в `state-gap` из-за
дырки, а на `ReplayGap` сдвигает курсор на `earliest_available_sequence - 1` и
запрашивает `FullSnapshot`; повторные одинаковые gap-ответы ограничены, после
чего UI показывает явное состояние восстановления вместо бесконечного цикла
resync.

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

## Core-owned execution ledger

### Storage bounded contexts

`evohime-local-storage` сохраняет SQLite-владение в одном crate, но его
публичная организация постепенно переводится от таблиц к доменам. Внутренние
фасады `domains::{memory,runs,workflow,agents,audit,receipts}` задают целевые границы:
каждый домен объединяет связанные store-операции и является местом для
дальнейшего переноса типов и транзакционных сервисов. Старые `*_store` модули
пока остаются публичными для совместимости существующих Core-вызовов.

Эволюция схемы проходит через `src/migrations.rs`. Исторический installer
v1..v92 временно вызывается из этого boundary, а новые версии должны
добавляться небольшими numbered installers внутри migrations, не расширением
`LocalDatabase::open` и не новым произвольным CRUD-store. Это позволяет
переносить миграции и доменные store-ы поэтапно, сохраняя одну транзакцию,
backup и rollback semantics.

Для legacy bootstrap действует правило единственного источника: перенос
крупного inline DDL выполняется целиком вместе с его migration-тестом, после
чего старый блок удаляется. Промежуточные дубликаты SQL запрещены, чтобы
история схемы не расходилась между `lib.rs` и `migrations/`.

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

## Данные, диагностика и восстановление

SQLite находится в `%LOCALAPPDATA%\EvoHime` либо в `EVOHIME_DATA_DIR`. Миграции выполняются транзакционно; перед изменением схемы создаётся `.db.bak`. Журнал событий экспортируется в JSONL. Логи core и supervisor пишутся в `%LOCALAPPDATA%\EvoHime\logs`. Permission-правила читаются из `%LOCALAPPDATA%\EvoHime\permissions.json` как упорядоченный JSON-массив PolicyRule: побеждает последнее совпавшее правило, отсутствующий или пустой файл означает встроенный набор, пустой массив `[]` означает осознанное отключение правил. Обновление использует отдельный transaction worker, backup компонентов и recovery незавершённой транзакции перед запуском Core.

Локальное состояние оболочки лежит рядом, в `%LOCALAPPDATA%\EvoHime\shell\`:

| Файл | Содержимое | Ограничения |
| --- | --- | --- |
| `workspaces.json` | список запомненных папок и последняя выбранная | нормализованные пути |
| `chats.json` | чаты, привязанные к workspace, и отправленные промпты | 100 чатов на workspace, 500 сообщений на чат, 4096 символов на промпт; кешируется в main, записи последовательно выполняются асинхронно через временный файл и атомарный `rename` |
| `provider.json` | выбранный провайдер, профили API-провайдеров, модели, пауза, общие и модельные RPM/RPH/RPD-лимиты, дневная квота токенов, зашифрованные ключи | режим `600`, запись через временный файл и `rename`; старый формат мигрирует при чтении |

Отсутствующий файл означает пустое состояние. Ошибка чтения или повреждённый
JSON не подменяются пустым состоянием: операция получает typed failure, а
исходный повреждённый файл сохраняется рядом как recovery-копия. Это не даёт
последующей записи затереть данные, которые ещё можно восстановить.

## Бюджет запуска

`evohime_core::run_policy` описывает неизменяемый snapshot политики одного запуска: `max_iterations`, `max_wall_clock_ms`, `max_tool_calls`, `max_tokens`, `max_cost_micros` и `approval_required`. Core проверяет счётчики перед отправкой эффекта; превышение любого из них останавливает запуск с `BudgetExceeded`. Renderer может показать snapshot, но не может поднять лимит в середине запуска.

`evohime_supervisor::pulse` описывает контракт локального digest расписаний: dead-letter даёт `Failed`, пропуски и неуспехи — `Degraded`; успешный счётчик никогда не маскирует отказ. Модуль пока никем не вызывается: пользователь видит статус Pulse в `OperationsPanel`, где он выводится из событий `runtime.schedule_failed`/`runtime.schedule_dead_letter`.

## Packaging и запуск

```powershell
.\scripts\build-windows-native.ps1
```

Для разработки используется `start-dev.ps1`; он читает `.env` по allow-list имён из `.env.example` и передаёт их только дочерним native-процессам. Для первой установки GitHub Actions собирает единственный маленький сетевой `EvoHime-Setup.exe`: он размещает только `updater\EvoHimeUpdater.exe`, `evohime-updater.exe` и bootstrap marker, создаёт один ярлык и передаёт загрузку полного комплекта модулей. Публикуемый `updater.zip` содержит UI и worker со встроенным transaction engine, поэтому updater может восстановить полный install tree без второго installer.

Bootstrap source имеет bounded marker `evohime.bootstrap.v1` и собирается из
уже опубликованного `module-updater` artifact. В нём
нет shell, Core, supervisor, listener, verifier, component manifest или
runtime-моделей: их exact versions, размеры и SHA-256 получает updater из
fixed `compatibility` manifest. Поэтому обычное изменение module releases не
пересобирает web installer; workflow запускает только узкий packaging gate.

Пакет x64 предназначен для Windows 10 2004+ и Windows 11 и содержит bundled Electron runtime, Rust runtime и локальные компоненты; отдельная установка Node.js или браузера не требуется.

## Карта доменных контрактов

Подробные доменные контракты вынесены в [`architecture/`](architecture/):

| Файл | Домен |
| --- | --- |
| [`architecture/hive.md`](architecture/hive.md) | Улей (Hive), делегирование, роли, SOP и координация команд |
| [`architecture/core-services.md`](architecture/core-services.md) | Микроядро Core, `evohime-service-contract`, `ModuleRegistry`, `core-assets`, реестр 26 модулей и hot-restart сервисов |
| [`hive-architecture.md`](hive-architecture.md) | Аудитное описание Hive/EvaPulse: настройки, маршруты, proactive evolution и Electron UI |
| [`architecture/workflow.md`](architecture/workflow.md) | Workflow/v1, оркестрация, триггеры, event bus и фоновое исполнение |
| [`architecture/plan-review.md`](architecture/plan-review.md) | Ревью и правка планов, Plan Artifact и Architecture Drift Review |
| [`architecture/model-routing.md`](architecture/model-routing.md) | Model Gateway, профили провайдеров, ключи, маршрутизация, каскады и стратегии промптов |
| [`architecture/local-models.md`](architecture/local-models.md) | Локальные модели, калибровка, адаптация, планировщик и hardware fit |
| [`architecture/context-knowledge.md`](architecture/context-knowledge.md) | Контекстный бюджет, память, RAG, чекпоинты, навыки и знания проекта |
| [`architecture/review-quality.md`](architecture/review-quality.md) | Верификация, quality contract, review lanes, телеметрия и оценка |
| [`architecture/capabilities.md`](architecture/capabilities.md) | Инструменты, адаптеры, мосты, CLI, голос, зрение, git-публикация и сервисный граф |
| [`architecture/shell-ui.md`](architecture/shell-ui.md) | Оболочка Electron, визуальный контракт, UI-расширения и self-healing updater |
| [`architecture/policy-receipts.md`](architecture/policy-receipts.md) | Политики исполнения, approval gate, guardrails и signed receipts |
