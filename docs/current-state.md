# EvoHime — текущее состояние

Обновлено: 2026-08-04.

## Native foundation

Готовы базовые части нового Windows-приложения:

- WinUI 3 shell и native solution;
- Rust `evohime-core` с model gateway, tool loop, cancellation и lifecycle events;
- SQLite schema bootstrap, transactional migration, backup и event replay;
- versioned named-pipe IPC между C# UI и Rust core;
- supervisor с single-instance mutex, Job Object и restart budget;
- структурированные JSONL-логи core/supervisor;
- native package builder и smoke-тест staging;
- WinUI compatibility/smoke tests.

Последние проверенные результаты: core 9/9, desktop IPC 7/7, local storage 4/4, WinUI 7/7, native package build — успешно.

## Product boundary

Старый browser/PostgreSQL runtime удалён из поддерживаемого запуска. `start-dev.ps1` собирает и запускает только native package через supervisor. Веб-клиент и старые setup/codegen scripts больше не являются частью репозитория.

## Следующие шаги

1. Завершить удаление старых server/launcher/worker workspace members.
2. Добавить native project picker, reconnect UI и tray/notifications.
3. Добавить provider settings, credential storage и backup/restore UI.
4. Собрать MSIX/portable release с update/rollback.
