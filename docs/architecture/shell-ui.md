# EvoHime — доменный контракт: оболочка Electron, UI-проекции и обновления

Канонический контракт домена Electron-оболочки, визуального контракта, UI-расширений и self-healing updater. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).

## Оболочка

Renderer состоит из панели проектов и чатов, ленты диалога и инструментальных разделов.

| Поверхность | Назначение |
| --- | --- |
| `ProjectSidebar` | проекты (workspace) и чаты внутри проекта; аккаунт и вход в настройки внизу |
| `HomeScreen` | стартовый экран; первый запрос сам создаёт чат |
| `TaskTimeline` + `ActivityLine` + `transcript.ts` | ход задачи, свёрнутый в читаемую ленту; ответы агента рендерятся Markdown |
| `tool-names.ts` | русские подписи инструментов вместо служебных идентификаторов |
| `ChatSummaryPanel` | сводка чата: изменения репозитория, файлы, терминал, контекст и фоновые задачи |
| `ModelPicker` | выбор API-модели для следующего запроса или модели Codex CLI; каталог приходит от выбранного backend |
| `ProviderForm` / `CodexPanel` | единая поверхность настроек источника моделей с внутренними вкладками API и Codex CLI |
| `PlanReviewPanel` | коллективное read-only ревью Markdown-плана несколькими моделями и synthesis-моделью; итог копируется в буфер или экспортируется в Markdown, история очищается кнопкой |
| `RecoveryBanner` + `recovery-state.ts` | состояние восстановления, выведенное только из подтверждённых Core событий |
| `OperationsPanel` | self-repair, память и конфликты, child-задачи, Pulse, индекс workspace, refinement и ambient proposals |
| `HiveDashboard` (вкладка «Улей») | единый дашборд по `HiveStateSnapshot`: департаменты, Королева Ева, метрики и бюджет, активные агенты, логи, подтверждения и настройка изолированного Eva self-development workspace; выбор провайдера идёт через `provider.select`, интервал опроса принадлежит оболочке, а статус Pulse читается отдельной проекцией `eva_pulse.status` и не является режимом Улья; неподтверждённые величины не подменяются mock-значениями |
| `ScheduledPanel` | отдельный список automation schedules: owner, UTC-время, revision, последний слот и enabled/paused |
| `OverviewPanel`, `TracePanel` | сводка событий запуска и фильтруемая трасса |

Бизнес-логики в renderer нет: он отображает состояние, полученное через IPC, и отправляет команды.

### Визуальный контракт оболочки

Текущий renderer использует единую тёмную палитру: фон приложения `#101218`,
sidebar `#181b23`, поверхности `#191c25`, hover/active `#252b39`, границы
`#343b4d`, основной акцент `#9b8cff`, вторичный акцент `#7595ff`. Композер
оформлен как компактная тёмная панель без свечения: строка контекста содержит
проект, ветку и состояние Git, режим доступа, провайдера, модель и индикатор
контекста. При нехватке ширины элементы строки переносятся, не сжимаясь и не
обрезая индикатор контекста; ветка и состояние Git остаются на экране, пока
выбран workspace. Поле задачи содержит прикрепление файлов, переключатель
веб-поиска и отправку. Клавиатурные подсказки и состояние Core находятся в
тонком футере.
Прикрепление открывает выбор нескольких файлов текущего workspace или принимает
файлы из буфера обмена через `Ctrl+V`; текстовая вставка остаётся штатной. В обоих
случаях проверяется, что файл находится внутри выбранного workspace, а в запрос
передаются только относительные пути через существующий preload bridge. Режим
доступа выбирается непосредственно в строке контекста и применяется через
`core.setPermissionMode`; веб-поиск включает явную инструкцию для следующего
запроса. Эти действия не
создают новый IPC-контракт и не переносят authority из Core. Sidebar
содержит бренд, выбранный workspace, чаты и быстрые действия «Новый чат»,
«Запланировано» и «Плагины». Пользовательские разделы «Обзор», «Ревью планов»,
«Память и Pulse», «Продолжения», «Анализ», «Слух», «Запланировано», а также
«Настройки» открываются из
основной навигации или выпадающего меню пользователя. Внутренние Core/agent/model
контракты, диагностика и служебные проекции не являются вкладками: ими управляет
ядро, агент и модель. В контекстной области чата остаются только рабочая панель,
браузер и трейс, а настройки открываются отдельным пользовательским действием.

