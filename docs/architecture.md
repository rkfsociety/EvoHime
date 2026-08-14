# EvoHime — Windows desktop architecture

Статус: текущая утверждённая архитектура продукта. Фактическое состояние реализации см. в [`current-state.md`](current-state.md).

EvoHime — локальное Windows-приложение.
Пользовательское короткое имя агента — «Ева».

```text
EvoHime.exe               Electron main + bundled renderer
        │ preload/contextBridge → desktop-ipc-v1 / named pipe
evohime-core.exe          agent loop, model gateway, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, restart, JSONL diagnostics
        │
evohime-transaction.exe   transactional update worker
```

Renderer не имеет node integration, не выполняет shell-команды и не открывает базу. Electron main ограничен окном, lifecycle, локальным состоянием оболочки и IPC adapter. Core владеет workspace, инструментами, моделью, секретами и локальным состоянием. Supervisor запускает core в Job Object и завершает дочернее дерево при остановке.

WinUI 3 больше не является пользовательской оболочкой пакета. Он сохранён как
временный compatibility runtime и oracle для совместимости IPC до отдельного
решения о его удалении.

## Оболочка

Renderer состоит из панели проектов и чатов, ленты диалога и инструментальных разделов.

| Поверхность | Назначение |
| --- | --- |
| `ProjectSidebar` | проекты (workspace) и чаты внутри проекта; аккаунт и вход в настройки внизу |
| `HomeScreen` | стартовый экран; первый запрос сам создаёт чат |
| `TaskTimeline` + `ActivityLine` + `transcript.ts` | ход задачи, свёрнутый в читаемую ленту; ответы агента рендерятся Markdown |
| `tool-names.ts` | русские подписи инструментов вместо служебных идентификаторов |
| `RepositoryBar` | ветка и счётчики изменений открытого репозитория |
| `ModelPicker` | выбор модели в чате; каталог разделён на free и paid |
| `ProviderForm` | единственная поверхность настроек провайдера (ключ, модель, base URL) |
| `RecoveryBanner` + `recovery-state.ts` | состояние восстановления, выведенное только из подтверждённых Core событий |
| `OperationsPanel` | read-only проекция memory-, child- и schedule-событий |
| `DeveloperTools`, `EditorPanel`, `TerminalPanel`, `SafetyPanel` | файлы и Git, редактор, ограниченный терминал, permission policy |

Бизнес-логики в renderer нет: он отображает состояние, полученное через IPC, и отправляет команды.

## IPC

Контракт находится в `crates/desktop-ipc/proto/evohime.desktop.proto`.

- major-версия несовместима, minor-расширения совместимы;
- фреймы ограничены 4 MiB;
- события имеют монотонный `sequence_id`;
- UI может запросить replay после последнего sequence ID;
- cancellation передаётся отдельной командой `StopTask`;
- `SelectModelRequest` меняет модель следующего запроса без перезапуска Core: gateway разрешает модель на каждый вызов, пустое значение возвращает модель маршрута;
- `CancelDatabaseOperation` кооперативно отменяет выполняющийся backup или restore.

Часть команд renderer не доходит до Core и обслуживается main-процессом: `workspace.*`, `chat.*`, `provider.*`, `identity.get`, `repository.get`. Это локальное состояние оболочки, а не права: Core заново проверяет capability, policy и approval для каждой команды, которая до него доходит.

## Данные, диагностика и восстановление

SQLite находится в `%LOCALAPPDATA%\EvoHime` либо в `EVOHIME_DATA_DIR`. Миграции выполняются транзакционно; перед изменением схемы создаётся `.db.bak`. Журнал событий экспортируется в JSONL. Логи core и supervisor пишутся в `%LOCALAPPDATA%\EvoHime\logs`. Permission-правила читаются из `%LOCALAPPDATA%\EvoHime\permissions.json` как упорядоченный JSON-массив PolicyRule: побеждает последнее совпавшее правило, отсутствующий или пустой файл означает встроенный набор, пустой массив `[]` означает осознанное отключение правил. Обновление использует отдельный transaction worker, backup компонентов и recovery незавершённой транзакции перед запуском Core.

Локальное состояние оболочки лежит рядом, в `%LOCALAPPDATA%\EvoHime\shell\`:

| Файл | Содержимое | Ограничения |
| --- | --- | --- |
| `workspaces.json` | список запомненных папок и последняя выбранная | нормализованные пути |
| `chats.json` | чаты, привязанные к workspace, и отправленные промпты | 100 чатов на workspace, 500 сообщений на чат, 4096 символов на промпт |
| `provider.json` | выбранный провайдер, модель, base URL и зашифрованный ключ | режим `600`, запись через временный файл и `rename` |

Повреждённый файл не роняет оболочку: он читается как пустой.

## Бюджет запуска

`evohime_core::run_policy` описывает неизменяемый snapshot политики одного запуска: `max_iterations`, `max_wall_clock_ms`, `max_tool_calls`, `max_tokens`, `max_cost_micros` и `approval_required`. Core проверяет счётчики перед отправкой эффекта; превышение любого из них останавливает запуск с `BudgetExceeded`. Renderer может показать snapshot, но не может поднять лимит в середине запуска.

`evohime_supervisor::pulse` сводит события расписаний в локальный digest. Dead-letter даёт `Failed`, пропуски и неуспехи — `Degraded`; успешный счётчик никогда не маскирует отказ.

## Ключ провайдера

Ключ вводится в `ProviderForm` и остаётся в main-процессе. Значение шифруется ОС через Electron `safeStorage` (DPAPI на Windows) и сохраняется в `provider.json`; renderer получает только summary с признаком `configured`. Core собирает model gateway из окружения при старте, поэтому сохранение ключа перезапускает supervisor вместе с Core, а pipe client переподключается к новой сессии. В окружение попадают только переменные выбранного провайдера, чтобы устаревший ключ второго не дошёл до gateway. Если ОС отказывается шифровать, ключ не записывается вовсе.

Base URL принимается только по `https` либо по `http` на loopback: ключ отправляется на этот адрес, и произвольный http-хост означал бы его утечку.

## Packaging и запуск

```powershell
.\scripts\build-windows-native.ps1
```

Для разработки используется `start-dev.ps1`; он читает `.env` по allow-list имён из `.env.example` и передаёт их только дочерним native-процессам. Для пользователя GitHub Actions собирает единственный `EvoHime-Setup.exe`. Установщик размещает внутренние `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`, `evohime-transaction.exe` и manifest в каталоге приложения и создаёт ровно один ярлык `EvoHime` на рабочем столе.

Пакет x64 предназначен для Windows 10 2004+ и Windows 11 и содержит bundled Electron runtime, Rust runtime и локальные компоненты; отдельная установка Node.js или браузера не требуется.

Безопасностные ограничения вынесены в [`../SECURITY.md`](../SECURITY.md).
