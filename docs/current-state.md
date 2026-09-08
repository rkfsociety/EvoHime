# EvoHime — текущее состояние

Обновлено: 2026-09-09.

Этот файл описывает подтверждённое состояние текущего checkout. Исторические
release-gates и результаты отдельных завершённых планов находятся в
[`release-evidence.md`](release-evidence.md); пошаговые незавершённые работы — в
[`plans/README.md`](plans/README.md).

`evohime-local-storage` сейчас имеет внутреннюю migration boundary
`src/migrations.rs` с numbered installers для v1–v26 и v32–v94, а также
bounded-context фасады в `src/domains.rs`. Все исторические installers теперь
вынесены из `lib.rs`; старые store-модули сохраняются только там, где они
ещё являются совместимым публичным контрактом, а новые доменные вызовы
должны проходить через фасады.
В Core внутренние workflow/runtime-модули также закрыты на уровне crate;
проверка ссылок в workspace используется как критерий дальнейшего сужения API.
На текущем этапе `evohime-local-storage` экспортирует 63 модуля (остальные
реализации закрыты на уровне crate), а `evohime-core` — 62; дальнейшее
сокращение оставшихся модулей требует миграции их фактических потребителей
в доменные фасады.

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

Добавлены проверки очереди размера 1 с потоком событий, задержкой чтения и
финальным `task.completed`, а также проверка явного уведомления об ошибке audit.
В Electron bridge крупные поля проверяются отдельным UTF-8 byte-bound helper,
включая Unicode и превышение 512 KiB. В workflow `electron-heavy` real-Core IPC
E2E запускается отдельным обязательным шагом после сборки Core с
`EVOHIME_REQUIRE_REAL_CORE_E2E=1`; локальный режим по-прежнему может пропускать
этот тест без собранного Windows Core.

## Agent Git Change Sets v1 (план 102, реализован)

Текущий checkout содержит Core-owned change-set flow: `observe` захватывает
точный Git baseline и workspace binding, `candidate` повторно проверяет
precondition и включает только attributed agent/tool paths, а pre-existing,
external, secret и ambiguous paths исключаются. `commit` сначала точечно
stage-ит только approved paths, затем использует `git commit --only` с bounded
pathspec и запускаемыми hooks; `keep` и `undo`
проходят через Core, optimistic revision и durable idempotency. Неизвестный
результат Git не ретраится вслепую, а требует reconciliation. Операция
`reconcile` проверяет parent/message/path evidence без повторного Git effect и
различает no-effect, доказанный commit и unknown. Idempotency key
сначала durable claim-ится; конкурентный duplicate не запускает второй effect,
а pending claim после crash остаётся reconciliation-required. Git subprocess
ограничен 120 секундами.

Change sets могут быть привязаны к существующим Incremental Change run и Task
Worktree record. Core проверяет наличие и незавершённость run, состояние
worktree и совпадение его base HEAD до durable записи change set; отдельной
Git-authority для этих consumers нет.

Storage schema — v94. Authenticated IPC command 233/event 78 и generated
Electron bindings передают только bounded redacted metadata; renderer не
получает workspace authority, секреты или raw Git payload. Локально после
реализации обновлены и прошли protocol check, TypeScript typecheck и
компиляционная проверка затронутых Rust crates. Полный acceptance-набор
оставлен GitHub Actions согласно правилу проекта.

## Продуктовая граница

EvoHime — локальное Windows desktop-приложение с одним пользовательским
ярлыком `EvoHime`. Внутри пакета работают Electron shell, отдельный Electron
updater `EvoHimeUpdater.exe`, `evohime-updater.exe`, `evohime-core.exe` и
`evohime-supervisor.exe`; Core владеет состоянием, SQLite, правами и эффектами,
а renderer получает только проекцию через authenticated versioned named pipe.

В текущий release scope входят Windows 10 2004+ / Windows 11 x64 и один
постоянный installer-релиз `installer`. Новые версионные релизы, публичный HTTP
server, внешний Node.js runtime, cloud control plane и обязательная GPU-зависимость
не входят в продукт.

## Runtime и упаковка

| Слой | Реализация | Подтверждение |
| --- | --- | --- |
| UI | Electron 43.4.0, React 19.2.8, TypeScript 5.9.3, Vite 7.3.6 | `desktop/evohime-electron/package.json` |
| IPC | `desktop-ipc-v1`, protobuf bindings, HMAC-сессия supervisor | `crates/desktop-ipc/`, `npm run check:protocol` |
| Core | Rust agent runtime, tools, SQLite и provider gateway | `crates/evohime-core/`, `crates/model-gateway/` |
| Supervisor | mutex, Job Object, lifecycle и recovery | `crates/evohime-supervisor/` |
| Native package | Electron shell `EvoHime.exe`, отдельный Electron updater `EvoHimeUpdater.exe`, Rust updater worker, Core, supervisor, `eva.exe`, analysis worker, listener, transaction и verifier | `scripts/build-windows-native.ps1` |
| Installer | Electron shell и отдельный Electron updater в постоянном `EvoHime-Setup.exe` | `installer/`, `.github/workflows/windows.yml` |