Меню закрывается по Escape, клику вне меню и после выбора раздела. Проект
выбирается существующей поверхностью `ProjectSidebar`, а чат остаётся
привязанным к выбранному workspace. Временные вложения и флаг веб-поиска живут
в composer renderer до отправки; IPC-контракт, Core-owned state, approvals,
recovery, updates и настройки компонентов не меняются.

## Обновления из исходников

### Component manifest и отдельный UI bundle

Native package дополнительно содержит `evohime.components.json` с bounded
`evohime.component-manifest.v1`: identity релиза, component inventory, размеры
и SHA-256 артефактов, зависимости и restart class. IDs компонентов, имена
артефактов и destination paths должны быть уникальными: updater отвергает
коллизии до загрузки байтов. Список module releases читается по bounded GitHub
pagination, чтобы не пропускать релиз нужного модуля за первой страницей.
Manifest version также обязана совпадать с версией в module release tag; при
рассинхроне updater останавливается до загрузки бинарника. Draft и prerelease
релизы не участвуют в выборе стабильного module update. Текстовые manifest-ы
читаются потоково и ограничиваются 64 KiB до JSON-парсинга. Все updater
staging-файлы получают UUID-имя и открываются без overwrite до atomic rename.
`listener-runtime` является
отдельным data-directory runtime и поэтому не входит в этот install-tree граф;
его версия и целостность проверяются отдельным runtime manifest. `evohime-updater` проверяет
этот marker до staged apply и отказывает при небезопасном пути, неизвестной
зависимости, цикле, неверном размере или хеше. Legacy `evohime.manifest.json`
остаётся bootstrap-контрактом.

Metadata component manifest развивается обратно совместимо: transaction worker
принимает пропущенные legacy-поля с безопасными значениями по умолчанию и
игнорирует additive metadata, а updater перед staged apply записывает полный
canonical marker. Поэтому исправления описательных полей не требуют полной
пересборки installer для уже установленного клиента.

Electron renderer собирается в отдельный `out/ui-bundle`; component downloader
отвергает UI-архив больше 128 MiB до скачивания и больше 512 MiB после
распаковки, проверяет имена файлов до декомпрессии и распаковывает bundle во
временное дерево. Только полностью проверенное дерево публикуется в staging;
частичный или повреждённый архив не заменяет предыдущий bundle. Main
процесс выбирает только validated version из `ui-active.json`. Повреждённый или
неполный pointer возвращает bundled fallback. Mixed UI+native apply использует
общий transaction journal с backup native-файлов и восстановлением UI pointer;
при неподдерживаемом manifest клиент сохраняет состояние для ручного recovery.

Production-обновление использует fixed GitHub Release `compatibility` с asset
`evohime.compatible.json`. Манифест фиксирует один совместимый набор: точный
module release каждого компонента, его версию, artifact, размер, SHA-256,
зависимости, restart class и минимальную версию updater. Поэтому updater не
смешивает независимые последние версии; публикация этого манифеста не требует
пересборки installer. Если установлен updater ниже требования, сначала
скачивается и атомарно заменяется только updater, затем новый updater получает
остальные компоненты. Повышение minimum updater поэтому служит двухфазным
bootstrap для уже установленных клиентов: старая версия сначала доставляет
только новый updater, а он перед смешанным обновлением атомарно заменяет уже
не зависит от существующего transaction worker: встроенный engine сначала
восстанавливает незавершённую транзакцию, затем применяет выбранные модули,
включая замену самого `evohime-transaction.exe` как обычного компонента.

