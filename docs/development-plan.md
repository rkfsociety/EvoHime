# План разработки EvoHime Native

## Цель

Создать стабильный локальный Windows AI-agent без браузерной панели и обязательных внешних сервисов. Пользователь запускает desktop app, выбирает workspace, запускает задачу и получает поток событий через named pipe.

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
2. Native shell — task workspace, replay/reconnect, tray and notifications. В работе.
3. Agent workflow — streaming, approvals, cancellation, checkpoints and diff review.
4. Developer tools — files, editor, Git and controlled terminal.
5. Product hardening — credentials, backup/restore, installer, update and crash recovery.
6. Cleanup — старый web/PostgreSQL/server runtime удаляется из workspace и CI. В работе.

## Acceptance criteria

- запуск с ярлыка не открывает браузер и консоль;
- UI и core эволюционируют независимо через IPC versioning;
- перезапуск core не теряет завершённые события;
- отмена задачи завершает дочерние процессы;
- опасные операции требуют approval и показывают preview;
- обновление восстанавливает данные из pre-upgrade backup;
- core tests работают без UI-сессии, WinUI smoke — на Windows CI.
