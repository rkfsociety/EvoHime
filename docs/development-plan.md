# План разработки EvoHime Native

Статус: исполняемый план текущего native-цикла. Для фактического состояния используйте [`current-state.md`](current-state.md), для долгосрочных направлений — [`roadmap.md`](roadmap.md).

## Цель

Создать стабильный локальный Windows AI-agent. Пользователь запускает desktop app, выбирает workspace, запускает задачу и получает поток событий через named pipe.

Текущая версия клиента: `0.0.000032`.

## Стек

| Слой | Технология |
| --- | --- |
| UI | C# + WinUI 3 |
| Core | Rust |
| IPC | versioned protobuf over Windows named pipes |
| Storage | SQLite + transactional migrations |
| Lifecycle | Rust supervisor + mutex + Job Object |
| Diagnostics | JSONL logs + replayable event journal |
| Packaging | x64 native package + Inno Setup installer |

## Этапы

1. Foundation — solution, core, SQLite, IPC, logging. Завершён.
2. Native shell — task workspace, replay/reconnect, tray and notifications. Завершён.
3. Agent workflow — streaming, approvals, cancellation, checkpoints and diff review. В работе: streaming, cancellation, approval round-trip, bounded Build recovery checkpoints, Core-owned project policy и WinUI policy panel завершены; leases/reconciliation и расширенный diff review продолжаются.
4. Developer tools — Files, Editor и controlled Terminal вертикальные срезы завершены; Git status/diff доступны через bounded Core IPC.
5. Product hardening — credentials, backup/restore, installer, update and crash recovery. Частично завершён.
6. Release cleanup — единый installer, retention релизов, диагностика и чистый native CI. Основная часть завершена.

## Текущий статус native-перехода

Legacy web UI, browser launcher, HTTP server и PostgreSQL persistence удалены. Дальнейшая разработка выполняется только для WinUI 3 + Rust Core + SQLite + named-pipe IPC.

| Блок | Статус | Последнее подтверждение |
| --- | --- | --- |
| Foundation: Core, SQLite, IPC, supervisor, diagnostics | ✅ Завершён | `e270efd`–`463e11b` |
| Package, smoke build, CI и единый installer | ✅ Завершён | `9b3430c` |
| Workspace, persistence, tray, notifications, replay | ✅ Завершён | `a43aaac`–`0246f05` |
| Approval round-trip через native IPC | ✅ Завершён | `87c5b39` |
| Core build policy, persistence и native policy panel | ✅ Завершён | `6352321`, `a087042` |
| Durable run recovery foundation | ✅ Завершён | `cbb64e9` |
| Автообновление, SHA-256 verification, upgrade smoke и rollback recovery | ✅ Завершён | `edaa8ec` |
| Files | ✅ Завершён первый вертикальный срез: read-only tree/file preview через Core IPC | текущий native-срез |
| Editor | ✅ Завершён bounded bridge через Plan/Build approval из Files и Tasks | `2162f6e` |
| Git | ✅ Завершены bounded read-only status/diff через Core IPC и native Git page | `ea3f065`, `28d850d` |
| Terminal | ✅ Завершён bounded Core command: sandbox, approval retry, timeout и ограниченный output | текущий native-срез |
| Leases/reconciliation и расширенный diff review | 🟡 В работе | — |
| Permission policy rules и закрытие обходов approval | 🟡 В работе | `docs/plans/` |
| Credentials, расширенный backup/restore и crash recovery UI | ✅ Реализован Core/UI MVP | текущий native-срез; `cargo test`, WinUI и IPC tests |

## Acceptance criteria

- запуск с ярлыка не открывает браузер и консоль;
- UI и core эволюционируют независимо через IPC versioning;
- перезапуск core не теряет завершённые события;
- отмена задачи завершает дочерние процессы;
- опасные операции требуют approval и показывают preview;
- обновление восстанавливает компоненты из pre-upgrade backup при ошибке и после аварийного завершения;
- core tests работают без UI-сессии, WinUI smoke — на Windows CI.

Подробные незавершённые планы не дублируются здесь: они ведутся в `docs/plans/`. При расхождении этого плана с реализацией сначала обновляется статус на основании кода и тестов.
