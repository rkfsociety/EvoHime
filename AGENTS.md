# EvoHime — Agent Guide

EvoHime — локальный native Windows AI-agent. Поддерживаемый продукт — WinUI 3 desktop application с Rust Core, SQLite и Windows supervisor.

## Общение

- Отвечай только на русском языке.
- Обращайся к пользователю «хозяин».
- Используй женский род; допускается лёгкая цундере-манера без оскорблений.

## Архитектура

```text
EvoHime.exe               WinUI 3 UI (пользовательский запуск)
        │ named pipe, desktop-ipc-v1
evohime-core.exe           Rust agent runtime, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, lifecycle, recovery, logs
```

UI не обращается к workspace, SQLite или model provider напрямую. Core — единственный владелец состояния и выполнения инструментов. Supervisor запускает core, ограничивает дерево процессов Job Object и восстанавливает core после аварийного завершения.

## Требования

- Windows 10 2004+ или Windows 11, x64;
- .NET SDK 10;
- Rust MSVC toolchain.

## Команды

```powershell
# Сборка и запуск native приложения
.\start-dev.ps1

# Запуск уже собранного native-пакета
.\start-dev.ps1 -SkipBuild

# Сборка переносимого native-пакета
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

# WinUI tests (CI прогоняет headless-зеркало EvoHime.IpcTests)
& 'C:\Program Files\dotnet\dotnet.exe' test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
& 'C:\Program Files\dotnet\dotnet.exe' test desktop\EvoHime.IpcTests\EvoHime.IpcTests.csproj

# Rust native foundation
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cargo check -p evohime-supervisor
```

Для native-задач используй только Windows launcher, Rust native crates, WinUI tests и native packaging scripts.
Веб-панель и Vite больше не являются частью продукта и не должны возвращаться в инструкции, запуск или архитектуру.

Если native-сборка останавливается на `prost-build` или другом crate, сначала проверь доступ Cargo к crates.io:

```powershell
Resolve-DnsName index.crates.io
Test-NetConnection index.crates.io -Port 443
```

NuGet и crates.io — независимые источники: успешный `dotnet restore` не означает, что Rust-сборка сможет обновить registry.

## IPC

Протокол редактируется в `crates/desktop-ipc/proto/evohime.desktop.proto`. Rust transport и C# envelope должны сохранять совместимость major-версии, sequence replay и bounded frame size. Изменение протокола требует обновить обе стороны и compatibility tests.

## Данные и диагностика

- SQLite и backup: `%LOCALAPPDATA%\EvoHime` или `EVOHIME_DATA_DIR`;
- core log: `%LOCALAPPDATA%\EvoHime\logs\core.jsonl`;
- supervisor log: `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`;
- экспорт событий — JSONL через `LocalDatabase::export_events_jsonl`.

Миграции SQLite выполняются транзакционно; перед изменением схемы создаётся backup. Секреты должны храниться через Windows Credential Manager/DPAPI, а не в исходниках или логах.

## Правила разработки

1. Не выноси runtime-состояние из native Core в UI.
2. Не добавляй бизнес-логику в WinUI: UI отображает состояние IPC.
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
- `docs/development-plan.md` — актуальный implementation plan;
- `docs/roadmap.md` — долгосрочные направления без деталей реализации;
- `docs/features/`, `docs/providers/`, `docs/security/` — справочные разделы;
- `docs/plans/` — рабочие планы и результаты их ревью.
