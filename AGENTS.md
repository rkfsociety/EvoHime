# EvoHime — Agent Guide

EvoHime — локальный Windows AI-agent. Текущий продукт — Electron desktop application с Rust Core, SQLite и Windows supervisor; WinUI 3 сохранён как compatibility runtime и oracle переходных тестов.

## Общение

- Отвечай только на русском языке.
- Обращайся к пользователю «хозяин».
- Используй женский род; допускается лёгкая цундере-манера без оскорблений.

## Архитектура

```text
EvoHime.exe               Electron main + bundled renderer
        │ named pipe, desktop-ipc-v1
evohime-core.exe           Rust agent runtime, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, lifecycle, recovery, logs
```

UI не обращается к workspace, SQLite или model provider напрямую. Core — единственный владелец состояния и выполнения инструментов. Supervisor запускает core, ограничивает дерево процессов Job Object и восстанавливает core после аварийного завершения.

## Требования

- Windows 10 2004+ или Windows 11, x64;
- .NET SDK 10;
- Rust MSVC toolchain;
- Node.js 22 LTS — только для разработки Electron shell; в продукт внешний
  Node.js не входит.

## Команды

```powershell
# Сборка и запуск desktop-приложения
.\start-dev.ps1

# Запуск уже собранного desktop-пакета
.\start-dev.ps1 -SkipBuild

# Сборка переносимого Windows-пакета
.\scripts\build-windows-native.ps1

# Запуск без терминала (double-click): debug-сборка пакета и старт UI
.\start-agent.cmd

# Разовый прогон одной задачи через консольный режим Core
.\scripts\test-agent.ps1 -Prompt 'проверь репозиторий'

# Smoke-тесты packaging, версии, workflow и release retention
$pwsh = Join-Path $PSHOME 'pwsh.exe'
& $pwsh -NoProfile -File scripts\native-package.tests.ps1
& $pwsh -NoProfile -File scripts\version.tests.ps1
& $pwsh -NoProfile -File scripts\native-workflow.tests.ps1
& $pwsh -NoProfile -File scripts\github-retention.tests.ps1

# Desktop UI/IPC tests (в переходный период сохраняется compatibility suite)
& 'C:\Program Files\dotnet\dotnet.exe' test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
& 'C:\Program Files\dotnet\dotnet.exe' test desktop\EvoHime.IpcTests\EvoHime.IpcTests.csproj

# Rust Core foundation
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cargo check -p evohime-supervisor

# Electron shell (desktop\evohime-electron)
cd desktop\evohime-electron
npm run bootstrap        # npm ci без lifecycle-скриптов + allow-list installers
npm run check:protocol   # генерируемые IPC-типы совпадают с каноническим proto
npm run typecheck
npm test                 # adapter, security policy, preload, real-Core E2E (skip без Core)
npm run build; npm run check:bundle   # статические security-проверки production bundles
npm run dev              # dev-запуск оболочки
npm run package          # распакованный Windows package в release\win-unpacked
```

Real-Core E2E тесты требуют собранный Core: `cargo build -p evohime-core`
(или `--release`); без бинарника они помечаются как пропущенные.

Для текущих desktop-задач используй Windows launcher, Rust crates, Electron tests в `desktop/evohime-electron` и Windows packaging scripts. WinUI/IPC tests остаются compatibility suite. Подробные решения завершённой миграции хранятся в памяти проекта; активные работы описаны в `docs/plans/development-plan.md`.
Electron renderer — встроенная часть desktop-приложения, а не web-панель: HTTP server, browser launcher и внешний Node.js runtime не возвращаются в продукт.

Если Rust-сборка останавливается на `prost-build` или другом crate, сначала проверь доступ Cargo к crates.io:

```powershell
Resolve-DnsName index.crates.io
Test-NetConnection index.crates.io -Port 443
```

NuGet и crates.io — независимые источники: успешный `dotnet restore` не означает, что Rust-сборка сможет обновить registry.

## IPC

Протокол редактируется в `crates/desktop-ipc/proto/evohime.desktop.proto`. Rust transport и Electron main/preload adapter должны сохранять совместимость major-версии, sequence replay и bounded frame size. Изменение протокола требует обновить обе стороны и compatibility tests; C# suite сохраняется только как временный compatibility oracle. Генерируемые TypeScript-типы проверяются `npm run check:protocol`.

Команды `workspace.*`, `chat.*`, `provider.*`, `identity.get` и `repository.get` обслуживает main-процесс: это локальное состояние оболочки, а не права. Всё, что доходит до Core, Core проверяет заново.

Подключение к Core аутентифицируется: supervisor выдаёт launch context (`%LOCALAPPDATA%/EvoHime/runtime/session.json`, owner-only DACL) с именем pipe и session secret, Core выдаёт одноразовый nonce, клиент отвечает `HMAC-SHA256(secret, role | client_id | nonce)`. Роли: `shell` (Electron) и `compatibility-shell` (WinUI). Общий known-answer вектор proof продублирован в Rust, Electron и C# тестах — менять его можно только во всех трёх сразу.

## Данные и диагностика

- SQLite и backup: `%LOCALAPPDATA%\EvoHime` или `EVOHIME_DATA_DIR`;
- core log: `%LOCALAPPDATA%\EvoHime\logs\core.jsonl`;
- supervisor log: `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`;
- локальное состояние оболочки: `%LOCALAPPDATA%\EvoHime\shell\` — `workspaces.json`, `chats.json`, `provider.json`;
- экспорт событий — JSONL через `LocalDatabase::export_events_jsonl`.

Миграции SQLite выполняются транзакционно; перед изменением схемы создаётся backup. Секреты не попадают в исходники и логи: ключ провайдера шифруется через Electron `safeStorage` (DPAPI) в `shell\provider.json` и доходит до Core только окружением supervisor; сохранение ключа перезапускает Core. Для локальной разработки используется `.env` рядом с `start-dev.ps1` по allow-list из `.env.example`.

## Правила разработки

1. Не выноси runtime-состояние из Rust Core в Electron UI.
2. Не добавляй бизнес-логику в renderer: UI отображает состояние IPC.
3. Новые Rust-функции и исправления покрывай тестами.
4. Соблюдай sandbox, таймауты, отмену и approval для опасных инструментов.
5. Перед заявлением о готовности запускай свежие проверки и проверяй `git diff --check`.
6. После изменений создавай task-only git-коммит в текущей ветке `main`.
7. Push выполняй только по прямому запросу пользователя. Если push нужен именно для проверки CI результата текущей задачи, это считается частью явно порученной проверки: выполняй push самостоятельно, дождись запуска CI и проверь его итог.
8. После сборки очищай `target/`, `bin/`, `obj/` и временные package artifacts, если они больше не нужны.

## Документы

- `docs/README.md` — карта документации и правило источника истины;
- `docs/architecture.md` — архитектура, runtime, IPC и упаковка;
- `docs/current-state.md` — подтверждённое состояние checkout;
- `docs/plans/development-plan.md` — актуальный implementation plan;
- `docs/plans/roadmap.md` — долгосрочные направления без деталей реализации;
- `docs/features/`, `docs/providers/`, `docs/security/` — справочные разделы;

Все планы лежат в `docs/plans/`. Новый план создаётся файлом в этом каталоге, а не в корне `docs/`.
