# EvoHime — native Windows roadmap

Актуальный roadmap описывает один локальный Windows-клиент Ева, распространяемый через `EvoHime-Setup.exe`. Пользователь запускает один ярлык `EvoHime`; внутренние Core и supervisor не являются отдельными продуктами.

## Текущая версия

`0.0.0001` — первый клиентский релиз.

## Завершено

| Блок | Статус | Подтверждение |
| --- | --- | --- |
| WinUI 3 shell и native solution | ✅ | `bb432fa` |
| Rust Core и SQLite event journal | ✅ | `93995bc`, `66e741e` |
| Versioned named-pipe IPC и replay | ✅ | `e0da370`, `463e11b` |
| Supervisor, mutex, Job Object и diagnostics | ✅ | `e0e0f75`, `a9018a8` |
| Workspace picker, persistence, tray, notifications | ✅ | `a43aaac`–`6991a11` |
| Native task timeline, cancellation и approval round-trip | ✅ | `0246f05`, `87c5b39` |
| Единый installer и CI build after checks | ✅ | `9b3430c` |
| Имя агента «Ева» и версия `0.0.0001` | ✅ | `775b20b` |
| Retention: только последний стабильный release/tag | ✅ | `dadcbf6` |
| Автообнаружение обновления, SHA-256 проверка, upgrade smoke и автоматический rollback | ✅ | `edaa8ec` |

## Ближайшая работа

### 1. Developer workflow

- Files: дерево workspace, открытие и безопасное чтение;
- Editor: native текстовый редактор с сохранением через Core;
- Git: status, diff, commit и безопасные операции;
- Terminal: controlled child process, поток stdout/stderr, timeout и Stop;
- approval preview для команд и изменений.

### 2. Product hardening

- Windows Credential Manager/DPAPI для provider keys;
- backup/restore SQLite и migration recovery;
- crash recovery и диагностика из UI;
- проверка upgrade path на чистой Windows 11 22H2+.

### 3. Native quality

- compatibility tests UI/Core для каждого изменения IPC;
- smoke installer на Windows CI;
- проверка single-instance и завершения Job Object;
- bounded logs, event replay и retention completed tasks;
- release только после зелёных Rust/WinUI/package checks.

## Release workflow

1. Push или pull request запускает проверки Rust, supervisor, package smoke и WinUI.
2. Job `build-native` стартует только после успешных проверок.
3. Собирается runtime в staging-каталог.
4. Inno Setup создаёт единственный `EvoHime-Setup.exe`.
5. Для tag `vX.Y.Z` создаётся GitHub Release.
6. Еженедельная retention-задача удаляет все versioned Releases/tags, кроме последнего.
