# EvoHime — текущее состояние

Обновлено: 2026-08-04.

## Продукт

EvoHime — локальный Windows-клиент для coding-agent задач. Пользовательское имя агента — **Ева**. Первая версия — `0.0.0001`.

Пользователь получает один `EvoHime-Setup.exe`. После установки на рабочем столе появляется один ярлык `EvoHime`, запускающий `EvoHime.exe`.

## Runtime

- `EvoHime.exe` — WinUI 3 интерфейс;
- `evohime-core.exe` — Rust agent loop, model gateway, tools, permissions, approvals и SQLite;
- `evohime-supervisor.exe` — single-instance mutex, Job Object, restart и диагностика;
- versioned protobuf over Windows named pipe — единственный UI/Core transport;
- `%LOCALAPPDATA%\EvoHime` — локальные данные и JSONL-логи.

Core и supervisor — внутренние компоненты установки, не отдельные пользовательские продукты.

## Готово

- native foundation: Core, SQLite, IPC, supervisor, event replay и diagnostics;
- WinUI workspace picker, persistence, tray, notifications и reconnect;
- streamed task timeline, cancellation и approval round-trip;
- native package smoke tests и Windows CI;
- единый Inno Setup installer с одним desktop shortcut;
- release retention: сохраняется только последний стабильный `vX.Y.Z` release/tag;
- имя агента «Ева» передаётся в system context Core.

## Следующий этап

1. Files, Editor, Git и controlled Terminal;
2. diff/command preview в approval UI;
3. Credential Manager/DPAPI, backup/restore и crash recovery;
4. update/rollback и дальнейшая проверка установочного UX.

## Граница продукта

Пользовательский продукт ограничен `EvoHime-Setup.exe`, `EvoHime.exe`, локальным Core, supervisor и данными в профиле Windows. Исследовательские и экспериментальные каталоги не входят в установочный runtime.
