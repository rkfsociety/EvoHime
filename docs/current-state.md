# EvoHime — текущее состояние

Обновлено: 2026-08-13.

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
- provider secrets хранятся в Credential Manager текущего Windows-пользователя; settings содержат только logical reference, предусмотрены миграция legacy и ручная ротация;
- Core-first SQLite backup/restore: Online Backup API, WAL checkpoint, DPAPI payload protection, checksum, preview, approval, progress, safety backup, rollback и redacted audit;
- filesystem.search исключает hard-default secret/auth paths, не следует symlink/reparse-обходам и не требует POSIX shell;
- shell blocklist расширен для Windows launcher/LOLBin семейств; recovery timeline различает `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL` и `FAILED`;
- native IPC/UI wiring для backup preview/restore и отображения storage progress/error.

## Следующий этап

1. leases/reconciliation и расширенный diff/command preview в approval UI;
2. повторная проверка installer/recovery UX на чистой Windows 11 22H2 reference VM;
3. informative ARM64/Insider compatibility runs.

## Граница продукта

Пользовательский продукт ограничен `EvoHime-Setup.exe`, `EvoHime.exe`, локальным Core, supervisor и данными в профиле Windows. Исследовательские и экспериментальные каталоги не входят в установочный runtime.

Legacy web UI, HTTP server, browser launcher, PostgreSQL migrations и React-компоненты удалены из репозитория. Native UI и versioned named-pipe IPC — единственный пользовательский интерфейс и transport boundary.
