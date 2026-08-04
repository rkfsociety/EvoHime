# EvoHime — Agent Guide

EvoHime — новый native Windows AI-agent. Поддерживаемый продукт — WinUI 3 desktop application; браузерный клиент, Electron, Tauri, WebView, PostgreSQL и Python worker в продуктовый контур не входят.

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

- Windows 11 22H2+, x64;
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

# Smoke manifest и идемпотентность staging
& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File scripts\native-package.tests.ps1

# WinUI tests
& 'C:\Program Files\dotnet\dotnet.exe' test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64

# Rust native foundation
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cargo check -p evohime-supervisor
```

Не запускай старый web stack, `cargo run -p evohime-server`, PostgreSQL setup, `npm run dev` или Docker compose для native-задач.

## IPC

Протокол редактируется в `crates/desktop-ipc/proto/evohime.desktop.proto`. Rust transport и C# envelope должны сохранять совместимость major-версии, sequence replay и bounded frame size. Изменение протокола требует обновить обе стороны и compatibility tests.

## Данные и диагностика

- SQLite и backup: `%LOCALAPPDATA%\EvoHime` или `EVOHIME_DATA_DIR`;
- core log: `%LOCALAPPDATA%\EvoHime\logs\core.jsonl`;
- supervisor log: `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`;
- экспорт событий — JSONL через `LocalDatabase::export_events_jsonl`.

Миграции SQLite выполняются транзакционно; перед изменением схемы создаётся backup. Секреты должны храниться через Windows Credential Manager/DPAPI, а не в исходниках или логах.

## Правила разработки

1. Не возвращай web UI или HTTP/WS как обязательный runtime-контур.
2. Не добавляй бизнес-логику в WinUI: UI отображает состояние IPC.
3. Новые Rust-функции и исправления покрывай тестами.
4. Соблюдай sandbox, таймауты, отмену и approval для опасных инструментов.
5. Перед заявлением о готовности запускай свежие проверки и проверяй `git diff --check`.
6. После изменений создавай task-only git-коммит в текущей ветке `main`.
7. Push выполняй только по прямому запросу пользователя.
8. После сборки очищай `target/`, `bin/`, `obj/` и временные package artifacts, если они больше не нужны.

## Документы

- `docs/superpowers/specs/2026-08-04-native-windows-agent-design.md` — архитектура;
- `docs/superpowers/plans/2026-08-04-native-windows-agent.md` — implementation plan;
- `docs/architecture.md` — deployment/runtime overview.