`shell-host` публикуется как полный `shell-host.zip` из `win-unpacked`, включая
`EvoHime.exe`, `resources/app.asar` и зависимые Electron runtime-файлы. Его
применение распаковывается во временное shell-tree и проходит общей backup/
rollback-транзакцией, сохраняя Core и остальные невыбранные модули.

`updater` публикуется как `updater.zip`: внутри лежат `evohime-updater.exe` с
встроенным transaction/recovery engine и полный самостоятельный Electron package `updater/`, включая
`updater/EvoHimeUpdater.exe` и его `resources/app.asar`. При self-update архив
распаковывается и проверяется до замены; bootstrap атомарно меняет worker,
каталог UI, архив и component manifest с отдельными backup/rollback. Старый
клиент с корневым `EvoHimeUpdater.exe` остаётся совместимым до первого
успешного обновления и затем мигрирует в новый каталог автоматически.

Web installer является базовым способом первой установки и восстановления. После его
запуска `EvoHimeUpdater.exe` проверяет совместимый набор; отсутствие локального
component marker означает базовую версию `0.0.0`, поэтому bootstrap получает
весь необходимый набор перед обычным запуском. Отдельного полного installer нет.
На пустом каталоге component transaction допускает отсутствующие до операции
файлы: backup хранит только существующие компоненты, а rollback удаляет
добавленные этой транзакцией файлы и сохраняет пользовательские дополнительные
файлы. Старые transaction journals без этого поля восстанавливаются по старой
семантике. Если shell ещё отсутствует, действие запуска в updater UI передаёт
управление тому же verified module apply и не пытается запустить отсутствующий
`EvoHime.exe`.
Локальная пересборка остаётся dev-only fallback и сравнивает коммит своей
сборки с вершиной отслеживаемой ветки.

```text
update.json          репозиторий, ветка, launchPolicy, интервал проверки
%LOCALAPPDATA%\EvoHime\source           git checkout, которым владеет обновление
%LOCALAPPDATA%\EvoHime\update-staging   собранный пакет до подмены
%LOCALAPPDATA%\EvoHime\update-state     журнал транзакции, recovery journal и backup
```

### Self-healing updater и recovery v1 (план 172, реализован 2026-09-15)

Recovery control-plane использует `recovery.json` schema 1 с bounded
идемпотентными фазами `prepared`/`downloaded`/`verified`/`replaced`/
`self-tested`/`committed`/`rolled-back`/`manual-recovery`. Первый запуск Rust
updater создаёт persistent `updater-fallback.exe` как last-known-good копию;
`--launch` выполняет headless preflight, а `--self-test` проверяет собственный
PE header, размер/hash, state paths и recovery state без запуска Core или UI.
Повреждённый или отсутствующий transaction worker больше не блокирует recovery:
встроенный transaction engine продолжает работу и заменяет этот compatibility-
компонент проверенным module artifact в составе общей транзакции. Status/UI получают лишь
bounded phase, slot/version, fallback, retry count и typed reason code; после
исчерпания retry updater переходит в manual recovery.

JSON-запросы updater повторяются не более двух раз, скачивание артефакта — не
более трёх раз с удалением `.part` после ошибки; неполный или несовпавший по
размеру, SHA-256 или PE header файл не становится active. Electron updater UI
хранит bounded `updater-crash-loop.json`: три старта в течение десяти минут
блокируют новый цикл и показывают manual recovery. Успешный старт сбрасывает
счётчик. Полная потеря recovery-копии или дерева установки остаётся случаем
для ручного восстановления через installer или repair bridge.

