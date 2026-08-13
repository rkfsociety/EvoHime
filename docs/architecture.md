# EvoHime — native Windows architecture

Статус: поддерживаемая архитектура продукта. Фактическое состояние реализации и ближайшие задачи см. в [`current-state.md`](current-state.md) и [`development-plan.md`](development-plan.md).

EvoHime — локальное Windows-приложение.
Пользовательское короткое имя агента — «Ева».

```text
EvoHime.exe               WinUI 3 UI (пользовательский запуск)
        │ desktop-ipc-v1 / named pipe
evohime-core.exe          agent loop, model gateway, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, restart, JSONL diagnostics
        │
evohime-transaction.exe   transactional update worker
```

UI не выполняет shell-команды и не открывает базу. Core владеет workspace, инструментами, моделью и локальным состоянием. Supervisor запускает core в Job Object и завершает дочернее дерево при остановке.

## IPC

Контракт находится в `crates/desktop-ipc/proto/evohime.desktop.proto`.

- major-версия несовместима, minor-расширения совместимы;
- фреймы ограничены 4 MiB;
- события имеют монотонный `sequence_id`;
- UI может запросить replay после последнего sequence ID;
- cancellation передаётся отдельной командой `StopTask`.

## Данные, диагностика и восстановление

SQLite находится в `%LOCALAPPDATA%\EvoHime` либо в `EVOHIME_DATA_DIR`. Миграции выполняются транзакционно; перед изменением схемы создаётся `.db.bak`. Журнал событий экспортируется в JSONL. Логи core и supervisor пишутся в `%LOCALAPPDATA%\EvoHime\logs`. Permission-правила читаются из `%LOCALAPPDATA%\EvoHime\permissions.json` как упорядоченный JSON-массив PolicyRule: побеждает последнее совпавшее правило, отсутствующий или пустой файл означает встроенный набор, пустой массив `[]` означает осознанное отключение правил. Обновление использует отдельный transaction worker, backup компонентов и recovery незавершённой транзакции перед запуском Core.

## Packaging и запуск

```powershell
.\scripts\build-windows-native.ps1
```

Для разработки используется `start-dev.ps1`. Для пользователя GitHub Actions собирает единственный `EvoHime-Setup.exe`. Установщик размещает внутренние `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`, `evohime-transaction.exe` и manifest в каталоге приложения и создаёт ровно один ярлык `EvoHime` на рабочем столе.

Пакет x64 предназначен для Windows 10 2004+ и Windows 11 и содержит только native runtime и его локальные компоненты.

Безопасностные ограничения вынесены в [`../SECURITY.md`](../SECURITY.md). Рабочие планы находятся в [`plans/`](plans/) и не являются источником фактического статуса.
