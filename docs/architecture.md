# EvoHime — native Windows architecture

EvoHime — локальное Windows-приложение. Браузер, HTTP/REST, WebSocket, PostgreSQL и Python worker не являются runtime-зависимостями продукта.
Пользовательское короткое имя агента — «Ева».

```text
EvoHime.exe               WinUI 3 UI (пользовательский запуск)
        │ desktop-ipc-v1 / named pipe
evohime-core.exe          agent loop, model gateway, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, restart, JSONL diagnostics
```

UI не выполняет shell-команды и не открывает базу. Core владеет workspace, инструментами, моделью и локальным состоянием. Supervisor запускает core в Job Object и завершает дочернее дерево при остановке.

## IPC

Контракт находится в `crates/desktop-ipc/proto/evohime.desktop.proto`.

- major-версия несовместима, minor-расширения совместимы;
- фреймы ограничены 4 MiB;
- события имеют монотонный `sequence_id`;
- UI может запросить replay после последнего sequence ID;
- cancellation передаётся отдельной командой `StopTask`.

## Данные и восстановление

SQLite находится в `%LOCALAPPDATA%\EvoHime` либо в `EVOHIME_DATA_DIR`. Миграции выполняются транзакционно; перед изменением схемы создаётся `.db.bak`. Журнал событий экспортируется в JSONL. Логи core и supervisor пишутся в `%LOCALAPPDATA%\EvoHime\logs`.

## Packaging

```powershell
.\scripts\build-windows-native.ps1
```

Пакет x64 для Windows 11 22H2+ содержит WinUI UI, core, supervisor и `evohime.manifest.json`. Smoke-тест проверяет отсутствие web/PostgreSQL компонентов и повторяемость staging.

Подробное решение: `docs/superpowers/specs/2026-08-04-native-windows-agent-design.md`.
