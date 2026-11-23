# EvoHime — текущее состояние

Обновлено: 2026-11-21.

Этот файл описывает подтверждённое состояние текущего checkout. Исторические
release-gates и результаты отдельных завершённых планов находятся в
[`release-evidence.md`](release-evidence.md); пошаговые незавершённые работы — в
[`plans/README.md`](plans/README.md).

## Улей (Hive / EvaPulse)

Улей реализован как Core-owned контур Королевы, child-агентов и отдельного
системного чата `hive-system-chat`; обычные чаты не попадают в Hive snapshot.
EvaPulse — независимый фоновый daemon: он читает journal observations, соблюдает
бюджет и изолированный каскад `hive` → `hive_fallback` → `Sleep`, а валидированные
сигналы передаёт `HiveSignalReceiver`. API-ключи Улья хранятся shell/main в
зашифрованном `provider.json` через Electron `safeStorage`/Windows DPAPI и
передаются Core только через окружение supervisor. Подробное описание всех
полей `HiveSettings`, proactive evolution, UI и известных ограничений находится
в [`hive-architecture.md`](hive-architecture.md); канонические typed boundaries —
в [`architecture/hive.md`](architecture/hive.md).

Сцена города Улья (`HiveCityScene`, живой архипелаг plan 199) разрабатывается
через dev-стенд Hive Studio: `npm run studio` в `desktop/evohime-electron`
поднимает Vite-сервер со страницей `http://localhost:5178/hive-studio.html` —
настоящая сцена в обычном браузере; источник данных — детерминированная
симуляция или live-опрос наблюдательного pipe `evohime-hive-daemon` через
dev-middleware `vite.studio.config.ts`. Страница и её CSS не входят в
production-бандл (rollup input перечисляет только `index.html`).
Раскладка кварталов столицы и спутников — радиально-кольцевая на общей сетке
из 32 спиц (`buildIslandView` в `src/shared/hive-city-map.ts`): кольца кварталов
сидят посередине между кольцевыми проспектами, все кольца делят одни и те же
спицы (столбцы сходятся в лучи), коридор главных ворот свободен, башни не
попадают на кардинальные коннекторы развязок.

## Независимые EvaPulse и Hive (plan 198, закрыт 2026-11-21)

EvaPulse и Hive разделены на уровне Core: `HiveSignalReceiver` принимает только
версионированный bounded signal, проверяет TTL/identity/idempotency и создаёт
Hive-owned task через обычный coordinator path. Acceptance и dead-letter записи
пишутся в bounded journal retention; повторная доставка не создаёт второй task.
Проверка срока действия выполняется в момент приёма, а не по времени создания
сообщения. Lifecycle Pulse отдаётся отдельной `eva_pulse.status` projection и не
входит в сериализуемый Hive snapshot.

Реализация подтверждена CI на `1246e503a477ed4f64563e04061365f5c07b4ff8`:
workflow `module router`, `core module`, `ui-bundle module`, `Rust
documentation` и `compatible release manifest` завершились `success`, Core
опубликован как `module-core-v0.0.000504`, UI bundle — как
`module-ui-bundle-v0.0.0225`. OperationsPanel больше не представляет EvaPulse
как переключатель или режим Улья: состояние Pulse отображается диагностически,
а typing Королевы и ручное обновление snapshot не зависят от Pulse lifecycle.
`core.togglePulse` сохранён как authenticated compatibility/diagnostic command
для старых клиентов. Контракт границы перенесён в
[`architecture/hive.md`](architecture/hive.md), временные plan-файлы удалены.

Headless Linux CLI `eva` автоматически читает настройки провайдера из
`$XDG_CONFIG_HOME/evohime/provider.env` (по умолчанию
`~/.config/evohime/provider.env`) перед запуском Core. Файл и каталог получают
права `0600` и `0700`; явные переменные процесса имеют приоритет.
При интерактивном `eva run` Core-запросы разрешений подтверждаются в терминале;
без TTY ожидание разрешения завершается отказом и кодом 3.

## Внешний вид оболочки

Основная навигация организована как постоянная узкая панель значков, а
контекстная колонка отдельно показывает выбор рабочей папки, создание диалога
и историю чатов. Это отделяет глобальные инструменты от содержимого проекта.
Переключение workspace доступно в левой панели и использует существующие
shell-команды workspace.list/select/pick; composer показывает выбранную
рабочую папку и сохраняет параметры разрешений, провайдера и модели. Сочетания
Ctrl+K (новый чат) и Ctrl+, (настройки) доступны из любой вкладки.
Композер принимает файлы из буфера обмена через Ctrl+V, повторно проверяя, что
они принадлежат выбранному workspace; обычная вставка текста не меняется.
Вспомогательные Workbench/Browser/Trace поверхности не должны сжимать чат до
нечитаемой колонки. Подробная схема, визуальные правила и критерии проверки
описаны в [ui-design.md](ui-design.md). Core ownership, approvals и IPC
семантика не меняются.

Цель выполнения принадлежит конкретному чату: UI закрепляет её только над
композером этого чата, а `ListGoals` принимает обязательный `chat_id` и Core
фильтрует projection по паре workspace/chat. Общего списка «постоянных целей»
в workspace Overview нет; агентское создание цели без активного чата отклоняется.

Settings «Внешний вид» управляет темой (системная, светлая, тёмная), масштабом,
плотностью элементов, цветом акцента и уменьшением анимации. Выбор применяется
сразу и сохраняется в localStorage renderer оболочки; Core и его состояние не
затрагиваются. Там же выбирается язык интерфейса: русский или English. Выбор
сохраняется отдельно от темы и параметров оформления; строки UI и форматирование
дат/чисел обновляются сразу. Сброс оформления сохраняет выбранный язык. Язык
передаётся и системному меню трея.

## Интеграции GitHub

Settings «Интеграции» поддерживает сохранение до 50 публичных GitHub
репозиториев в Core SQLite (schema v180). Core обращается к GitHub только по
явному запросу пользователя: показывает сводку и не более 10 открытых issues и
10 pull requests. Используется только анонимный HTTPS GET к `api.github.com`;
redirect отключён, каждый ответ ограничен 512 KiB, общий timeout — 25 секунд.
Вход в аккаунт, приватные репозитории и операции записи не поддерживаются.
Проверки реализации и доставки для этого checkout отмечены в
[`release-evidence.md`](release-evidence.md).

## Триггеры событий

Settings → «Триггеры событий» создаёт versioned правила с привязкой к конкретной
версии встроенного workflow. На Windows доступны события завершения/ошибки задач
Core из durable journal и изменения файлов внутри выбранной рабочей папки через
`ReadDirectoryChangesW`. Включённые правила проходят обычную проверку
capability/approval в WorkflowRuntime; история содержит только bounded metadata.
Изменения в рабочей папке пропускаются с outcome `dropped_with_audit`, пока в ней
выполняется workflow, запущенный триггером, чтобы избежать рекурсивного запуска.
Webhook-провайдер не подключён.

`evohime-local-storage` сейчас имеет внутреннюю migration boundary
`src/migrations.rs` с numbered installers для v001–v026, v032–v039,
v042–v116 и v149–v181, а также
bounded-context фасады в `src/domains.rs`. Все исторические installers теперь
вынесены из `lib.rs`; старые store-модули сохраняются только там, где они
ещё являются совместимым публичным контрактом, а новые доменные вызовы
должны проходить через фасады.
В Core внутренние workflow/runtime-модули также закрыты на уровне crate;
проверка ссылок в workspace используется как критерий дальнейшего сужения API.
На текущем этапе `evohime-local-storage` экспортирует 111 публичных модулей
(остальные реализации закрыты на уровне crate), а `evohime-core` — 113;
дальнейшее
сокращение оставшихся модулей требует миграции их фактических потребителей
в доменные фасады.

## Core-owned A2A remote-agent bridge (plan 180, implementation stages 1–3)

`crates/evohime-core/src/external_agent_a2a.rs` добавляет bounded Core-owned
contract для outbound A2A remote-agent backend как protocol/backend extension
существующего external-agent owner. Профили, untrusted card snapshots,
delegation requests, context capsules и task bindings детерминированно
валидируются, хэшируются SHA-256 и имеют типизированные переходы состояний.
Card claims не дают локальных capability; сырые карты, заголовки, куки,
транскрипты, токены и сырые цели не сохраняются и не проецируются — только
id, ссылки, хэши и типизированное состояние. Неоднозначная отправка становится
`UnknownOutcome` и не может превратиться в слепой повторный submit.

`crates/evohime-local-storage/src/external_agent_a2a_store.rs` даёт
metadata-only persistence (schema v181) для профилей, хэшей card snapshots и
task bindings; миграция `v181.rs` создаёт `a2a_remote_profiles`,
`a2a_card_snapshots` и `a2a_task_bindings`. `ExternalAgentProtocolKind`
расширен версионированным `A2a` backend kind с проверкой версии протокола в
`validate_preset`. Runtime facade выполняет только outbound profile-owned
discovery/submit/status/cancel с bounded body, redirect-off и post-DNS
SSRF/private-address gate; remote artifacts принимаются только inline после
quota/MIME/hash validation. Authenticated external-agent IPC предоставляет
redacted A2A operations, а restart recovery переводит nonterminal bindings в
reconcile-only states. Inbound listener, dynamic discovery, credential
forwarding и второй runtime/scheduler/storage authority отсутствуют.

## Image generation and editing

Core exposes typed generate/edit/mask-edit jobs through the existing
ModelGateway and authenticated desktop IPC. Schema v177 stores bounded,
redacted lifecycle metadata only; validated PNG/JPEG outputs are published as
owner-scoped ArtifactStore references after decoded geometry, MIME, signature,
hash and aggregate quota checks. Current providers without an advertised image
output capability return typed unavailable; no live provider generation was
verified. The Operations panel exposes generate/edit when Core advertises the
capability and otherwise shows its unavailable state.

## Актуальный code baseline

`shell_execute` и `process_run` перед созданием процесса совместно проверяют
имя executable и аргументы: известные сетевые клиенты/туннели, домены
`*.trycloudflare.com` и комбинации ссылок на `.env*`, SSH/AWS/Solana credentials,
keystore или сигнатур секретов с upload/POST-признаками отклоняются fail-closed.
Это блокирует известную форму command-based сбора и отправки секретов, но не
является сетевой изоляцией ОС и не гарантирует перехват собственного сетевого
кода произвольного executable; при отсутствии process-level guarantee внешний
dispatch получает typed `unsupported`/`blocked` outcome.

Core-owned sensitive egress boundary завершена в `sensitive_data_guardrails`:
versioned detector pack v1 классифицирует email/credentials/private keys,
phone, Luhn payment cards, IBAN mod-97 и contextual tax IDs без хранения raw
findings. Classification summary поднимает `required_privacy` до route
selection, а exact final payload перед provider/tool/agent effect повторно
проходит destination-aware gate с payload/policy/detector binding. Schema v184
добавляет metadata-only `sensitive_egress_decisions`; OCR/image/audio и remote
sanitizer остаются явно unsupported.

Все 22 workspace packages проходят локальный rustdoc gate для каждого library
target с `-D missing_docs`, `broken_intra_doc_links` и `invalid_html_tags`.
Внешне достижимые Rust API снабжены предметными `///`-описаниями; примеры
исполняются как workspace doc-tests. CI gate определён в
`.github/workflows/rustdoc.yml`; точный post-push результат фиксируется в
`release-evidence.md`.

Текущий опубликованный commit и результаты CI для закрытых направлений
фиксируются в последней записи [`release-evidence.md`](release-evidence.md).
Актуальные release markers берутся из `release-versions/`: `analysis-worker
0.0.000045`, `cli 0.0.000103`, `core 0.0.000504`, `installer 0.0.000066`,
`listener-runtime 0.0.000045`, `listener 0.0.000046`, `shell-host 0.0.000134`,
`supervisor 0.0.000048`, `transaction 0.0.000071`, `ui-bundle 0.0.0225`,
`updater 0.0.000135` и `verifier 0.0.000059`.

