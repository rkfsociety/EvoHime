# План разработки EvoHime Native

## Цель

Создать стабильный локальный Windows AI-agent. Пользователь запускает desktop app, выбирает workspace, запускает задачу и получает поток событий через named pipe.

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
3. Agent workflow — streaming, approvals, cancellation, checkpoints and diff review. В работе: streaming, cancellation, approval round-trip, bounded Build recovery checkpoints и Core-owned project policy завершены; leases/reconciliation и расширенный diff review продолжаются.
4. Developer tools — files, editor, Git and controlled terminal. Следующий этап.
5. Product hardening — credentials, backup/restore, installer, update and crash recovery. В работе.
6. Release cleanup — единый installer, retention релизов, диагностика и чистый native CI. В работе.

## Текущий статус native-перехода

Legacy web UI, browser launcher, HTTP server и PostgreSQL persistence удалены. Дальнейшая разработка выполняется только для WinUI 3 + Rust Core + SQLite + named-pipe IPC.

| Блок | Статус | Последнее подтверждение |
| --- | --- | --- |
| Foundation: Core, SQLite, IPC, supervisor, diagnostics | ✅ Завершён | `e270efd`–`463e11b` |
| Package, smoke build, CI и единый installer | ✅ Завершён | `9b3430c` |
| Workspace, persistence, tray, notifications, replay | ✅ Завершён | `a43aaac`–`0246f05` |
| Approval round-trip через native IPC | ✅ Завершён | `87c5b39` |
| Автообновление, SHA-256 verification, upgrade smoke и rollback recovery | ✅ Завершён | `edaa8ec` |
| Files, Editor, Git, Terminal | ⬜ Следующий этап | — |
| Credentials, расширенный backup/restore, update/MSIX | ⬜ Запланирован | — |

## Acceptance criteria

- запуск с ярлыка не открывает браузер и консоль;
- UI и core эволюционируют независимо через IPC versioning;
- перезапуск core не теряет завершённые события;
- отмена задачи завершает дочерние процессы;
- опасные операции требуют approval и показывают preview;
- обновление восстанавливает компоненты из pre-upgrade backup при ошибке и после аварийного завершения;
- core tests работают без UI-сессии, WinUI smoke — на Windows CI.