- `evohime.build.json` рядом с бинарниками хранит коммит и ветку сборки; без маркера версия считается неизвестной и клиент пересобирается;
- коммит, не трогающий код клиента (документация, планы, CI-конфиг), не вызывает пересборку: клиент сравнивает установленный коммит с целевым через compare API и пропускает обновление, если ни один изменённый путь не влияет на сборку. Любая неопределённость — обрезанный diff, незнакомый путь, недоступный API — трактуется как «код менялся»: лишняя пересборка дешевле устаревшего клиента;
- production-обновление использует `compatibility` manifest и конкретные независимые module releases `module-<module>-v<semver>`; перед скачиванием проверяются identity набора, зависимости, размер и SHA-256 каждого артефакта. Решение об обновлении принимается по этому набору, а не по несогласованным «последним» версиям или commit. Commit-based source rebuild сохранён только как явно включаемый dev-only режим;
- проверка обновлений ходит в GitHub API с токеном пользователя, если он есть: анонимный лимит — 60 запросов в час на IP, и выбранный чужим трафиком с того же адреса лимит останавливает обновления с `403`, тогда как с токеном лимит 5000. Источники по убыванию явности: `EVOHIME_UPDATE_GITHUB_TOKEN`, поле `githubToken` в `update.json`, `GH_TOKEN`/`GITHUB_TOKEN`, `gh auth token`. Токен не обязателен — без него проверка работает как раньше; он уходит только на `api.github.com`, не пишется в логи и не сохраняется клиентом;
- git обновления работает без интерактива: сохранённые на машине учётные данные (`gh`, credential manager) используются, но ни git, ни credential helper не могут открыть диалог или спросить пароль в терминале. Зависшее за невидимым окном обновление хуже упавшего — оно блокирует запуск;
- `update.json` пишет установщик, репозиторий принимается только по `https`, ветка и интервал проверки нормализуются — конфигурация не может увести сборку на чужой источник или превратить проверку в busy loop;
- установленный ярлык и post-install запуск вызывают `updater\EvoHimeUpdater.exe` с аргументом `--evohime-updater`. Это самостоятельный Electron package внутри модуля updater: он не поднимает Core, supervisor, трей или обычное окно. Проверку, скачивание и применение он передаёт невидимому Rust worker `evohime-updater.exe`, внутри которого статически доступен transaction/recovery engine; видимое окно остаётся открытым и получает статус с прогрессом на протяжении проверки, скачивания и верификации. Только после отдельного `requires_exit` от worker окно кратко закрывается для освобождения файлов перед транзакцией или self-update, затем новый updater снова открывает окно либо запускает `EvoHime.exe`. Перед запуском shell gate дожидается проверки, а при наличии модульных релизов автоматически передаёт их в apply и не предоставляет режима запуска без обновления. Main-процесс дополнительно проводит тот же невидимый safety gate до старта Core и supervisor, но renderer shell больше не рисует legacy full-window `UpdateGate`; внутри приложения остаётся только компактная иконка для фонового обновления уже работающего shell;
- updater worker запускается без консоли и напрямую как native process, без `cmd.exe`/shell-границы; перед установкой worker проверяется как полный x64 PE32+ с корректными DOS/PE-заголовками, а встроенный transaction engine пишет точный rollback/validation failure в `update-state/updater.json#error`; ошибка применения переводит UI в `failed`, а не оставляет его в `applying` или ошибочно показывает доступное обновление;
- у уже запущенного клиента фоновая проверка собирает обновление в staging и предлагает перезапуск баннером, не прерывая работу;
- пользовательский repair-run доступен в `OperationsPanel` после bounded digest из трёх ошибок задач. Перед `repair.start` пользователь выбирает provider и model; пара сохраняется в repair status и переносится через diagnose, commit, push и restart. Сама ошибка только показывает кнопку: `repair.start`, `repair.commit`, `repair.push`, `repair.refreshCI` и обновление запускаются отдельными кликами;
- repair-run работает в `%LOCALAPPDATA%\\EvoHime\\repair\\<repair-id>`, проверяет origin выбранного workspace и канонический URL EvoHime, а изменения `AGENTS.md`, `.codex`, workflows, updater, supervisor, receipt, security и `.env*` останавливает до ручного review;
- transaction worker сохраняет backup до post-restart health handshake. Новая оболочка пишет `%LOCALAPPDATA%\\EvoHime\\update-state\\health.json` только после authenticated Core connection; отсутствие marker за 90 секунд вызывает rollback;
- transaction worker запускает проверяемую новую оболочку с внутренним аргументом `--evohime-post-update`: это пропускает только повторный launch gate, который иначе увидел бы ещё не закоммиченную транзакцию и снова передал управление updater, а периодическая проверка после старта остаётся включённой;
- health-marker принимается только с точным bounded JSON-флагом `healthy:true`;
  при timeout worker возвращает прежнее дерево установки, оставляет причину в
  transaction error и не считает обновление применённым. Без `--health-file`
  сохраняется совместимость старого локального тестового режима;
