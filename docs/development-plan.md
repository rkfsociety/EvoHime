# План разработки EvoHime Native

## Цель

Создать стабильный локальный Windows AI-agent без браузерной панели и обязательных внешних сервисов. Пользователь запускает desktop app, выбирает workspace, запускает задачу и получает поток событий через named pipe.

Первая версия клиента: `0.0.0001`.

## Стек

| Слой | Технология |
| --- | --- |
| UI | C# + WinUI 3 |
| Core | Rust |
| IPC | versioned protobuf over Windows named pipes |
| Storage | SQLite + transactional migrations |
| Lifecycle | Rust supervisor + mutex + Job Object |
| Diagnostics | JSONL logs + replayable event journal |
| Packaging | portable x64 package; MSIX позже |

## Этапы

1. Foundation — solution, core, SQLite, IPC, logging. Завершён.
2. Native shell — task workspace, replay/reconnect, tray and notifications. Завершён.
3. Agent workflow — streaming, approvals, cancellation, checkpoints and diff review. В работе: streaming, cancellation и approval round-trip завершены; checkpoints и diff review впереди.
4. Developer tools — files, editor, Git and controlled terminal. Следующий этап.
5. Product hardening — credentials, backup/restore, installer, update and crash recovery. Запланирован.
6. Cleanup — старый web/PostgreSQL/server runtime удалён из поддерживаемого workspace и CI; архивные исходники удаляются отдельным безопасным проходом. В работе.

## Текущий статус native-перехода

| Блок | Статус | Последнее подтверждение |
| --- | --- | --- |
| Foundation: Core, SQLite, IPC, supervisor, diagnostics | ✅ Завершён | `e270efd`–`463e11b` |
| Package, smoke build, CI и отказ от web runtime | ✅ Завершён | `fb5e00e`–`8b84ad9` |
| Workspace, persistence, tray, notifications, replay | ✅ Завершён | `a43aaac`–`0246f05` |
| Approval round-trip через native IPC | ✅ Завершён | `87c5b39` |
| Files, Editor, Git, Terminal | ⬜ Следующий этап | — |
| Credentials, backup/restore, update/MSIX | ⬜ Запланирован | — |

## Acceptance criteria

- запуск с ярлыка не открывает браузер и консоль;
- UI и core эволюционируют независимо через IPC versioning;
- перезапуск core не теряет завершённые события;
- отмена задачи завершает дочерние процессы;
- опасные операции требуют approval и показывают preview;
- обновление восстанавливает данные из pre-upgrade backup;
- core tests работают без UI-сессии, WinUI smoke — на Windows CI.
