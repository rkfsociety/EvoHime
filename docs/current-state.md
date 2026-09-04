# EvoHime — текущее состояние

Обновлено: 2026-09-04.

Этот файл описывает подтверждённое состояние текущего checkout. Исторические
release-gates и результаты отдельных завершённых планов находятся в
[`release-evidence.md`](release-evidence.md); пошаговые незавершённые работы — в
[`plans/README.md`](plans/README.md).

## Продуктовая граница

EvoHime — локальное Windows desktop-приложение с одним пользовательским
ярлыком `EvoHime`. Внутри пакета работают Electron shell, `evohime-core.exe` и
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
| Native package | Core, supervisor, `eva.exe`, analysis worker, listener, transaction и verifier | `scripts/build-windows-native.ps1` |
| Installer | Electron `EvoHime.exe` в постоянном `EvoHime-Setup.exe` | `installer/`, `.github/workflows/windows.yml` |

Для разработки используется PowerShell 7+ и Node.js 22 LTS. В установленный
клиент не вносятся изменения: диагностика и проверки выполняются в исходниках,
временных каталогах или CI.

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

## Следующий незавершённый порядок

Планы 119–143 остаются незавершённой очередью. План 144 — новый предложенный
этап модульных релизов и выборочного обновления компонентов; он не меняет
текущий full-installer механизм до отдельной реализации и закрытия.
Полный каталог, блокирующие и опциональные зависимости находятся в
[`plans/README.md`](plans/README.md), исполняемый порядок — в
[`development-plan.md`](development-plan.md).

## Как поддерживать этот документ

Обновляйте дату и этот файл только по фактам из кода, тестов и release evidence.
Контракт завершённого плана переносится сюда и в `architecture.md`, а сам
временный plan-комплект удаляется. Историю и гипотезы не добавляйте в раздел
текущего состояния.
