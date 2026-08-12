# EvoHime — текущее состояние

Обновлено: 2026-08-12.

## Продукт

EvoHime — локальный Windows-клиент для coding-agent задач. Пользовательское имя агента — **Ева**. Текущая версия native-клиента — `0.0.000032`.

Пользователь получает один `EvoHime-Setup.exe`. После установки на рабочем столе появляется один ярлык `EvoHime`, запускающий `EvoHime.exe`.

## Runtime

- `EvoHime.exe` — WinUI 3 интерфейс;
- `evohime-core.exe` — Rust agent loop, model gateway, tools, permissions, approvals и SQLite;
- `evohime-supervisor.exe` — single-instance mutex, Job Object, restart и диагностика;
- `evohime-transaction.exe` — скрытый transaction worker для backup, commit и rollback обновлений;
- versioned protobuf over Windows named pipe — единственный UI/Core transport;
- `%LOCALAPPDATA%\EvoHime` — локальные данные и JSONL-логи.

Core и supervisor — внутренние компоненты установки, не отдельные пользовательские продукты.

## Готово

- native foundation: Core, SQLite, IPC, supervisor, event replay и diagnostics;
- WinUI workspace picker, persistence, tray, notifications и reconnect;
- streamed task timeline, cancellation и approval round-trip;
- native package smoke tests и Windows CI;
- единый Inno Setup installer с одним desktop shortcut;
- установленный клиент сам поднимает supervisor и Core;
- автообнаружение GitHub Release, SHA-256 проверка installer и upgrade smoke в CI;
- автоматический rollback при ошибке установщика и recovery незавершённой транзакции перед запуском Core;
- release retention: сохраняется только последний стабильный `vX.Y.Z` release/tag;
- имя агента «Ева» передаётся в system context Core;
- Core-owned build policy, её хранение и native policy panel;
- durable recovery foundation для длительных запусков и reconciliation.

## Следующий этап

1. Files, Editor, Git и controlled Terminal;
2. leases/reconciliation и расширенный diff/command preview в approval UI;
3. permission policy rules с glob-областями и повторной проверкой approval-пути;
4. Credential Manager/DPAPI, расширенный backup/restore и crash recovery UI;
5. дальнейшая проверка установочного UX на чистой Windows 11.

## Граница продукта

Пользовательский продукт ограничен `EvoHime-Setup.exe`, `EvoHime.exe`, локальным Core, supervisor и данными в профиле Windows. Исследовательские и экспериментальные каталоги не входят в установочный runtime.

Legacy web UI, HTTP server, browser launcher, PostgreSQL migrations и React-компоненты удалены из репозитория. Native UI и versioned named-pipe IPC — единственный пользовательский интерфейс и transport boundary.