Для разработки используется PowerShell 7+ и Node.js 22 LTS. В установленный
клиент не вносятся изменения: диагностика и проверки выполняются в исходниках,
временных каталогах или CI.

## Актуальное состояние CI и релизов

Последний синхронизированный checkout — коммит
`d9742a0fd9de9b8ed4ed90f0d6eb117be73d3949`. Push-workflow для него завершился
успешно: [run 34030005220](https://github.com/rkfsociety/EvoHime/actions/runs/34030005220).
Полный ручной workflow с публикацией listener runtime запущен через GitHub
Actions: [run 34034590713](https://github.com/rkfsociety/EvoHime/actions/runs/34034590713).

Постоянные каналы поставки разделены по назначению: [`installer`](https://github.com/rkfsociety/EvoHime/releases/tag/installer) — первая установка, [`listener`](https://github.com/rkfsociety/EvoHime/releases/tag/listener) — отдельный модульный release listener runtime.
Локально выполняются только быстрые проверки; полный acceptance-прогон Rust,
Electron, native package и installer выполняется в GitHub Actions.

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
- `Составные задачи`, `Продолжения`, `Анализ`, `Слух`, `Задачи для человека`;
- `Запланировано` — список локальных automation schedules.

Технические панели, включая runtime-контракты, execution backends, безопасность,
диагностику и `Организацию агентов`, находятся в свёрнутом разделе `Интерфейс разработчика`. В
верхней панели доступны `Рабочая панель`, `Открыть браузер`, `Трейс` и индикатор
состояния. `UpdateGate` не показывает рабочую оболочку до завершения startup
проверки обновления.

## Провайдеры и модели

Поддерживаются профили LiteRouter, OpenAI-compatible и OpenAI Responses API;
`mock` используется только в тестах. Anthropic и Ollama остаются planned.
Профили и зашифрованные ключи хранятся в
`%LOCALAPPDATA%\EvoHime\shell\provider.json`; ключ доступен Core только через
окружение supervisor.

Есть два разных маршрута выбора модели:

1. выбор API-модели передаётся в Core для следующего запроса и не требует
   перезапуска Core;
2. выбор активного API-профиля и сохранение ключа перезапускают Core после
   обновления окружения;
3. выбор модели Codex CLI сохраняется в `shell\codex.json`, после чего shell
   перезапускает Core, чтобы новый запуск Codex получил выбранную модель.

Каталог моделей для API и Codex получается динамически. Токены ChatGPT и
cookies не читаются и не сохраняются EvoHime. Панель чата показывает выбранный
режим, профиль и модель; автоматического переключения на другой backend нет.

## Задачи и расписания

`ProjectSidebar` является точкой выбора workspace и чата. `TaskTimeline`
отображает поток Core-событий, approval и recovery; renderer не выполняет
инструменты и не владеет бизнес-логикой.

`ScheduledPanel` получает schedules через `automation.listSchedules` и меняет
активность через `automation.setScheduleEnabled`. Список ограничен 64
элементами, отображает owner (`user` или `workspace`), UTC-время, revision и
последний слот. Приостановленные записи явно помечаются как `paused`.

`OperationsPanel` объединяет пользовательский self-repair, память и pending
items, child-задачи, Pulse, инструменты, локальный индекс workspace, refinement
и ambient proposals. Ошибки недоступных optional adapters остаются typed
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
- update transaction: `%LOCALAPPDATA%\EvoHime\update-state\`;
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

Свежие узкие проверки этого изменения:

| Проверка | Результат |
| --- | --- |
| `cargo test -p evohime-context-budget --lib` | PASS, 142/142 |
| `cargo test -p evohime-core --lib doctor::tests` | PASS, 11/11 |
| `cargo test -p evohime-local-storage --lib latest_schema` | PASS |
| `cargo check -p evohime-core --benches` | PASS |
| `cargo bench -p evohime-core --bench runtime_paths -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10` | PASS |

Полный workspace test и полный набор Electron-тестов для этой задачи не
запускались; перечисленные результаты относятся только к затронутым модулям.

## Подтверждённые проверки checkout

Последний свежий прогон перед обновлением документации:

| Проверка | Результат |
| --- | --- |
| `scripts/documentation.tests.ps1` | PASS, 168 tracked text files |
| `npm run check:protocol` | PASS |
| `npm run typecheck` | PASS |
| `npm test` | PASS, 121 files passed / 3 skipped; 549 tests passed / 8 skipped |
| `cargo test -p evohime-core persistent_agent_registry --lib` | PASS, 6/6 |
| `cargo test -p evohime-local-storage schema_90_migrates_guided_calibration_and_persistent_agent_registry_atomically --lib` | PASS, 1/1 |
| `npm run test -- --run tests/persistent-agent-organization-registry.test.tsx` | PASS, 1/1 |
| `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc` | PASS, Core 815/815; local-storage 283/283; desktop-ipc 36/36; doctests 0 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS |
| `cargo check --locked -p evohime-supervisor -p evohime-updater` | PASS |
| `npm run build` / `npm run check:bundle` | PASS |
| `npm run package` / `scripts/native-package.tests.ps1` | PASS |
| `scripts/runtime-stall-guard.tests.ps1` | PASS |

Пропущены только authenticated-core/real-Core/source-update E2E, которым в этом checkout не
предоставлен собранный runtime или включающий их флаг. Это не означает, что
релизный acceptance-прогон выполнен заново.

В текущем checkout реализован план 144:
native package генерирует `evohime.components.json` для первоначальной поставки;
каждый runtime-модуль имеет отдельную semver-версию и собственный versioned Release
`module-<module>-v<semver>` с manifest, размером, SHA-256, зависимостями и restart policy.
После успешной публикации старый Release этого модуля удаляется. Router не
использует commit diff или SHA: он сравнивает `release-versions/<module>.txt`
с последним тегом `module-<module>-v<semver>` и запускает workflow только для
модуля, чья локальная версия новее опубликованной.
В текущем checkout добавлен fixed release `compatibility` с дешёвым asset
`evohime.compatible.json`: он фиксирует конкретный release tag, версию, artifact,
размер, SHA-256, зависимости и минимальную версию updater для каждого модуля.
Updater принимает только этот согласованный набор; если установленная версия
updater ниже требования, сначала обновляется сам updater, после чего новый worker
применяет остальные модули.
`shell-host` теперь публикуется полным ZIP из `win-unpacked`, включая
`resources/app.asar`; native package собирается из артефактов Rust/Electron,
переданных из проверочных jobs, без повторного Cargo/npm package.
Отдельное Electron-приложение `EvoHimeUpdater.exe` следует утверждённому референсу
[`update-window-design.md`](update-window-design.md): тёмная оболочка EvoHime,
отдельные состояния проверки и обновления, а при ошибке releases окно остаётся
открытым для явного действия пользователя. Его невидимый Rust worker собирается
без Windows console subsystem и не рисует пользовательский интерфейс.
`installer` оставлен только для первоначальной установки или полного
восстановления; его отсутствие в router трактуется как «релиз ещё не создан»
только при подтверждённом HTTP 404, а сетевые и повреждённые ответы останавливают
маршрутизацию. Общий component Release не используется.

## Plan 119 — Execution Environment Profiles v1 (закрыт 2026-09-08)

Core/storage реализуют bounded profile envelope, canonical hash, typed owner
references, pinned/follow-compatible preflight, fail-closed diagnostics,
activation/current snapshots, run pinning, optimistic revision и durable
idempotency outcomes в schema v93. Поддержанные metadata adapters — routing,
backend, external-agent preset, execution policy и approval policy; остальные
owner kinds остаются typed `unavailable_owner` до появления безопасного
versioned lookup и не могут незаметно активироваться.

IPC command 260/event 105 проходит authenticated Core path, replay/resync и
Electron metadata-only projection. Свежие проверки: Core profile 6/6,
local-storage profile 3/3, protocol/typecheck/focused UI 1/1, полный Electron
suite 129 files / 572 tests passed / 1 skipped file / 4 skipped tests,
production build и bundle check, native package smoke. Полный Rust suite для
затронутых crates также прошёл до финального documentation-only переноса.

## Следующий незавершённый порядок

Планы 120–143 остаются незавершённой очередью. Планы 102, 119 и 144 реализованы и закрыты;
его подтверждённый контракт находится в `architecture.md`, а evidence — в
`release-evidence.md`.
Полный каталог, блокирующие и опциональные зависимости находятся в
[`plans/README.md`](plans/README.md), исполняемый порядок — в
[`development-plan.md`](development-plan.md).

## Как поддерживать этот документ

Обновляйте дату и этот файл только по фактам из кода, тестов и release evidence.
Контракт завершённого плана переносится сюда и в `architecture.md`, а сам
временный plan-комплект удаляется. Историю и гипотезы не добавляйте в раздел
текущего состояния.