Новые задания чата получают из Core conversation log последние 40 durable
реплик пользователя и ассистента (до 64 KiB), сохранённые до текущего запроса.
Они поступают в модель с исходными ролями и учитываются общим context budget;
текущий запрос добавляется после истории. Путь одинаков для model provider и
Codex CLI, а ошибки чтения истории не допускают молчаливого запуска без неё.

Startup `EventJournal::open` выполняет миграции и idempotent schema installers
один раз. Длительные workspace RAG index/search/vector операции используют
bounded pool из подготовленных SQLite connections через `LocalDatabase::open_prepared`;
поисковый запрос больше не устанавливает схему. Pool инвалидируется после
database restore, чтобы следующие операции открыли заменённый файл.

Replay/latest-sequence/review history, conversation history paging и выбранные
Workspace RAG status/provenance reads используют тот же pool
с пределом в четыре prepared connections. Эти read-only операции больше не
держат основную `EventJournal` mutex connection; writes и atomic
read-modify-write остаются на ней и сериализованы. Pool сохраняет restore
generation fence. SQLite schema и IPC не изменялись.

Путь typed execution ledger не выполняет отдельный `BEGIN`/`COMMIT` для каждой
строки: `LocalDatabase::append_ledger_events` валидирует bounded batch, переиспользует
подготовленные SQLite statements и публикует весь batch одной транзакцией. При
ошибке проверки terminal outcome транзакция откатывается целиком; одиночный
`append_ledger_event` остаётся для независимых событий. Startup reconciliation
также использует этот batch-путь. Это устраняет лишние commit-затраты в связанных
операциях ledger, не перенося runtime-состояние из Rust Core.

## Durable event delivery

События, которые обязаны попасть в журнал, проходят через bounded ожидающую
`mpsc`-очередь `TaskCoordinator`, а не через lossy `broadcast`. Отдельный
уведомительный `broadcast` получает событие только после journal write, поэтому
`Lagged` у клиента не прерывает запись. Journal SQL, backup и restore выполняются
в blocking worker; ошибка journal или audit удерживается в Core как
`persistence_error` и публикуется событием `EventPersistenceFailed`.
Для событий, связанных с разговором, основной journal, conversation history и
`conversation.event` delivery записываются одним SQLite transaction/commit;
искусственный сбой между вставками проверяется regression-тестом и после
reopen не оставляет частичных строк.

`sequence_id` монотонен, но не непрерывен: retention удаляет истёкшие
`ambient.%` строки и оставляет дырки в нумерации. Оболочка переносит такую
дырку как обычное продвижение курсора и продолжает поток; в `state-gap` она
входит только по типизированному `ReplayGap`, который Core выдаёт тогда, когда
запрошенный курсор старше самого раннего сохранённого события. На gap оболочка
сдвигает курсор на `earliest_available_sequence - 1` и запрашивает
`FullSnapshot`, а серия одинаковых gap-ответов ограничена тремя попытками —
иначе UI навсегда оставался в состоянии «связь с Core потеряна».

`model.context` дополнительно фиксирует bounded lifecycle-поля вызова: номер
итерации, лимит tool-capable итераций, `tool_calls_allowed` и typed
`terminal_reason`. Поэтому trace может отличить обычный вызов с доступными
инструментами от последнего tool-free шага `iteration_limit_final_answer`;
текст итогового сообщения модели не считается доказательством недоступности
инструментов. Старые IPC-клиенты игнорируют эти additive optional fields.

Для сквозного разбора работы агента Core также пишет в тот же durable journal
события `model.trace` с фазами `request` и `response`. В них видны итерация,
модель и маршрут, размеры контекста, число tool schemas/tool calls, фактический
usage, latency и request/response IDs; тело prompt, reasoning и ответа модели
не сохраняются, а для ответа остаётся только content digest. `TracePanel` и
Markdown-экспорт показывают эти события рядом с `model.context`, tool calls,
approval и terminal outcome, поэтому последовательность «контекст → вызов
модели → ответ → инструмент/следующий вызов» восстанавливается по одному
conversation-bound trace.

Conversation-bound `task.failed` trace events передаются в renderer как
redacted terminal projection: в payload остаются только bounded `error_code`,
`source` и `operation` (или безопасная детерминированная классификация), а для
filesystem boundary failures — только безопасные `path_form`, `path_scope` и
`path_boundary_reason` без самого пути.
При malformed или oversized исходном payload сохраняется fallback
`task_failed/core/task.execute`, а не generic или raw payload. Полный error text,
URL, prompt и секреты не сохраняются в этой проекции;
Trace UI показывает диагностические поля отдельно и включает их в Markdown
под заголовком `diagnostics`. Renderer повторно нормализует `task.failed` перед
отображением и экспортом, сохраняя тот же fallback для legacy payload.
Тот же safe projection применяется при записи исходной строки `task.failed` в
глобальный durable event journal, поэтому replay больше не возвращает raw
`error`; fallback transcript и recovery UI показывают только `error_code`.
Обычные conversation-bound trace payloads передаются как projection v3:
тип события, projection kind, фаза, безопасные имена инструментов, bounded
счётчики и размеры payload без самого prompt, arguments, tool output, URL или
секретов. Ошибки filesystem-инструментов дополнительно попадают в bounded
`tool_failures` summary с формой пути, областью и причиной отказа. Поля содержимого остаются redacted; main bridge по-прежнему
отклоняет необработанные URL, prompt и секретные assignment-поля. Экспорт
Markdown дополнительно содержит deterministic summary с диапазоном sequence,
числом событий по типам, outcome задачи, сводкой tool calls и routing statuses.
Для ошибки `ollama.download` trace отдельно показывает, был ли фактически
получен allow-listed marker `shell.ollama_download_fallback`; URL установщика и
raw shell-log payload в renderer не передаются.
При наличии marker он также отображается отдельной карточкой «События
оболочки», чтобы экран и Markdown показывали один и тот же факт.

