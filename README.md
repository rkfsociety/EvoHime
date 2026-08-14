# EvoHime

Первая версия Windows-клиента — `0.0.0001`. Пользовательское короткое имя агента — **Ева**. Обращения «Ева» и «EvoHime» означают одного и того же агента.

Локальный Windows-агент. Текущий runtime состоит из Electron desktop shell, Rust Core и supervisor; состояние хранится локально в SQLite, обмен идёт через аутентифицированный версионируемый named pipe IPC. WinUI 3 сохранён только как compatibility runtime и набор проверок переходного периода.

## Требования

- Windows 10 2004 или Windows 11, x64;
- Rust toolchain с поддержкой `x86_64-pc-windows-msvc`;
- Node.js 22 LTS — только для разработки Electron shell; в продукт внешний Node.js не входит;
- .NET SDK 10 — только для compatibility suite WinUI/IPC.

## Запуск разработки

```powershell
.\start-dev.ps1
```

Скрипт собирает текущий Windows-пакет и открывает Electron-клиент. Клиент сам запускает единственный скрытый supervisor, а supervisor — Core. Для запуска уже собранного каталога:

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

## Ключ провайдера

В приложении ключ вводится в настройках: шестерёнка рядом с аккаунтом внизу левой панели. Ключ шифруется средствами ОС (DPAPI через Electron `safeStorage`), хранится в `%LOCALAPPDATA%\EvoHime\shell\provider.json` и передаётся Core через окружение supervisor; сохранение ключа перезапускает Core. Renderer видит только признак «ключ задан».

Для локальной разработки достаточно `.env` рядом с `start-dev.ps1`:

```powershell
Copy-Item .env.example .env
```

`start-dev.ps1` читает из `.env` только перечисленные в `.env.example` имена и передаёт их дочерним процессам. Сам `.env` в репозиторий не попадает и в пакет не копируется.

## Архитектура

```text
EvoHime.exe               Electron main + bundled renderer
        │ versioned named pipe
evohime-core.exe ── SQLite + model gateway + tools
        ▲
evohime-supervisor.exe ── mutex + Job Object + restart + logs
        │
evohime-transaction.exe ── backup, commit и rollback обновлений
```

Данные и JSONL-логи пользователя хранятся в `%LOCALAPPDATA%\EvoHime`. Локальное состояние оболочки (список workspace, чаты, настройки провайдера) лежит там же в подкаталоге `shell\`; владельцем задач, инструментов и журнала событий остаётся Core.

## Проверки

```powershell
& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File scripts\native-package.tests.ps1
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cd desktop\evohime-electron; npm run typecheck; npm test
```

Compatibility suite WinUI/IPC (нужен .NET SDK 10):

```powershell
& 'C:\Program Files\dotnet\dotnet.exe' test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
```

Архитектура находится в [`docs/architecture.md`](docs/architecture.md), фактическое состояние — в [`docs/current-state.md`](docs/current-state.md), ближайший порядок — в [`docs/plans/development-plan.md`](docs/plans/development-plan.md).