- недостающие Git, Node.js, Rust и MSVC Build Tools ставятся через winget по фиксированным идентификаторам пакетов;
- локальная сборка падает транзиентно (оборванная загрузка Electron, недописанный `release/`), поэтому после первой неудачи производные каталоги удаляются и сборка повторяется один раз; вторая неудача показывается как есть. Полный вывод сборки лежит в `%LOCALAPPDATA%\EvoHime\logs\update-build.log` — UI показывает только последнюю строку;
- встроенный transaction engine выполняет тот же staged apply: дожидается не только выхода оболочки, но и момента, когда все уже существующие файлы дерева установки действительно доступны на запись (включая Electron DLL, которые могут удерживаться дочерними процессами или защитным сканером), делает полный backup установки, переносит staging и при любой ошибке восстанавливает прежнюю установку. Активный `evohime-updater.exe` исключается из этого preflight и backup/copy, потому что worker сам держит файл открытым; self-replacement updater выполняется отдельной отложенной операцией после выхода worker. Копирование переживает блокировки повторами до 120 секунд, а незавершённая транзакция откатывается при следующем запуске. Валидатор component manifest принимает также старые `dependencies: null` и одиночные строковые зависимости, а новый native package всегда сериализует это поле массивом. `evohime-transaction.exe` остаётся публикуемым compatibility-компонентом для старых пакетов и полного native package, но production updater не запускает его и не требует его наличия. Выборочное обновление отдельных native-модулей выполняет тот же preflight: updater дожидается выхода оболочки и доступности всего установленного дерева до создания backup и записи.

Неудачное обновление не блокирует работу: установленная сборка запускается как обычно, а причина отказа показывается в UI.

Безопасностные ограничения вынесены в [`../../SECURITY.md`](../../SECURITY.md).

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

## Event Visualizer Registry v1 (план 81)

Core-owned registry разрешает typed event matcher детерминированно по
specificity/priority/id и всегда имеет host-controlled generic fallback.
Встроенные descriptors покрывают tool, file, test, workflow и artifact events;
extension contributions декларативны и не могут заменить security-critical
host renderer. Projection bounded и sensitivity-filtered, действия остаются
Core-owned. SQLite schema v81 и authenticated additive IPC command 235/event 80
хранят только metadata/descriptor state; arbitrary HTML/JS и raw payload не
передаются renderer.

## Bounded Electron diagnostics and event projections

`desktop/evohime-electron/src/main/diagnostics/logger.ts` redacts and
serializes each shell record before enqueue, then writes through one async
queue per Electron process. Queue capacity is bounded by 512 records and
1 MiB of encoded JSONL by default; overflow drops the incoming record and is
reported through bounded logger status. Disk failures disable that logger
instance and drop its pending tail without recursive logging or retries.
Rotation remains bounded to 4 MiB per active file and three generations.
Main and standalone updater use separate `shell-main.jsonl` and
`shell-updater.jsonl` paths because their processes have independent lifecycle
and cannot safely coordinate rotation. Normal quit and controlled exits drain
for at most 1.5 seconds, then continue with a fixed stderr message; a process
crash can lose no more than the configured pending queue. Support-bundle
creation flushes the main logger first and includes its redacted counters.