Экспорт трейса дополнен читаемым транскриптом беседы: `TracePanel` читает
Core-owned журнал conversation через `core.getConversationEvents` (те же
renderer-payload'ы, что показывает чат) и собирает Markdown-раздел `transcript:`
с промптом пользователя, текстом ассистента, вызовами инструментов, их выводом
и терминальной ошибкой; та же секция «Транскрипт беседы» показывается в панели.
`CoreEvent::ToolStarted` и conversation-событие `tool_started` теперь несут
bounded guardrail-redacted `arguments` (≤ 8 KiB, при переполнении суффикс
`…[truncated]`; при блокировке guardrail — `<sensitive_data_blocked>`), поэтому
экспорт объясняет, что именно модель просила сделать. Вывод инструмента для
`CoreEvent::ToolOutput` ограничен 32 KiB с явным суффиксом
`…[tool output truncated; request the rest with a smaller limit or offset]`:
раньше чтение файла около 65 KiB не помещалось в conversation payload и событие
подменялось маркером `conversation.projection_failed`. Исходящий текст
проверяется egress-guardrail чанками по границам строк, поэтому большой tool
output редактируется целиком, а не роняет задачу по размеру. Причина отказа
guardrail остаётся в `task.failed` стабильным кодом (`sensitive_data_blocked`,
`egress_payload_too_large`, `egress_guardrail_unavailable`), а не generic
`task_failed`, и дублируется в model-trace событием `model.egress_blocked` без
тела сообщения. Redacted projection v3
живого потока по-прежнему не несёт prompt, arguments или tool output: транскрипт
строится из журнала conversation, а не перескакивает projection boundary.
Контент транскрипта проходит ту же shell-границу `asTraceContent`, что и раньше:
URL заменяются на `[URL]`, секретные assignment-поля — на `[REDACTED]`.

Добавлены проверки очереди размера 1 с потоком событий, задержкой чтения и
финальным `task.completed`, а также проверка явного уведомления об ошибке audit.
В Electron bridge крупные поля проверяются отдельным UTF-8 byte-bound helper,
включая Unicode и превышение 512 KiB. В workflow `electron-heavy` real-Core IPC
E2E запускается отдельным обязательным шагом после сборки Core с
`EVOHIME_REQUIRE_REAL_CORE_E2E=1`; локальный режим по-прежнему может пропускать
этот тест без собранного Windows Core.

Исходящая IPC-очередь Core остаётся bounded (`128` кадров), но при заполнении
теперь применяет асинхронный backpressure через `PollSender`, а не закрывает
валидное соединение с ошибкой `outbound IPC frame queue is full`. Регрессионный
тест `channel_writer_waits_for_capacity_instead_of_closing` подтверждает, что
последовательные кадры replay дожидаются чтения writer-задачей.

## Support bundle и автоматический отчёт об ошибке

Electron main собирает `evohime-support-bundle-v2` только из bounded redacted
projection. ZIP использует deflate, когда это уменьшает размер. Ручная
`shell.submitDiagnostics` и автоматический отчёт для живого `task.failed`
используют одну GitHub authorization chain: `EVOHIME_UPDATE_GITHUB_TOKEN`,
`update.json`, `GH_TOKEN`/`GITHUB_TOKEN` или `gh auth token`. Bundle, который не
помещается в issue, автоматически отправляется в secret/unlisted Gist как
base64 ZIP, после чего issue содержит ссылку, размер и SHA-256. Старые
replay/resync failures при запуске не отправляются; один task ID claim-ится не
более одного раза за процесс. Отказ GitHub/Gist остаётся best-effort shell
diagnostic и не меняет исходный Core failure.

Electron JSONL diagnostics redacts before enqueue, uses one serialized async
writer per process, and bounds pending records by count and UTF-8 bytes. Main
shell and updater write separate `shell-main.jsonl` / `shell-updater.jsonl`
files so their independent rotation cannot race. Controlled shutdown performs
a bounded flush; a timeout reports only a fixed message to stderr and allows
exit. Support-bundle runtime metadata includes the redacted logger status. The
chat event boundary keeps compact Core instance/epoch/sequence metadata, while
conversation projection helpers own replay deduplication; only the timestamp
LRU is retained locally and it is capped at 256 entries. Workflow event polling
uses `afterSequence`, correlates projections by run ID, drains full pages, and
stops after terminal catch-up. See [`architecture.md`](architecture.md) and [`architecture/workflow.md`](architecture/workflow.md).

## Продуктовая граница

EvoHime — локальное Windows desktop-приложение с одним пользовательским
ярлыком `EvoHime`. Внутри пакета работают Electron shell, updater-модуль с
каталогом `updater\EvoHimeUpdater.exe` и Rust worker `evohime-updater.exe`, `evohime-core.exe` и
`evohime-supervisor.exe`; Core владеет состоянием, SQLite, правами и эффектами,
а renderer получает только проекцию через authenticated versioned named pipe.

В текущий release scope входят Windows 10 2004+ / Windows 11 x64 и один
web-релиз `installer` для первой установки, обновления и восстановления.
Новые версионные релизы, публичный HTTP
server, внешний Node.js runtime, cloud control plane и обязательная GPU-зависимость
не входят в продукт.

## Runtime и упаковка

| Слой | Реализация | Подтверждение |
| --- | --- | --- |
| UI | Electron 45.3.3, React 19.3.1, TypeScript 7.0.2, Vite 8.3.8 | `desktop/evohime-electron/package.json` |
| IPC | `desktop-ipc-v1`, protobuf bindings, HMAC-сессия supervisor | `crates/desktop-ipc/`, `npm run check:protocol` |
| Core | Rust agent runtime, tools, SQLite и provider gateway | `crates/evohime-core/`, `crates/model-gateway/` |
| Supervisor | mutex, Job Object, lifecycle и recovery | `crates/evohime-supervisor/` |
| Native package | Electron shell `EvoHime.exe`, updater-модуль `updater\EvoHimeUpdater.exe` + Rust worker, Core, supervisor, `eva.exe`, analysis worker, listener, transaction и verifier | `scripts/build-windows-native.ps1` |
| Installer | Маленький web `EvoHime-Setup.exe`, который после запуска получает полный комплект модулей | `installer/EvoHime.iss`, `.github/workflows/installer.yml` |

## Headless CLI на Linux

`eva` запускается на Linux и автоматически поднимает соседний `evohime-core`.
Клиент использует существующие Core-команды через owner-only Unix socket и
проверяет UID peer и HMAC session proof. Сборка из checkout:
`cargo build --locked --release -p evohime-cli -p evohime-core`, запуск:
`./target/release/eva doctor --json`. Для выполнения задач настройте провайдера
переменными окружения; Ollama использует `MODEL_PROVIDER=ollama` и
`OLLAMA_MODEL`. Electron shell, Windows supervisor и supervisor-dependent
операции остаются Windows-only.

Для разработки используется PowerShell 7+ и Node.js 22 LTS. В установленный
клиент не вносятся изменения: диагностика и проверки выполняются в исходниках,
временных каталогах или CI.

## Web installer

Упрощённый web-контур: installer source собирается из
опубликованного самодостаточного updater module, проверяется marker- и content-gate
и упаковывается `installer/EvoHime.iss`. Первый запуск открывает
самостоятельный updater, получает fixed `compatibility` manifest и применяет
полный комплект модулей через verified staging/rollback; shell до успешного
apply не запускается. Публикация манифеста останавливается, если последний
релиз updater ниже указанного минимума. Отдельного offline/full installer нет.

Окно updater остаётся видимым во время проверки, скачивания и верификации и
показывает текущий процент. Headless worker запрашивает закрытие окна отдельным
`requires_exit` только непосредственно перед заменой файлов; после self-update
окно запускается повторно, а после общей транзакции запускается Ева.

Версии поставляемых модулей не дублируются отдельной таблицей в статусе: их
канонический источник — файлы `release-versions/*.txt`, перечисленные
выше для текущего baseline.

Plan 174 реализует Core-owned free-access evidence, consent-gated probes,
strict routing и redacted UI projection. Evidence привязано к уникальному
opaque binding сохранённого ключа и текущему profile revision/hash; при
ротации ключа старые rows удаляются до hydration, а ошибка cleanup fail-closes
Core startup. OpenRouter exact-zero pricing plus semantic completion — пока
единственный authority для `VerifiedFreeLimited`. Cloudflare codes 5035/3036
сохраняются только как typed `ActivationRequired`/`QuotaRejected`; успешная
проверка Cloudflare без pricing authority остаётся `Unknown`. `FreeOnly`
останавливает stale/unknown/activation/quota evidence, а `PreferFree` требует
явного paid-fallback option. Реализация, локальные gates и полный GitHub CI
успешны для commit
[`7fa1af30ff2071da390828c1137a7df59308a864`](https://github.com/rkfsociety/EvoHime/commit/7fa1af30ff2071da390828c1137a7df59308a864);
детальные workflow и module release evidence записаны в
[`release-evidence.md`](release-evidence.md).
План 175 реализован и опубликован: затронутые Core и UI bundle module
workflows, module router и Rust documentation gates завершились успешно.
Полный native package acceptance был отменён до завершения после уточнения
модульного scope; его частичный вывод не считается evidence. Точные module tags
и CI evidence приведены в [`release-evidence.md`](release-evidence.md).
SQLite schema v176 добавляет metadata-only extraction source, provenance links,
fenced lease и candidate freshness/head CAS поверх неизменённой v175 истории.
Core durable-записывает eligible source до `TaskCompleted`, сохраняет typed
origin/root/depth, сериализует dialog/ambient извлечение cancellation-safe
RAII lease и восстанавливает bounded source через существующую background
reconciliation точку. Auxiliary extractor dispatch требует durable Model
Request Provenance v1 envelope, receipt и dispatch marker; failure блокирует
provider вызов и записывается как typed lifecycle outcome. Event
`memory.extraction` и read-only `OperationsPanel` показывают только bounded
metadata. Основное тело памяти остаётся в единственном Memory storage owner.
Затронутые module markers повышены только для `core` и `ui-bundle`; подтверждение
их публикации и полных GitHub gates будет добавлено в
[`release-evidence.md`](release-evidence.md).
План 173 Cloud Provider Profiles закрыт: явная profile identity и отдельный
Cloudflare account ID проходят через encrypted provider store и supervisor
environment; Core хранит bounded profile/catalog snapshots в SQLite schema
v174, выполняет discovery и capability preflight, а при старте восстанавливает
только валидированные snapshots. Каталог имеет lifecycle
`Fresh/Stale/Unavailable/CredentialRejected/DiscoveryUnsupported`, typed
failure codes и deterministic deduplication; provider errors не раскрываются в
projection. Stale/expired/failed snapshots и подтверждённое отсутствие модели
не проходят route preflight; `DiscoveryUnsupported` не блокирует явно
выбранный model ID. Только подтверждённое `Unsupported` для chat или
запрошенных tool calls блокирует dispatch. Неизвестные capability и исходное
provider health остаются `Unknown`. Bounded redacted `provider_catalog`
передаётся через существующий authenticated `model.catalog`; ProviderForm и
ModelPicker отображают Core-owned profile, lifecycle и capability state.
Полный контракт находится в [`architecture/model-routing.md`](architecture/model-routing.md), проверки —
в [`release-evidence.md`](release-evidence.md).
Каталог моделей провайдера принимается в пределах 16 MiB и 4096 записей: этот
предел вмещает реальные ответы OpenAI-совместимых агрегаторов (LiteRouter
отдаёт около 4,2 MiB и 1743 модели).
Сжатый контекст — это представление одного вызова модели, а не замена истории
задачи: `messages` сохраняют полный диалог, а `context_ledger` фиксирует
выборку. Вытесненные в artifact store элементы несут bounded summary в prompt,
а выбранные записи scratchpad (facts, tool findings) попадают в контекст явно,
поэтому модель видит результаты инструментов и после сокращения контекста.
Выбранная модель сохраняется в `shell/provider.json` и при следующем запуске
передаётся Core как модель маршрута; ModelPicker не подменяет её первым
элементом каталога. Windows named-pipe сервер обслуживает shell и CLI
одновременно, поэтому `eva` работает и при запущенном desktop-приложении.
Ошибка каталога при выборе модели больше не закрывает IPC-соединение: Core
отвечает типизированным `model.select.rejected` с безопасным кодом причины, а
ModelPicker показывает отказ и не повторяет автоматический выбор модели.
Новое состояние трассы выпускается как `routing.trace` schema v2; renderer
сохраняет чтение ранее сохранённых v1 событий.
Staging marker updater теперь всегда записывается как полный
`evohime.component-manifest.v1`, совместимый со встроенным transaction parser.
При staged apply активный `evohime-updater.exe` исключается из preflight и
backup/copy: его замена выполняется отдельной операцией после выхода worker.
Обработчик сообщений renderer использует новый объектный API Electron для
`console-message`, поэтому updater больше не должен печатать предупреждение о
deprecated positional arguments в консольное окно.
При post-update запуске supervisor получает внутренний признак живой
транзакции и откладывает recovery до health-handshake/commit, чтобы не
откатывать установку параллельно с transaction worker.

## Граница текущего checkout и CI

Текущий checkout содержит task-only реализацию единственного web-установщика и
self-contained updater в `main`. `HEAD` `376fa354` совпадает с
`origin/main`; последний коммит, меняющий код, — `cbd5aa6539ff873248a418ceb133e61fcb91417d`.
Module router и Core workflow для этого code baseline завершились успешно;
подробное redacted evidence находится в
[`release-evidence.md`](release-evidence.md).

## История чата в renderer

`conversation-projection` хранит загруженные history pages отдельно от
текущих `subscribed`/`live` событий и сохраняет монотонный cursor подписки.
Новые события не удаляют ранее загруженную историю. `TaskTimeline` принимает
только новый префикс/суффикс общего event-потока, индексирует страницы и
conversation event IDs, и обновляет только затронутую проекцию; подписка
пересоздаётся отдельно при смене чата или восстановлении соединения и хранится
в Core на уровне конкретного IPC-соединения, а не в общем состоянии процесса. Он
отображает историю через перемещаемое окно до 80 строк, сохраняет позицию чтения при prepend
старой страницы и не автопрокручивает пользователя, если он читает старый
участок ленты.

Список чатов оболочки отфильтрован по выбранному `workspacePath`; модельная
история привязана к ID conversation, а проектный контекст — к workspace scope.
Смена проекта сбрасывает открытый чат и его renderer projection, поэтому новый
запрос создаёт или открывает только чат выбранного проекта.

Исторические refs и workflow закрытых планов сохранены в
[`release-evidence.md`](release-evidence.md) с их исходными commit и run ID.

Постоянные каналы поставки разделены по назначению: [`installer`](https://github.com/rkfsociety/EvoHime/releases/tag/installer) — единственный web installer, [`listener`](https://github.com/rkfsociety/EvoHime/releases/tag/listener) — отдельный модульный release listener runtime.
Локально по умолчанию выполняются быстрые проверки; полный acceptance-прогон
Rust, Electron, native package и installer является обязательным в GitHub
Actions и может запускаться локально при необходимости.

Core pipe работает fail-closed: отсутствие authenticated context вне явного
`EVOHIME_DEV_MODE=1` останавливает процесс до открытия базы и pipe. Негативные
запуски реального Core проверяет `crates/evohime-core/tests/production_pipe_startup.rs`;
политику dev-режима и сохранение authentication — тесты `pipe_server`.

## Пользовательская оболочка

Основная навигация находится в одном окне Electron. Слева доступны workspace и
чаты, а также быстрые действия `Новый чат`, `Запланировано` и `Плагины`.
Пользовательские представления:

- `Обзор` — состояние системы и workspace;
- `Ревью планов` — коллективное ревью Markdown-плана;
- `Память и Pulse` — память, heartbeat и диагностика;
- `Улей` — единый дашборд: департаменты, Королева Ева, метрики и бюджет,
  активные агенты, логи и решения human-in-the-loop; мини-чат, индикатор
  «Королева печатает», approvals и статусы задач читаются из typed event log
  `hive-system-chat`, live-подписка принадлежит своему IPC-соединению, а
  маршрут `hive` на IPC-границе назначается только shell-клиенту Electron с
  этим `conversation_id`;
- `Продолжения`, `Анализ`, `Слух`;
- `Запланировано` — список локальных automation schedules.

Отдельного UI-раздела «Составные задачи» нет. В обычном чате Core агенту
доступны фиксированный каталог workflow и инструменты запуска, статуса и отмены;
произвольного редактирования графов нет. `child`-узлы исполняет Core-owned
исполнитель зарегистрированных ролей: read-only инструменты, grants как
подмножество родителя, бюджет как остаток родителя и pinned TeamSession при её
наличии. Отчёт проходит `Validating → WaitingParentAcceptance → Accepted` и
возвращается в fan-in. Панель «Дочерние задачи и контексты» показывает bounded
timeline `child.workflow` со статусом роли и кратким отчётом, без prompt и сырого
вывода инструментов. Run status и cancel доступны только из workspace, в котором
запуск был создан. Глубина child nesting ограничена `MAX_CHILD_DEPTH = 2`:
королева → рабочий (`depth=1`) и подагенты рабочего (`depth=2`). Подагент
уровня 2 не может порождать детей (`child_depth_exceeded`), глубина 3 и больше
запрещена. Дополнительный hop разрешён только queen-issued `DelegationGrant`;
без grant попытка отклоняется как `delegation_not_granted`, grants подагента
обязаны быть подмножеством grants рабочего, а бюджет — долей остатка бюджета
рабочего. `DelegationGrant.max_children` — потолок, а не план: рабочий порождает
подагентов лениво, по одному, ждёт отчёт и останавливается досрочно при
решённой задаче, возвращая неиспользованное время; одновременно в работе не
больше `max_parallel_subagents`. Потолок выбирает королева (явно или
эвристикой `HiveSettings::choose_delegation_ceiling`), а расширить его может
только queen-side `extend_delegation` по запросу рабочего и в пределах
`HiveSettings` (16/4/64 по умолчанию; `0` — без отдельного потолка для
per-worker и parallel, workflow-wide guard обязан быть положительным).
`parent_is_child` проверяется не только как поле запроса: executor
устанавливает runtime-маркер `ChildExecutionContext` с глубиной вокруг
воркера, поэтому claim без маркера отклоняется как `child_context_missing`, а
`depth` нельзя подделать из IPC или legacy-команды. `NestedChildForbidden`
сохраняется для недопустимой вложенности и legacy-пути без grant.
`OperationsPanel` показывает `depth=1`/`depth=2`, отдельный статус подагента и
ленивый fan-out (`подагенты N / M`, ожидания, «в работе», «ожидает решения»,
«расширение grant'а запрошено») в существующем child timeline, без нового
agent loop или панели.
Executor и legacy `SubmitChildRequest`/`SubmitChildReport` используют один
`child_workflow` writer для checkpoint, projection, accepted report и handoff —
одинаковый отчёт даёт одинаковые структуры независимо от пути. TeamSession
читается не более 64 записей по 64 KiB (snapshot сверх лимита не сохраняется), а
projection несёт `report_status` из enum и `report_summary` не длиннее 512
символов.

Инструмент `agent_run` выполняет настоящий вложенный agent loop, а не
заглушку: Core перехватывает вызов до реестра и запускает bounded read-only
субагента с собственным task id. Субагент видит только `filesystem_read/list/
search/stat`, `git status/diff/log/show/blame/changed_files` и `memory_search`;
его отдельная permission-сессия запрещает запись, shell, git write, браузер,
MCP и микрофон, а workflow, `agent_run`, app и kernel-инструменты не попадают в
каталог. `max_steps` ограничен 1–32 (по умолчанию 8), `timeout_ms` — не больше
180 секунд, `model_route` закрепляет модель, а provider/profile наследуются от
родителя. Глубина вложенности — один уровень: субагент не может запустить ещё
одного (`agent_run cannot start a nested subagent`). События и tool chatter
субагента дренируются и не попадают ни в conversation log, ни в дашборд Улья;
наружу возвращается только итоговый ответ и bounded метаданные.

`OperationsPanel` получил отдельную секцию «Состояние улья», которая раз в 2
секунды запрашивает read-only команду `core.getHiveState` и отображает
`HiveStateSnapshot` (`crates/evohime-core/src/hive_state.rs`): королеву, число
агентов и активных задач, сетку департаментов `active_count / max_capacity` с
полосками прогресса, список активных агентов с ролью, текущим действием,
глубиной и прогрессом подагентов, а также поток задач Processing / Analyzing /
Plan. Snapshot собирается из bounded-окна `child.workflow` в журнале, активных
записей `persistent_agent_registry` и `HiveSettings`; в окно входят только
поддеревья, чей корень привязан к `hive-system-chat` (шаблоны обычных чатов в
дашборд не попадают). Это производная проекция
без prompt, отчётов и вывода инструментов. Пустой журнал или реестр дают пустые
массивы и нулевые метрики, а не ошибку. Секция бюджета показывает выбранный
профиль и модель, committed dispatch-счётчики за скользящие минуту/час/сутки
против RPM/RPH/RPD из route configuration (нулевой cap — «лимит не настроен»),
настроенную дневную квоту токенов с окном «сутки UTC» или «скользящие 24 часа»
и дневную оценку стоимости DeepSeek по фактическому usage из
`context_ledger_usage`, включая provider-reported cache-hit tokens и
peak/off-peak тариф по времени отправки запроса. Квота считается по usage,
записанному со scope маршрута (`hive` или `hive_fallback`), поэтому основной и
резервный маршруты и общие чаты не делят один счётчик. Недоступная история
отдаётся как `budget.history_available = false`, неизвестная модель или
отсутствующий тариф — как недоступная стоимость, а не как нулевой расход.

`EvaPulse` — Core-owned фоновый цикл улья (`crates/evohime-core/src/eva_pulse.rs`).
Он стартует вместе с Core и просыпается по `HiveSettings.pulse_interval_secs`
(по умолчанию 1800 с). Перед каждым циклом демон читает committed dispatch-окна,
дневную оценку DeepSeek и bounded-окно `child.workflow`; при исчерпанном бюджете
(`daily_budget_usd = 0` или оценка расхода достигла потолка), окне LiteRouter или
исчерпанной дневной квоте токенов маршрута Улья
он засыпает до точного `wake_at_ms`, не создавая задач. Новые упавшие воркеры
приводят к одному bounded `EvaPulseSignal`, по которому `HiveSignalReceiver`
создаёт одну ограниченную recovery-задачу Улья, но только
на primary-маршруте LiteRouter: DeepSeek и другие маршруты для фоновой работы не
используются, а 429 не повторяется и не уходит в платный fallback. При заданном
`eva_workspace_path` recovery-задача получает инструкции автономного ремонта и
capability `eva_self_development` с каталогом `eva_workspace_*`; Core проверяет
путь как отдельный Git checkout и сохраняет штатные permission approvals и
тестовый gate перед commit/push. Без пути остаётся read-only исследование.
Настройка пути доступна в дашборде Улья; пустое или невалидное значение
очищает конфигурацию и отключает инструменты с ошибкой в UI. Gateway
записывает полученный cooldown в process-local `rate_limit_signal`, и демон
читает его перед следующим циклом. Здоровый цикл
пишет `EvaPulse: all good` в `core.jsonl`. Ручное управление остаётся
`core.togglePulse` (proto command 287): `false` переводит демон в `disabled`,
`true` будит его и запускает цикл сразу; панель Улья этот вызов не использует.
Статус Pulse отдаётся отдельной проекцией `eva_pulse.status` (proto command
292), а `hive_state.snapshot` schema v7 несёт только Hive-owned состояние и
читает старые schema 1–6 через совместимый fallback. Снимок также содержит
настроенные лимиты запросов и дневную квоту токенов (с окном сброса) для
основного и резервного маршрутов Улья, фактический `daily_budget_usd`, число
начатых задач за UTC-сутки и среднюю длительность завершённых задач из Core
event journal; при ошибке чтения UI показывает нули. Мок-метрики, лимит `$0.50`
и демо-логи удалены.

Путь `HiveSettings.eva_workspace_path` в дашборде «Улей» задаёт отдельный Git
checkout для self-development. Core валидирует путь через `EvaWorkspace::open`;
пустой или невалидный ввод очищает путь, выключает инструменты и возвращает
ошибку в UI. При настроенном workspace EvaPulse направляет recovery-задачу
исследовать, исправить код, запустить тесты и сделать commit. Core добавляет
capability `eva_self_development` и инструменты `eva_workspace_*` только для
такой задачи и только после стандартного permission preflight. Изменения
остаются внутри checkout, а commit закрыт тестовым gate, push — тестовым и
чистым-worktree gates Self-Development Tools. Фоновый маршрут остаётся на
primary LiteRouter; DeepSeek автоматически не вызывается и платный
fallback не включается. Без пути остаётся обычная read-only задача исследования.
Снимок Улья schema v7 отдаёт `daily_budget_usd`, число начатых задач за UTC-сутки
и среднюю длительность завершённых задач из Core event journal; при ошибке
чтения UI показывает ноль. Мок-метрики, лимит `$0.50` и демо-логи удалены.

Внутренние runtime-контракты, execution backends, безопасность, диагностика и
организация агентов не представлены отдельными вкладками: это Core/agent/model
поверхность, которой управляет ядро. В верхней панели доступны только контекстные
`Рабочая панель`, `Открыть браузер`, `Трейс` и индикатор состояния. Окно
остаётся видимым при запуске, а blocking progress UI принадлежит отдельному
`updater\EvoHimeUpdater.exe`: renderer shell больше не отображает legacy
`UpdateGate`. Main-процесс сохраняет только невидимый launch safety gate до
завершения транзакции и не отправляет Еву сразу в трей.

## Провайдеры и модели

Поддерживаются LiteRouter, OpenAI Responses API, Ollama и OpenAI-compatible
транспорт с явными профилями OpenAI, OpenRouter, Groq, Gemini, Mistral,
Cloudflare Workers AI, NVIDIA NIM, Cerebras, Hugging Face, DeepSeek, xAI,
Together AI, Fireworks AI, Moonshot, Z.AI, SambaNova, Nebius, Rout, AnyModel и
generic custom;
`mock` используется только в тестах. В настройках все вендоры выбираются одним
списком, а профиль OpenAI-compatible применяется внутри выбранного пункта.
Ollama не требует ключа и ограничен loopback endpoint.
Профили и зашифрованные ключи хранятся в
`%LOCALAPPDATA%\EvoHime\shell\provider.json`; ключ доступен Core только через
окружение supervisor. Каждый OpenAI-compatible вендор хранится отдельным
профилем с независимым ключом; прежняя общая запись мигрирует при чтении.
Для каждого провайдера там же сохраняются настройки паузы
между запросами: по умолчанию она выключена для всех провайдеров, и лимиты
запросов — общие для провайдера (`*`) и отдельные для моделей RPM/RPH/RPD плюс
дневная квота токенов с окном «сутки UTC» или «скользящие 24 часа». Переключение
провайдера не сбрасывает эти значения. Расход токенов учитывается отдельно для
каждого маршрута, поэтому исчерпанная квота блокирует новые запросы этого
маршрута, не смешиваясь с другими аккаунтами и Ульем.

Есть два разных маршрута выбора модели:

1. выбор API-модели передаётся в Core для следующего запроса и не требует
   перезапуска Core;
2. выбор активного API-профиля автоматически сохраняется и перезапускает Core
   после обновления окружения; отдельное подтверждение выбора не требуется.
   Изменение ключа или endpoint сохраняется отдельным действием формы и также
   перезапускает Core;
3. выбор модели Codex CLI сохраняется в `shell\codex.json`, после чего shell
   перезапускает Core, чтобы новый запуск Codex получил выбранную модель.

Каталог моделей для API и Codex получается динамически. Токены ChatGPT и
cookies не читаются и не сохраняются EvoHime. Панель чата показывает выбранный
режим, профиль и модель; автоматического переключения на другой backend нет.

Для Ollama каталог разделён на установленные модели и рекомендации для
скачивания. Рекомендации Core строятся по snapshot устройства: CPU, ОЗУ,
свободному месту и VRAM/GPU. Скачивание выполняется через Core и native Ollama
`/api/pull`; renderer не обращается к Ollama напрямую.
Если Ollama отсутствует, настройки предлагают установить её через main-owned
сервис: официальный `OllamaSetup.exe` потоково скачивается по HTTPS, запускается
отдельным процессом и после завершения проверяется loopback API. Если Chromium
возвращает `net::ERR_BLOCKED_BY_CLIENT`, main повторяет только загрузку через
Node transport; allowlist URL, bounded-размер и проверка Windows-заголовка
сохраняются. Промежуточный installer удаляется.

## Задачи и расписания

`ProjectSidebar` является точкой выбора workspace и чата. `TaskTimeline`
отображает поток Core-событий, approval и recovery; renderer не выполняет
инструменты и не владеет бизнес-логикой.

`ScheduledPanel` получает schedules через `automation.listSchedules` и меняет
активность через `automation.setScheduleEnabled`. Список ограничен 64
элементами, отображает owner (`user` или `workspace`), UTC-время, revision и
последний слот. Приостановленные записи явно помечаются как `paused`.

Функциональность background execution не представлена отдельной вкладкой и
остаётся Core-owned контрактом: authenticated command 262/event 107 передаёт
redacted проекцию detached runs, schedules, queues, waits и immutable attempts.
Core
принимает `background-execution/v1`, хранит snapshots и wakeups в schema v103,
выполняет restart reconciliation и не считает отсутствующий effect adapter
успешным: результатом остаётся `runtime_adapter_unavailable`.

`OperationsPanel` объединяет пользовательский self-repair, память и pending
items, child-задачи, Pulse, инструменты, локальный индекс workspace, refinement
и ambient proposals. Пока Core не находится в полностью подключённом состоянии,
карточки памяти, child jobs, Pulse и инструментов показывают состояние как не
подтверждённое, а не подменяют отсутствующую проекцию нулевыми счётчиками или
успешным статусом. Ошибки недоступных optional adapters остаются typed
`unavailable` и не превращаются в успешный эффект.

## Пользовательский self-repair

Self-repair запускается только действием пользователя из `OperationsPanel` и
работает в изолированном checkout. До запуска пользователь обязан выбрать
provider и model; выбранная пара сохраняется в статусе repair-run и переносится
через diagnose, commit, push и restart. Каждый из этих этапов требует отдельного
подтверждения. Автоматического ремонта, автоматического push и фонового
перезапуска рабочей сессии нет.

Repair не редактирует выбранный пользователем workspace и не меняет
установленный клиент. Защищены `AGENTS.md`, `.codex`, CI workflows, installer,
updater, supervisor, receipts, security-файлы и `.env*`. Push допускается только
в настроенную ветку после подтверждения пользователя и зелёных проверок.

## Данные и границы безопасности

- данные и backup: `%LOCALAPPDATA%\EvoHime` или `EVOHIME_DATA_DIR`;
- Core log: `%LOCALAPPDATA%\EvoHime\logs\core.jsonl`;
- supervisor log: `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`;
- состояние shell: `%LOCALAPPDATA%\EvoHime\shell\`;
- update transaction: `%LOCALAPPDATA%\EvoHime\update-state\`; при ошибке worker сохраняются точная причина и отдельное поле `error`, а Electron отображает явную фазу `failed`;
- экспорт событий выполняется JSONL через `LocalDatabase::export_events_jsonl`.

Persistent Agent Organization Registry v1 хранится в Core-owned SQLite schema
92. Он сохраняет durable agent identity, reporting history, exact Goal/role
profile references и assignments к уже существующим task/run/team-session/
handoff; новый runtime или scheduler не создаётся. Startup recovery помечает
потерянные assignment sources как `unknown_after_restart`. Cost projection
сейчас явно `unavailable`, потому что agent-keyed authoritative ledger ещё не
существует.

Миграции SQLite транзакционны и создают backup до изменения схемы. Named pipe
аутентифицируется launch context и HMAC proof; роли `shell`, `listener` и `cli`
разделены. Approval, sandbox, таймауты, отмена, bounded frames и redacted
diagnostics обязательны для опасных операций. Подробная модель границ — в
[`../SECURITY.md`](../SECURITY.md) и [`architecture.md`](architecture.md).

## Модульность и узкие проверки

Основные Core и IPC-домены разделены на обычные Rust-модули с отдельными
тестовыми файлами; крупнейшие исходные файлы checkout находятся в пределах
лимита 2000 строк. Doctor дополнительно проверяет размер исходников, число
`include!` и наличие индексов горячих SQLite-запросов. Context Budget Manager
использует `Arc` для крупных payload-ов, выполняет initial pruning in-place и
считает fallback estimator, а workflow
runtime переиспользует canonical hash, ограничивает глубину графа 64 узлами и
публикует bounded dispatch metrics.

## Execution trace contract (план 184, этапы 1–2)

Core содержит versioned redacted contract в
`crates/evohime-core/src/execution_trace_evaluation.rs`: trace schema v1,
evaluation contract v1, максимум 4096 events, 128 assertions, 256 элементов
в assertion list и 128 символов на identifier. Raw prompts, tool arguments,
provider output и reasoning не являются полями trace. Canonical hash имеет
вид `sha256:<64 lowercase hex>`; `ExecutionTrace::legacy_tool_trace_digest()`
возвращает те же digest bytes в прежнем 64-hex формате поля
`AttemptResult::tool_trace_digest`, без второго trace store.

Validation требует непрерывную event sequence, уникальные event ids,
предшествующий ToolCall для RetryStarted, уникальные effect ids,
согласованные outcome для нормализованных событий и matching approval до
EffectCommitted. Пустой trace классифицируется как incomplete, превышение
лимита — отдельно. Unknown event kinds и дополнительные payload-поля
отвергаются serde-контрактом.

Exact-SHA baseline `f034f3d82a07176fa17d58688b5b9e0f76ebd5b8` прошёл Core
tests/lint/release build, Linux CLI/IPC, Rust documentation/doc-tests, UI
bundle tests/build, module-router и compatible manifest; run IDs и scope
проверок записаны в `release-evidence.md`. В текущем изменении добавлены
regression tests для пустого trace и повторного effect id; Core marker
`0.0.000412` ожидает post-push CI. Локальные Rust tests/build/lint и точный
`git diff --check` в этой среде не запускались: локального checkout нет,
а clone недоступен из-за DNS. Stage 184.1 остаётся активным до exact-SHA CI и
whitespace gate.

Runtime capture, partition по run/attempt, persistence и lifecycle ещё не
подключены. Audit текущего checkout показывает, что обычный ToolAgent loop
публикует Core tool events, но не записывает typed `ExecutionEventV1` для
каждого model tool call. Receipt-tracked terminal path записывает typed
events в Standalone scope без attempt id. Workflow attempt projection хранит
attempt ids и поддерживает link на ledger, но production call site для
`append_ledger_event_with_node_transition` не найден — сейчас он используется
в storage tests. Поэтому следующий этап 184.2 должен сначала связать
существующую Core tool/effect boundary с authoritative typed ledger и
redaction-before-hash. Нельзя выводить approval/effect/attempt ids из
неструктурированных payloads, синтезировать их или создавать второй trace
store. Неполный и неоднозначный исход должен оставаться partial/unknown и
не давать passing verdict.

## Declared analysis kernel tool (план 192, этап 1)

Kernel уже существовал как Core-owned runtime, но не был объявлен ни одним
инструментом, поэтому агент не мог его вызвать. Этап 192.1 добавляет
декларацию и host-контракт без второй реализации dispatch:

- `crates/tool-runtime/src/tools/analysis.rs` объявляет `analysis_run`
  (canonical schema, пустые permissions, `ReadOnly`, `approval = Never`,
  `workspace_scope = "task"`, bounded inline размер), а
  `builtin_input_schema` возвращает эту схему вместо заглушки;
- `crates/tool-runtime/src/kernel_host.rs` объявляет трейт `KernelHost` и
  bounded типы запроса/результата с stable typed кодами ошибок;
- `ToolRegistry` получает `Option<Arc<dyn KernelHost>>`, builder
  `with_kernel_host` и accessor; без host `analysis_run` зарегистрирован, но
  каждый вызов возвращает `analysis_kernel_unavailable` — in-process fallback
  отсутствует;
- `catalog_preflight_input("analysis_run")` проходит объявленную схему, поэтому
  инструмент остаётся в авторизованном snapshot;
- `classify_call_risk("analysis_run")` = `None`.

Этапы 192.2–192.3 также реализованы в исходниках: `CoreKernelHost` подключён к `ToolRegistry::with_kernel_host` в `main.rs` поверх общей с `IpcBridge` карты `analysis_kernels`, добавлена IPC-команда `ListAnalysisKernels` (tag `291`) и панель `AnalysisKernelPanel` в `OperationsPanel`; Rust CI подтверждается после push. (Исторически этап 192.1 не создавал host-реализацию и вкладку «Анализ»:)
они остаются этапами 192.2–192.4, и до их реализации вызов из агента
завершается typed `unavailable`. Значения объектов, inline result и абсолютные
пути в декларацию, каталог и логи не попадают.

## Provider failure classification

Исторический сбой `4ad79c10-6bdf-43ab-9934-d1e0de261c3f` показал, что общего
`task.failed` и кода классификации недостаточно для диагностики: provider body
и безопасные поля JSON error терялись до записи trace.

Исправлено в исходниках:

- `ProviderError::http_status` и `ProviderError::failure_code` в
  `evohime-model-gateway` извлекают HTTP status из формата adapter-сообщения и
  дают bounded код без provider text, URL и секретов;
- `classify_provider_error` — единый контракт для gateway fallback loop и Core
  resilience policy: `401`/`403`/`402`/`404` и прочие `4xx` — `InvalidRequest`
  без retry и circuit, `429` — cooldown, timeout/connect и `5xx` — повторяемые;
- Core публикует точный код отказа (`provider_auth_rejected`,
  `provider_quota_exhausted`, `provider_model_not_found`,
  `provider_rate_limited`, `provider_request_rejected`, `provider_timeout`,
  `provider_unreachable`, `provider_unavailable`, `provider_configuration`) в
  `task.failed` projection и в trace (`http_status` в `provider.error_*`);
- renderer показывает по этому коду понятную причину и подсказку в баннере
  восстановления и в ленте диалога (единый `failureReasonText`), а неизвестный
  код по-прежнему выводится как есть;
- adapter теперь сохраняет для `provider.error_*` bounded
  `provider_diagnostics`: JSON error type/code/param/message, безопасный request
  ID и метаданные размера тела. Сырые response bodies, authorization headers и
  URL не пишутся; текстовые поля проходят Core redaction перед trace. Событие
  дополнительно связывает отказ с номером попытки, выбранной моделью и
  назначением вызова. Для известных причин (модель не найдена, лимит контекста,
  неподдерживаемый параметр) Core также публикует отдельный actionable code.

## Delivery requirements и tool loadout

Задача `d2aeef9c-cb8f-4d08-b253-f0bb664b012c` в установленной версии показала
блокер: intent router классифицировал смешанный запрос как `inspect` (слова
«проверь/найди/покажи» перевесили «добавь»), adaptive catalog выдал только
read-only инструменты, а delivery gate продолжал требовать
`filesystem_write`, пока не исчерпались 32 итерации; в UI оставался общий
`task_failed`.

Исправлено в исходниках:

- explicit mutation keyword побеждает read-only этап независимо от числа
  совпадений; отрицание распознаёт общий предикат
  `evohime_context_budget::loadout::contains_unnegated_keyword`, который
  используют и router, и `DeliveryRequirements`;
- adaptive catalog добавляет к лексическому top-N только уже авторизованные
  инструменты, без которых требование поставки невыполнимо; permission
  preflight является источником истины по mutation, поэтому в режиме полного
  доступа модель получает write-инструменты независимо от формулировки
  prompt, а в режиме подтверждения — при явном mutation-слове; в полном
  доступе каталог сохраняет и рабочий набор агента (filesystem, shell, git
  status/diff/commit), поэтому лексический top-N не сужает выбор из композера;
- невыполнимые требования не превращаются в отказ: Core снимает их с
  bounded trace `task.delivery_requirements_degraded`, осмотр без изменения и
  без явной команды тестов не требует сборки, а три одинаковых gate-запроса
  подряд означают остановку требований с возвратом ответа модели;
- redacted `task.failed` projection различает `tool_iteration_limit` и
  общий `task_failed`, renderer показывает причину по коду.

Exact-SHA CI по этому изменению ожидает результата.

### Лимит шагов и недоступный patch

Задача `ac4358e2-09a7-461b-bbbc-9a5487018497` («добавить в композер вложение файлов через ctrl+v») в установленной версии с моделью `glm-5.3-flash-cheap:free` за 32 итерации сделала 31 успешный read-only вызов и ни одной правки, затем завершилась `task.failed` с `error_code=tool_iteration_limit` и без итогового текста. В model trace этой задачи в списке tools отсутствовал `filesystem_patch`: runtime требовал `expected_hash`, а catalog probe и declared schema его не передавали, поэтому probe не проходил preflight и инструмент молча выпадал из набора модели.

Исправлено в исходниках:

- `filesystem_patch` объявляет `expected_hash` обязательным в схеме и runtime, а его catalog probe передаёт placeholder: без этого preflight отклонял probe, инструмент выпадал из набора модели, и правка существующих файлов была недоступна; Core по-прежнему подставляет хеш последнего чтения, если модель опустила поле;
- сверх лимита tool-итераций цикл делает один дополнительный tool-free вызов: инструменты убираются из запроса, напечатанный tool call не исполняется, задача завершается `task.completed` с текстом модели, а не `tool_iteration_limit`;
- для задачи с обязательным изменением Core считает успешные шаги без правки и на 6/12/18-м шаге добавляет bounded guidance перейти к `filesystem_patch` или `filesystem_write` (`task.delivery_mutation_nudge`), максимум три раза.

### Схемы инструментов и ложные повторы

Работа через Еву выявила дефекты, которые отклоняли корректные шаги агента
без понятной причины.

- `builtin_input_schema` для 27 зарегистрированных инструментов не имел явной
  ветки и возвращал fallback с единственным полем `input`. Модель видела эту
  схему, отправляла `{"input":{}}` в `git_status` и получала
  `this tool does not accept input`. Теперь явные схемы объявлены для
  `git_status`/`git_changed_files` (пустой объект), каждого git-инструмента,
  `filesystem_stat`/`filesystem_mkdir`, `logs_*`, `cargo_*`, `archive_*` и legacy
  `browser_session_*`; тест
  `no_registered_builtin_tool_falls_back_to_the_generic_input_schema` не даёт
  вернуть заглушку.
- `git_diff` делил схему с `git_log`/`git_show`/`git_blame`/`git_pull`/`git_push`
  и объявлял `reference`, `remote`, `branch`, `force`, `max_count`, которых его
  исполнитель не читает. Схемы разделены, а `DiffInput` отвергает неизвестные
  поля через `serde(deny_unknown_fields)`: вызов с `reference` больше не
  превращается в diff всего дерева.
- `RecentToolCalls::forget_reads` существовал, но не вызывался, поэтому повтор
  `filesystem_read` или `git_status` после правки удалялся как дубликат с
  guidance «Его повтор удалён Core». Теперь успешная правка или коммит
  сбрасывают окно read-only сигнатур, включая `git_status` и `git_diff`.
- Recovery-подсказка для инструмента без аргументов требует передать `{}`
  вместо вывода бесполезной схемы `{"type":"object"}`.

### Канонический лексикон tool id

Имена инструментов больше не подменяются перед отправкой провайдеру: id равен
`filesystem_read`, `git_status`, `git_worktree_create` и попадает в prompt,
provider-запрос, журнал и receipt без преобразований.

- `provider_function_name` удалён из `crates/model-gateway/src/tools.rs`;
  `wire_tools` и `response_tool` только снимают Core-owned `manifest_hash` и
  schema-ключи, которые не принимает upstream.
- `adaptive_tool_catalog::resolve_tool_call_name` заменил
  `canonical_tool_call_name` и принимает историческую точечную форму только
  внутри активного loadout: алиас не расширяет authority.
- `ToolRegistry::resolve_tool_id` и `manifest_for` принимают `git.diff` для
  durable записей и старых клиентов; `preflight` и все пути `execute*`
  разрешают имя через него, поэтому историческая строка не становится
  доступом к незарегистрированному инструменту.
- `canonical_tool_id` нормализует списки в сохранённых capability, skill и
  prompt-strategy пакетах; `legacy_parser` канонизирует имя разобранного
  текстового вызова, поэтому в новое состояние не попадает точечная форма.
- Эвристики, выводившие смысл из позиции точки, переведены на
  `tool_namespace` (`context_budget`, `registry`, `trace_diagnostics`,
  `conversation_event_log`, `legacy_parser`, continuation gate). Имена
  событий IPC (`git.status`, `memory.search`, `workflow.started`) остались в
  прежней форме: это отдельный лексикон, а не tool id.
- `permissions.json`, `workspaces.json` и `chats.json` миграции не требуют:
  первые адресуют `Permission`-enum, остальные tool id не хранят.
  `EXPECTED_TOOL_COUNT = 74` не изменился — переименованы имена, не состав.

## Подтверждённые проверки checkout

Этап 192.1 опубликован в `origin/main` коммитом
`51c6860484c9b25196f53add11e24477565e11d9`. Exact-SHA проверки завершились
успешно: Core workflow `43327009208`, Rust documentation `43326924666`,
module router `43326924778` и compatible release manifest `43329493376`.
Эти результаты подтверждают этап 192.1, но не заменяют runtime integration и
recovery этапа 192.2. Исторические baseline и результаты сохранены в
[`release-evidence.md`](release-evidence.md).

Bounded-таймаут модульных Rust-тестов измеряет только исполнение: `core.yml` и
`windows.yml` сначала собирают тестовые бинарники отдельным шагом
`cargo test --locked --no-run`. `core.yml` затем запускает тот же набор `-p`
одним `cargo test --locked --lib --bins --tests --no-fail-fast` с явным
таймаутом шага: одинаковый feature-набор не заставляет Cargo пересобирать
пакеты, target selection исключает doc-tests (их проверяет `rustdoc.yml`), а
примеры собираются отдельным шагом без запуска. `windows.yml` сохраняет
по-пакетный `Invoke-BoundedCargoTest` с явным лимитом. Раньше холодная сборка
`evohime-core` (~16 минут) входила в 20-минутный лимит того же процесса,
поэтому пакет обрывался через `taskkill` посреди doc-tests, а убитые примеры
попадали в отчёт как `FAILED`. Контракт закреплён проверкой в
`scripts/module-router.tests.ps1`.

Сборка модулей использует `Swatinem/rust-cache` с раздельными ключами для
тестового и release-профиля, а release-бинарник Core собирается отдельной
джобой только для `push`/`workflow_dispatch`, поэтому pull request не платит
за release-профиль. Результат нового CI-расписания подтверждается только
exact-SHA прогоном GitHub Actions.

| Проверка | Результат |
| --- | --- |
| Workspace RAG decode regression | PASS; valid/invalid/empty UTF-8 и UTF-16LE/BE |
| `cargo test --locked -p evohime-tool-runtime` (локально, после этапа 192.1) | PASS: 172/6/8/3 теста, 2 ignored (worker/network smoke) |
| `cargo test --locked -p evohime-core --lib` с `RUST_MIN_STACK=33554432` | PASS: 1100 тестов, 0 failed; на дефолтном стеке libtest бинарник падает `STATUS_STACK_OVERFLOW`, поэтому локально нужен тот же стек, что в CI |
| `cargo fmt --all -- --check` и `git diff --check` | PASS |
| `cargo clippy --locked -p evohime-core --all-targets -- -D warnings` | PASS (после этапа 192.1) |
| `cargo clippy --locked -p evohime-tool-runtime --all-targets -- -D warnings` и `cargo doc -p evohime-tool-runtime --lib` с `-D missing_docs` | PASS |
| `scripts/documentation.tests.ps1` | PASS: 117 tracked text files |
| Узкие прогоны `evohime-core`: revision-precondition unit test и doc-tests `prompt_cache_planner`/`research` | PASS |
| `scripts/module-router.tests.ps1` и `scripts/ci-selection.tests.ps1` | PASS |
| Electron `npx vitest run tests/project-sidebar.test.tsx` | PASS: 3 теста |
| Electron полный `npm test` | PASS: 140 файлов, 702 теста; 1 файл и 4 теста skipped |
| Electron `npm run typecheck`, `npm run check:protocol` | PASS |
| Electron shell/updater production builds и `npm run check:bundle` | PASS |
| `scripts/documentation.tests.ps1` и `git diff --check` | PASS |
| Сравнительный benchmark latency/cache | Не выполнялся; ускорение не заявляется |
| GitHub exact-SHA CI / публикация модулей | Ожидает push; локальный workflow этого не подтверждает |

## Исторические сведения о закрытых планах

Следующие краткие записи сохранены для навигации по ранее закрытым планам. Они
не являются текущей очередью, свежим тестовым отчётом или заменой
`release-evidence.md`.

## Plan 119 — Execution Environment Profiles v1 (закрыт 2026-09-08)

Core/storage реализуют bounded profile envelope, canonical hash, typed owner
references, pinned/follow-compatible preflight, fail-closed diagnostics,
activation/current snapshots, run pinning, optimistic revision и durable
idempotency outcomes в schema v93. Поддержанные metadata adapters — routing,
backend, external-agent preset, execution policy и approval policy; остальные
owner kinds остаются typed `unavailable_owner` до появления безопасного
versioned lookup и не могут незаметно активироваться.

IPC command 260/event 105 проходит authenticated Core path, replay/resync и
Electron metadata-only projection. В исходном release evidence зафиксированы
проверки: Core profile 6/6,
local-storage profile 3/3, protocol/typecheck/focused UI 1/1, полный Electron
suite 129 files / 572 tests passed / 1 skipped file / 4 skipped tests,
production build и bundle check, native package smoke. Полный Rust suite для
затронутых crates также прошёл до финального documentation-only переноса.

## Статус очереди на момент синхронизации

Незавершённый каталог содержит план `180` и планы `184–188`; планы `01–179`, `181`, `183` и `189` закрыты. Планы `102`,
`118–130` и `144` реализованы и закрыты; их подтверждённые контракты находятся
в `architecture.md` и `docs/architecture/*.md`, а evidence — в `release-evidence.md`. Точный порядок
выбирается по blocking dependencies в [`plans/README.md`](plans/README.md), а
не по старому линейному списку.

## Plan 176 — Guided Capability Recipes v1 (закрыт 2026-09-25)

Core предоставляет фиксированный каталог восьми typed recipes поверх
существующих workflow templates. Запуск требует точного registry-validated
binding; неподдержанные категории остаются `Unsupported`. Preflight проверяет
bounded inputs, workspace, workflow graph, grants, budgets, approvals и
revisions. Запуск создаёт обычный workflow run и immutable recipe attribution
одной SQLite-транзакцией; recovery, status и cancellation принадлежат
существующему workflow runtime. IPC commands 280–284 дают Electron bounded
catalog/preflight/run/fork projections. Fork завершённого run создаёт draft
из точного template с placeholders и без inputs, outputs, grants, secrets или
allowlists. Канонический контракт находится в
[`architecture/capabilities.md`](architecture/capabilities.md#guided-capability-recipes-v1).

## Plan 132 — Durable Background Execution Plane (закрыт 2026-09-09)

Реализованы Core contract `background-execution/v1`, additive schema v103,
bounded queues/waits/wakeups/attempt history, restart reconciliation,
generation-fenced wake transitions, authenticated IPC 262/107, generated
Electron bindings, deterministic OneShot/Interval/Cron fire polling и
Core-owned redacted projection без отдельной вкладки renderer. Существующие automation,
workflow, agent, goal, human-work и remote-task owners не дублируются.
Локальные тесты, сборки, линтеры и smoke/E2E в исходном task snapshot не
запускались; эта запись не является текущим CI evidence.

## Plan 133 — Built-in Deterministic Developer Utilities (закрыт 2026-09-09)

Core ToolRegistry получил 9 отдельных stateless utility IDs с typed bounded
manifest schemas: Base64, SHA-256/SHA-512, JSON format/minify, UUID v4,
secure token и text case conversion. Реализация находится в
`crates/tool-runtime/src/developer_utilities.rs`; она не читает workspace, не
использует shell/network/model и не создаёт отдельное storage. Common registry
dispatch сохраняет permission, receipt, cancellation, timeout, event и
adaptive catalog boundaries; deterministic and oversized/invalid-input
contracts покрыты исходными unit tests.

Поскольку checkout зафиксирован без push, live CI для commit недоступен.
Локальные tests/builds/linters/smoke/E2E не запускались по ограничению Романа;
выполнены только разрешённые статический анализ и `git diff --check`.

## Plan 134 — Host Resource Telemetry & Pressure Guard (закрыт 2026-09-09)

Core получил `host_resource_telemetry` contract и bounded in-memory ring
(`MAX_HISTORY=120`) для validated CPU, available-memory и storage-free
metrics. `MetricStatus` различает unavailable/unsupported/stale/invalid
состояния, а `PressurePolicy` детерминированно вычисляет Unknown/Normal/
Elevated/High/Critical с conservative fail-closed поведением. Service wired в
`TaskCoordinator::record_host_resource_snapshot`; отдельная SQLite authority,
network telemetry, shell polling и renderer-owned pressure не добавлены.

Локальные tests/builds/linters/smoke/E2E в исходном task snapshot не
запускались; эта запись не является текущим CI evidence.

## Plan 131 — Unified Context Namespace (закрыт 2026-09-09)

Core реализует bounded metadata-only namespace поверх ссылок на существующие
owners. Schema v102 хранит node/projection/view/trace/idempotency metadata;
stale, corrupt, unauthorized и budget-exhausted paths fail closed. Retrieval
проверяет immutable view, sensitivity, roots, depth, quotas, explicit refs и
stable ordering, а trace сохраняет только typed selection metadata.

Authenticated IPC — command 261/event 106. Electron Context Namespace Explorer
не представлен отдельной вкладкой; Core сохраняет read-only redacted projection.
`ContextDetailResolver` остаётся versioned
adapter boundary: в текущем checkout detail явно имеет
`projection_generation_failed/detail_resolver_unavailable`; успешный raw
detail не создаётся. Установленный клиент не запускался и не изменялся.
Полный каталог, блокирующие и опциональные зависимости находятся в
[`plans/README.md`](plans/README.md), исполняемый порядок — в
[`development-plan.md`](development-plan.md).

## Plan 127 — Remote Client Control Plane (MVP закрыт 2026-09-09)

Добавлен `evohime-remote` с versioned bounded frame/device/availability
контрактом, content hash и sequence replay guard. Core boundary
`remote_client_control_plane` без deployable relay остаётся `Unavailable` или
`offline`: он не открывает socket, не читает SQLite и не получает authority.
Android APK, relay service и server installer отсутствуют в checkout и имеют
явный статус `unavailable`; этот deployment gate не скрывается repository MVP.

## Plan 128 — Local Inference Scheduler (MVP закрыт 2026-09-09)

Добавлен Core-owned typed scheduler contract с revision/hash/status/priority и
fail-closed admission. Поскольку versioned inference-stream adapter отсутствует,
runtime execution, worker queue и measured scheduling остаются
`unavailable`; metadata-only contract не объявляет локальный inference рабочим.

## Plan 129 — Confidence-Gated Model Cascade (MVP закрыт 2026-09-09)

Добавлен bounded Core policy contract с threshold/revision/hash и fail-closed
decision. Existing Model Gateway остаётся routing owner; без versioned
confidence producer или eligible route cascade имеет `Unavailable`/`NeedsReview`.

## Plan 130 — Task Ownership & Lease Fencing (MVP закрыт 2026-09-09)

Добавлен общий Core fencing contract поверх существующих lease owners. Проверки
owner/generation/deadline возвращают fail-closed decision; новая authority и
новая durable таблица не добавлялись.

## Как поддерживать этот документ

Обновляйте дату и этот файл только по фактам из кода, тестов и release evidence.
Контракт завершённого плана переносится сюда и в доменный файл `docs/architecture/`, а сам
временный plan-комплект удаляется. Историю и гипотезы не добавляйте в раздел
текущего состояния.

## Plan 120 — Grounded Research Workspace (закрыт 2026-09-09)

В checkout присутствуют typed Core-контракты source revision, evidence locator,
session, citation, artifact и delta; additive migration 95 устанавливает
metadata-only storage. Legacy fetch сохранён как adapter, bounded session runner
подключён к Core actor, network policy, cancellation и startup recovery. IPC
использует authenticated registry transport, а `ResearchWorkspacePanel`
отображает только ограниченную проекцию. Артефакты проверяются по revision/
evidence lineage и продвигаются через существующий Artifact Handoff registry.

## Plan 121 — Local Model Performance Calibration (закрыт 2026-09-09)

Добавлены Core contracts exact calibration identity, bounded samples и
deterministic aggregation, SQLite schema 96 для session/profile metadata,
verified-runtime admission и Electron projection. Текущий runtime boundary не
предоставляет inference-stream adapter, поэтому подтверждённое состояние —
typed `unavailable_adapter`; measured profile и routing signal не создаются
без такого adapter.

## Plan 178 — Core-Owned Local Model Adaptation (implemented and released)

В working tree реализован versioned GGUF adaptation pipeline для Windows CPU:
explicit install pinned llama.cpp `b10981`, metadata-only durable jobs, fresh
resource admission и SQLite disk reservations, FIFO retry из durable-background
poll loop, Supervisor Job Object quantizer/runtime, output validation,
streamed local inference, real benchmark, explicit approval-gated promotion и
restart/publication recovery. Источник — только уже managed GGUF F16/F32;
targets — `Q4_K_M`, `Q5_K_M`, `Q8_0`; VRAM/accelerator fit остаётся unknown.

Реализация и requirement-by-requirement source audit завершены; `git diff --check`
проходит. Локальные tests/builds не запускались по workflow реализации numbered
plans. Windows CI и публикации завершены: Core `0.0.000380` подтверждён
успешным workflow `36254214711` на SHA
`279689476a283fac3b9e6beddadd09f97bfee318`; supervisor `0.0.000046`, shell-host
`0.0.000109` и UI bundle `0.0.000122` имеют опубликованные артефакты после
успешных module workflows. Совместимый manifest workflow `36255965206` прошёл
и ссылается на текущий SHA и эти версии. Подробные ссылки и release hashes — в
[`release-evidence.md`](release-evidence.md#core-owned-local-model-adaptation-v1-2026-09-26-текущая-задача).

## Plan 122 — Verification Evidence Ledger (закрыт 2026-09-09)

Добавлены content fingerprint, typed verification evidence/status/readiness
contract, fail-closed evaluator, SQLite schema 97 и Core journal persistence.
Существующий snapshot verifier не расширялся до новой authority; полноценный
process runner и downstream consumer adapters остаются typed unavailable.

## Plan 123 — Content-Aware Context Compression (закрыт 2026-09-09)

Добавлены typed content classification/loss/recovery contracts, deterministic
bounded compactor, schema 98 metadata и projection-only diagnostics. Existing
Context Budget/Ledger и ArtifactStore остаются authoritative; protected
diagnostic lines выбирают fallback, а renderer не получает recovery authority.

## Plan 135 — Code Review Lane (закрыт 2026-09-09)

Core владеет diff-bound target identity для workspace changesets, agent git changesets, task worktrees, commit ranges и remote PR metadata. Контракт хранит immutable target/content hashes, bounded changed paths, typed findings, coverage и fail-closed verdict. Storage v104 сохраняет только bounded metadata в транзакционных revision rows, проверяет idempotency/optimistic revision и не принимает secrets, prompts или raw logs.

Команды `save`, `get`, `reconcile` и `interrupt` проходят Core coordinator и journal/replay path; interrupted/partial/unknown coverage не становится `Clean`, re-review сопоставляет findings по fingerprint и помечает отсутствующие open findings stale. Authenticated IPC command 263 и Electron generated bindings дают metadata-only projection; renderer не владеет storage, target identity, verdict или policy.

## Plan 136 — Evidence-Preserving Static Analysis Packs (закрыт 2026-09-09)

Добавлен Core-owned bounded registry pack/rule metadata с trust state,
evidence class, rollout modes, analyzer identity, coverage, findings,
immutable baseline/adoption/delta contract. SQLite schema v105 хранит только
bounded JSON revisions и idempotency metadata. Analyzer execution не
подменяется metadata record: untrusted packs не enforce-ятся, unsupported или
partial coverage не считается clean, а Code Diagnostics, Verification Ledger и
Code Review Lane сохраняют свои authorities. IPC 264/109 и Electron дают
только redacted metadata projection.

## Plan 138 — Skill Source & Update Lifecycle (закрыт 2026-09-09)

Добавлены Core provenance/source и installed-revision contracts с различением
bundled/managed/vendored/workspace/imported modes, trust/update/divergence
state и exact runtime identity. Schema v107 хранит metadata-only revisions;
проверка update возвращает explicit review для divergence и не перезаписывает
локальные skill bytes. Existing Skill Registry, trust pipeline и workspace
mutation остаются owners. IPC 266/111 и Electron проецируют только metadata.

## Plan 137 — Agent Context Loadouts (закрыт 2026-09-09)

Добавлен versioned Core profile/binding/snapshot contract для разрешённых
context assets с bounded entries, exact/latest revision policies,
requiredness, usage mode и fail-closed health. SQLite schema v106 хранит
metadata-only profile revisions с idempotency; source stores, ACL и Context
Namespace остаются владельцами содержания и разрешений. IPC 265/110 и
Electron дают только projection, без raw memory/knowledge/skill payloads.

## Plan 139 — Kernel Capability Facade (закрыт 2026-09-09)

Добавлен Core snapshot-bound facade для typed capability descriptors, calls и
handles: exact snapshot/capability/version проверяются Core, недоступная
capability возвращает `Unavailable`. Existing Tool/Workflow/Child/Context/
Analysis Kernel и policy owners не дублируются. Schema v108 хранит только
bounded facade metadata; IPC 267/112 и Electron остаются projection-only.
## Plan 140 — Authorized Security Assessment Lane (закрыт 2026-09-09)

Core владеет bounded scope/authorization/finding lifecycle с immutable hash и
revision-aware SQLite storage (schema v109). Запуск требует действующей
authorization, policy hash, scope bounds и evidence reference; expired,
denied, revoked, stale и unknown состояния дают non-success. Реализация
metadata-only и не является scanner/executor: эффекты остаются у существующих
policy, approval, provenance и tool owners. Authenticated IPC command 268 и
event 113 проецируют в Electron только redacted metadata.
## Plan 141 — Runtime Service Graph (закрыт 2026-09-09)

Добавлен Core-owned bounded graph contract с lifecycle, immutable revision и
canonical hash. Schema v110 хранит только metadata и idempotency key;
`save/get/pin` проходят через Core, а pin закрепляет active revision без
исполнения node. Authenticated IPC command 269/event 114 и Electron panel
показывают только redacted projection; второй scheduler, permission system,
внешний service и renderer authority не добавлены. Локальные tests, builds,
linters, smoke/E2E в исходном task snapshot не запускались; эта запись не
является текущим CI evidence.

## Plan 142 — Agent Program Optimizer (закрыт 2026-09-09)

Добавлен Core-owned bounded optimizer contract с immutable revision/hash,
deterministic metadata-only score и durable run pin. Schema v111 хранит только
program metadata, idempotency и pin; optimizer не исполняет шаги и не создаёт
новый scheduler/gateway. Authenticated IPC command 270/event 115 и Electron
panel дают redacted projection. Локальные tests, builds, linters, smoke/E2E в
исходном task snapshot не запускались; эта запись не является текущим CI evidence.

## Plan 145 — Git Remote Publication Protocol (закрыт 2026-09-09)

Добавлен bounded Core-owned publication intent с schema v113,
revision/idempotency storage и redacted IPC 272/117. Внешний Git transport,
credentials и push не реализованы: результат остаётся typed
`transport_unavailable`, effect owner — существующий Git/change-set subsystem.
Локальные tests, builds, linters, smoke/E2E в исходном task snapshot не
запускались; эта запись не является текущим CI evidence.

## Plan 143 — Project Knowledge Notebook (закрыт 2026-09-09)

Добавлен Core-owned metadata-only notebook с bounded note references,
immutable revision/hash, schema v112, idempotency и durable active-run pin.
Raw note body, secrets и knowledge authority не переносятся в новый слой.
Authenticated IPC command 271/event 116 и Electron panel дают только
redacted projection. Локальные tests, builds, linters, smoke/E2E в исходном task
snapshot не запускались; эта запись не является текущим CI evidence.

## Plan 146 — Voice Input & Dictation (закрыт 2026-09-09)

Добавлен Core-owned dictation profile с schema v114, immutable revision/hash,
idempotent metadata storage и typed `unavailable` availability. Raw audio и
transcript не сохраняются и не проецируются; authenticated IPC 273/118 и
Electron panel остаются metadata-only. Локальные tests, builds, linters,
smoke/E2E в исходном task snapshot не запускались; эта запись не является
текущим CI evidence.

## Plan 168 — Hardware Fit Evidence Catalog (закрыт 2026-09-14)

Текущий checkout содержит Core contract/matching/admission, SQLite schema v168,
revision-fenced metadata storage, authenticated IPC 276/121 и bounded Electron
projection. Core unit tests (5) и storage regressions (2 + migration) прошли; protocol check,
TypeScript typecheck и `cargo check -p evohime-core --locked` прошли. Electron
В исходном task snapshot Electron unit test был недоступен из-за Node 20;
проект требует Node 22 LTS.
Stage-файлы плана удалены после переноса контракта в `docs/architecture/`.
Exact-commit CI успешен для `0ebdde23`: module router `34878668073`, Core
`34878724254`, UI/shell-host предыдущего task commit `34876365464`/`34876370453`,
compatible release manifest `34881572843`; clippy, tests, release build и
module publication прошли.

## Plan 169 — Agent Client Protocol Bridge (закрыт 2026-09-14)

В checkout уже добавлены typed ACP protocol/auth/backend contracts, bounded
JSON-RPC validation, capability hash snapshot, ACP session state transitions,
schema migration v169 и additive authenticated IPC 277/122. Existing
`EvoHimeV1` preset остаётся обратно совместимым; Electron показывает только
redacted ACP metadata. Локально прошли ACP contract 7/7, migration/storage
полный набор 388/388, clippy, format, protocol check, TypeScript typecheck и
diff-check; в исходном task snapshot Electron unit test был недоступен из-за
Node 20 вместо Node 22. Полный
Core lib-набор дал 898/903: пять старых Linux receipt-key IPC tests получили
`UnsupportedPlatform`, что не относится к ACP и проверяется Windows CI.
После исправления идемпотентности миграции v169 task commit `e65b9da7`
подтверждён module router `34890711685`, Core module `34890773504`
(Windows tests, clippy, release build и publication) и compatible release
manifest `34893277334`.

## Plan 147 — Offline Experience Consolidation Cycle (закрыт 2026-09-09)

Добавлен bounded Core cycle с schema v115, revision/idempotency storage и
offline metadata-only evaluation. External effects, raw experience,
transcripts и secrets не сохраняются; authenticated IPC 274/119 и Electron
panel проецируют только redacted state. Локальные tests, builds, linters,
smoke/E2E в исходном task snapshot не запускались; эта запись не является
текущим CI evidence.

## Plan 170 — Multi-Reviewer Ensemble & Adjudication (закрыт 2026-09-14)

Добавлен Core-owned bounded ensemble contract: immutable profile/snapshot/run,
independence metadata, normalized candidates, conservative clusters,
metadata-only adjudication/completion projection и proof-gated verdicts без
majority authority. SQLite schema v170 хранит только bounded JSON metadata;
run revision и cancel fenced, а existing review/model lanes остаются владельцами
исполнения и findings. Добавлены authenticated IPC command 278/event 123,
Electron settings panel и protocol types. Локально прошли `cargo check`,
Core clippy, storage lib tests 391/391, protocol generation/check,
TypeScript typecheck и `git diff --check`; Electron unit tests требуют Node 22
и в текущем Node 20 окружении не запускались. Exact-commit CI для follow-up
`e64eff14` успешен: Core `34901485643` (Windows tests, clippy, release build и
publication), module router `34901436796`, compatible release manifest
`34904053471`.

## Plan 171 — Language Intelligence Runtime (закрыт 2026-09-15)

Добавлен Core-owned bounded language-intelligence contract для descriptor,
session, document/query snapshot, semantic result и WorkspaceEdit proposal.
Raw LSP JSON-RPC, untrusted executable paths и автоматическое применение edit
не разрешены; lifecycle/provenance/freshness остаются Core/supervisor/policy
границами. SQLite schema v171 использует metadata-only storage с revision
fence; authenticated IPC 279/event 124 и Electron Settings projection
показывают только bounded status/provenance. Локально прошли Core check/clippy,
focused Core 2/2 и storage 1/1, protocol check, TypeScript typecheck и
diff-check.
Exact-commit CI для `ed879822` успешен: Core `34905875176`, shell-host
`34905873117`, UI bundle `34905877063`, module router `34905820570` и
compatible release manifest `34908038913`.

## Plan 172 — Self-healing updater и recovery (закрыт 2026-09-15)

Updater получает bounded versioned recovery journal в `update-state/recovery.json`,
persistent `updater-fallback.exe`, headless `--self-test`/launch preflight,
fail-closed PE/size/SHA validation и redacted recovery projection в status/UI.
Встроенный transaction engine сохраняет rollback; новый executable/module или
web-runtime не добавляются. Локально прошли updater tests, clippy, Electron
typecheck, protocol check и `git diff --check`. Follow-up `a1561ba6` добавил
fallback hash validation, bounded download retry, updater UI crash-loop guard
и package recovery-slot smoke. Exact-commit CI PASS: updater `34910315890`,
UI bundle `34910317294`, shell-host `34910314115`, module router `34910263599`,
compatible release manifest `34910682285`.
Полный Windows workflow `34912288572` также завершён успешно: real-Core IPC
E2E, Electron bundles и bundle security, native package, acceptance matrix,
единственный installer, install/upgrade, rollback, staged rebuild,
выборочный UI apply и публикация прошли без ошибок.
Текущее окно `updater\EvoHimeUpdater.exe` приведено к минималистичной
вертикальной компоновке: остаются логотип, центральный статус, краткое
сообщение и единая progress bar. Если при запуске обнаружены module releases,
launch gate автоматически запускает их применение; отдельные действия
«обновить» и «запустить текущую версию» из окна удалены.

После воспроизведения отказа Windows при замене Electron DLL transaction worker
усилен: перед tree-transaction он проверяет на запись все существующие файлы
установки, а операции копирования и отката повторяются до 120 секунд. Узкий
локальный прогон `evohime-updater` прошёл полностью (33 библиотечных и 3
бинарных теста); установленный клиент для проверки не изменялся. Для доставки
этого исправления без пересборки installer добавлен двухфазный bootstrap:
compatibility manifest сначала требует новый updater, после чего update-agent
сам применяет остальные модули без запуска установленного transaction worker.
Дополнительно выборочный native apply теперь получает PID текущей оболочки и
перед backup ждёт её завершения и освобождения всего Electron-дерева; основное
packaged-приложение также корректно закрывается при запуске module update.
После успешной замены transaction worker передаёт новой оболочке внутренний
маркер post-update launch: она пропускает только рекурсивный launch gate,
поднимает Core и пишет health marker, после чего обычные периодические проверки
обновлений продолжаются. Standalone updater также показывает статический
bootstrap-экран до загрузки React и пишет ошибки загрузки renderer/preload в
shell-main.jsonl, поэтому окно больше не выглядит бесконечно пустым при сбое
пакетированного интерфейса.

## Plan 179 — Core-owned Prompt Strategy Profiles v1

Checkout implementation adds schema v179 immutable strategy metadata, deterministic capability/evidence-aware resolution, exact per-dispatch snapshots and recovery, seven bounded composition types, strategy-aware benchmark identity, authenticated Core operations, guided recipe pinning, and a metadata-only Electron settings panel. Raw prompt and example bodies stay out of registry/provenance projections; generic task/context events carry omission markers. Implementation commit `cbe5a595fb68c3162c9790bced2edfe1a5a92a66` is pushed to `main`; its Core tests, lint, release build and publication, Rust documentation/doctests, module router and compatible manifest workflows all passed. UI bundle tests/build/publication passed at `fec4ff3857d219574503ca320c855bf07af3b6b1`; no UI bundle source files changed between that commit and the implementation SHA.

## Plan 189 — GitHub Public Repository Integration v1 (закрыт 2026-09-27)

Settings «Интеграции» теперь хранит до 50 публичных GitHub `owner/repo`
идентификаторов в Core SQLite (schema v180). По явному выбору/обновлению Core
получает сводку репозитория, до 10 issues и до 10 pull requests; private repos,
credentials и write actions не поддерживаются. Внешний запрос — только
anonymous HTTPS GET к `api.github.com`, с redirect запретом, ответом не более
512 KiB и общим timeout 25 секунд. Открытие Settings и добавление записи сами
не запускают сеть. Добавлены migration/store, Core API contracts и mock-based
tests, а также renderer regression test. Локальные tests/build/typecheck и
visual preview не запускались; exact-commit CI для checkout не доступен до
push, поэтому implementation correctness остаётся pending CI.

## Plan 197 — Изолированные сессии чатов (закрыт 2026-11-20)

Каждый чат выполняется как независимая логическая сессия внутри одного процесса
Core: `ModelGateway` хранит пул всех настроенных профилей провайдеров
(`MODEL_ADDITIONAL_ROUTES_JSON`, `ProviderRouteKey`), `CoreCommand::StartTask` и
`ToolAgent` фиксируют неизменяемый `SessionBinding` (`provider`, `profile_id`,
`model`, `permission_mode`, `isolation_mode`) на уровне запуска, `PermissionMode`
изолируется через per-run `session_id` с очисткой при завершении, `ModelCatalog`
разрешает маршрут по `(provider, profile_id)` с изоляцией `ProviderCatalogCache`,
а `chat_worktree` обеспечивает опциональный `git worktree` вне дерева исходников
с recovery осиротевших worktree. В Electron `provider.select` больше не вызывает
`restartCore()`, `chats.json` аддитивно хранит `session?: ChatSessionSettings`,
и пикеры композера привязаны к активному `chatId`.

Локально прошли фокусные проверки: `evohime-model-gateway` (3 теста на
`resolve_route` и `route_pin`), `evohime-core` (`session_binding_*` — 3 теста,
`chat_worktree` — 3 теста, `model_catalog_parallel_routes_do_not_pollute_cache`
— 1 тест), Electron `npm run check:protocol`, `npm run typecheck`, `vitest`
(`shell-bridge.test.ts`, `provider-store.test.ts`, `chat-store.test.ts` — 121
тест), `git diff --check`. Последующий baseline `0a853f88` прошёл Core, UI
bundle, shell-host, CLI, compatible manifest и остальные module release gates;
исторические workflow/run IDs записаны в `release-evidence.md`.
