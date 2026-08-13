# EvoHime

Первая версия Windows-клиента — `0.0.0001`. Пользовательское короткое имя агента — **Ева**. Обращения «Ева» и «EvoHime» означают одного и того же агента.

Локальный Windows-агент. Текущий runtime состоит из WinUI 3 UI, Rust Core и supervisor; состояние хранится локально в SQLite, обмен идёт через версионируемый named pipe IPC. Утверждённая целевая оболочка — Electron; переход описан в [`docs/plans/0-electron-shell-migration.md`](docs/plans/0-electron-shell-migration.md).

## Требования

- Windows 10 2004 или Windows 11, x64;
- .NET SDK 10;
- Rust toolchain с поддержкой `x86_64-pc-windows-msvc`.

## Запуск разработки

```powershell
.\start-dev.ps1
```

Скрипт собирает текущий Windows-пакет и открывает WinUI-клиент. Клиент сам запускает единственный скрытый supervisor, а supervisor — Core. После завершения Electron migration этот launcher должен открывать Electron-клиент. Для запуска уже собранного каталога:

```powershell
.\start-dev.ps1 -SkipBuild
```

Пакет можно собрать отдельно:

```powershell
.\scripts\build-windows-native.ps1
```

Для релиза GitHub Actions собирает единственный пользовательский файл `EvoHime-Setup.exe`. После установки создаётся один ярлык `EvoHime`, запускающий `EvoHime.exe`; Core и supervisor являются внутренними служебными компонентами.
Обычные изменения в `main` проходят CI без публикации релиза. Новый релиз создаётся автоматически только после намеренного изменения версии desktop-пакета; тег и Release создаются самим workflow.
Еженедельная retention-задача оставляет только текущий стабильный релиз `vX.Y.Z` и удаляет все предыдущие Releases вместе с соответствующими version-tags; вручную это можно запустить через `workflow_dispatch`.

## Архитектура

```text
EvoHime.exe               WinUI 3 UI (сейчас) / Electron UI (цель)
        │ versioned named pipe
evohime-core.exe ── SQLite + model gateway + tools
        ▲
evohime-supervisor.exe ── mutex + Job Object + restart + logs
```

Данные и JSONL-логи пользователя хранятся в `%LOCALAPPDATA%\EvoHime`.

## Проверки

```powershell
& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File scripts\native-package.tests.ps1
$dotnet = 'C:\Program Files\dotnet\dotnet.exe'
& $dotnet test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
```

Архитектура цели находится в [`docs/architecture.md`](docs/architecture.md), фактическое состояние — в [`docs/current-state.md`](docs/current-state.md), общий порядок — в [`docs/development-plan.md`](docs/development-plan.md), подробные работы — в [`docs/plans/`](docs/plans/).