The workspace RAG UTF-8 decoder validates borrowed bytes with
`std::str::from_utf8` before creating the required owned text. Invalid UTF-8
continues through the lossy replacement path; UTF-16 BOM decoding and original
byte/chunk semantics are unchanged.

Electron chat event scanning retains only a compact
`coreInstanceId/sessionEpoch/sequenceId/taskId/eventType` boundary. It does not
retain payload strings or duplicate conversation pages; existing Core
projection helpers provide event-ID/sequence replay and conflict semantics.
The local message timestamp LRU is capped at 256 IDs; explicitly loaded
conversation history remains visible and is not silently trimmed. Workflow
polling correlates Core projections by `run_id`, advances only from returned
event pages through the existing `afterSequence` contract, fetches the next
page when full, allows only one projection cycle at a time, and stops only
after a terminal run's final page has been observed. Reconnect starts a new
cycle from the retained event cursor; unmount/run change clears timers.

## Per-chat session isolation (Plan 197)

Each chat inside the single `evohime-core.exe` process runs as an isolated
logical session without spawning a second Core process or restarting Core when
switching already-configured providers or models in the composer:

- **Provider pool:** `ProviderStore.environment()` exports the active provider's
  variables plus all other configured profiles in `MODEL_ADDITIONAL_ROUTES_JSON`
  (`ModelGatewayConfig::from_env` merges them into `ModelGateway`, reserving
  `default`, `hive`, and `hive_fallback`). `provider.select` no longer calls
  `restartCore()`; Core restarts only when secrets change (`provider.save` /
  `provider.clearKey`).
- **Immutable `SessionBinding`:** `StartTask` (`evohime.desktop.proto` fields
  `provider`, `profile_id`, `model`, `permission_mode`, `isolation_mode`) maps
  to `CoreCommand::StartTask.session_binding`. `ToolAgent` resolves the route
  and pinned model once at run start (`ModelGateway::resolve_route`,
  `route_pin`) instead of reading global `SelectedModel` per iteration, and
  unconfigured providers fail immediately with `provider_not_configured` before
  creating an `agent_run` effect.
- **Per-run `PermissionMode`:** When `SessionBinding.permission_mode` is set,
  `ToolAgent` allocates an ephemeral `PermissionEngine` session UUID (`session_id`
  in `ToolContext`, excluding `MicrophoneListen`) and clears it on run exit
  without mutating global workspace modes or bypassing `PolicyRuleSet`.
- **Per-route `ModelCatalog`:** `ModelCatalogRequest` accepts optional
  `provider` and `profile_id`; `IpcBridge::fetch_model_catalog_payload`
  resolves the matching route and caches snapshots in `ProviderCatalogCache`
  keyed by `provider_catalog_scope_key` (distinct per `(provider, profile_id,
  base_url)`).
- **Optional `git worktree` sandbox (`chat_worktree`):** When
  `IsolationMode::IsolatedWorktree` is requested, Core allocates a detached
  worktree in a service directory outside the repository tree (`idempotency_key`
  prefix `chat_session:`), fails closed with `isolated_worktree_unavailable` on
  non-Git workspaces, cleans up clean worktrees on completion, preserves dirty
  worktrees for explicit apply/discard, and marks orphaned dirty worktrees
  `recovery_pending` on restart.
- **Shell persistence:** `chats.json` additively stores optional
  `session?: ChatSessionSettings` per chat (`chat.saveSessionSettings`), and
  composer pickers (`ChatProviderPicker`, `ModelPicker`, isolation selector)
  bind to the active `chatId`.
