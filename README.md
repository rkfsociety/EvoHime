# EvoHime

Первая версия Windows-клиента — `0.0.0001`. Пользовательское короткое имя агента — **Ева**. Обращения «Ева» и «EvoHime» означают одного и того же агента.

Локальный Windows-агент. Приложение состоит из WinUI 3 UI, Rust core и supervisor; состояние хранится локально в SQLite, обмен идёт через версионируемый named pipe IPC.

## Требования

- Windows 10 2004 или Windows 11, x64;
- .NET SDK 10;
- Rust toolchain с поддержкой `x86_64-pc-windows-msvc`.

## Запуск разработки

```powershell
.\start-dev.ps1
```

Скрипт собирает native-пакет и открывает WinUI-приложение. Клиент сам запускает единственный скрытый supervisor, а supervisor — Core. Для запуска уже собранного каталога:

```powershell
.\start-dev.ps1 -SkipBuild
```

Пакет можно собрать отдельно:

```powershell
.\scripts\build-windows-native.ps1
```

Для релиза GitHub Actions собирает единственный пользовательский файл `EvoHime-Setup.exe`. После установки создаётся один ярлык `EvoHime`, запускающий `EvoHime.exe`; Core и supervisor являются внутренними служебными компонентами.
Обычные изменения в `main` проходят CI без публикации релиза. Новый релиз создаётся автоматически только после намеренного изменения `<Version>` в `desktop/EvoHime.Desktop/EvoHime.Desktop.csproj` на версию выше последней опубликованной; тег и Release создаются самим workflow.
Еженедельная retention-задача оставляет только текущий стабильный релиз `vX.Y.Z` и удаляет все предыдущие Releases вместе с соответствующими version-tags; вручную это можно запустить через `workflow_dispatch`.

## Архитектура

```text
EvoHime.exe
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

Архитектура и поэтапный план находятся в `docs/architecture.md` и `docs/development-plan.md`; рабочие планы — в `docs/plans/`.
