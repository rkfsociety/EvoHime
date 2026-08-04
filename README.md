# EvoHime

Нативный Windows-агент без браузерной панели. Приложение состоит из WinUI 3 UI, Rust core и supervisor; состояние хранится локально в SQLite, обмен идёт через версионируемый named pipe IPC.

## Требования

- Windows 11 22H2 или новее;
- .NET SDK 10;
- Rust toolchain с поддержкой `x86_64-pc-windows-msvc`.

## Запуск разработки

```powershell
.\start-dev.ps1
```

Скрипт собирает native-пакет, запускает supervisor скрыто и открывает WinUI-приложение. Для запуска уже собранного каталога:

```powershell
.\start-dev.ps1 -SkipBuild
```

Пакет можно собрать отдельно:

```powershell
.\scripts\build-windows-native.ps1
```

Результат находится в `artifacts/native/windows-x64` и содержит только `EvoHime.Desktop.exe`, `evohime-core.exe`, `evohime-supervisor.exe` и manifest.

## Архитектура

```text
EvoHime.Desktop.exe
        │ versioned named pipe
evohime-core.exe ── SQLite + model gateway + tools
        ▲
evohime-supervisor.exe ── mutex + Job Object + restart + logs
```

Данные и JSONL-логи пользователя хранятся в `%LOCALAPPDATA%\EvoHime`. Веб-клиент, PostgreSQL, Docker и Python worker не являются частью продукта.

## Проверки

```powershell
& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File scripts\native-package.tests.ps1
$dotnet = 'C:\Program Files\dotnet\dotnet.exe'
& $dotnet test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
```

Архитектура и поэтапный план находятся в `docs/superpowers/specs/2026-08-04-native-windows-agent-design.md` и `docs/superpowers/plans/2026-08-04-native-windows-agent.md`.
