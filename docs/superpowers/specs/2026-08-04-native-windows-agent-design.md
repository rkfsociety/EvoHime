# EvoHime Native Windows Agent — Design Specification

> Статус: согласовано пользователем 2026-08-04.

## Цель

Переписать EvoHime как стабильное локальное Windows-приложение для работы с coding agents. Веб-панель, браузерный клиент, PostgreSQL и обратная совместимость со старой архитектурой не являются ограничениями: это новый продукт, использующий текущий репозиторий как источник уже проверенных идей и алгоритмов.

## Не входит в цель

- web-панель и запуск интерфейса в браузере;
- Electron, Tauri и WebView как основа интерфейса;
- multi-tenant, SaaS, серверная регистрация пользователей;
- сохранение старого HTTP/WS API ради совместимости;
- PostgreSQL, Docker и обязательный Python worker в локальной установке;
- перенос всех старых frontend-компонентов без пересмотра UX.

## Архитектура

```text
EvoHime.exe              WinUI 3 native desktop UI
        │
        │ versioned named-pipe IPC
        ▼
evohime-core.exe         Rust agent runtime and local tools
        │
        ├── model gateway (HTTPS)
        ├── filesystem / shell / Git / MCP tools
        ├── permissions and approvals
        ├── task engine and event journal
        ├── memory and project index
        └── SQLite state.db
        │
        └── controlled child processes via Windows Job Objects

evohime-supervisor.exe   process lifecycle, health, update, recovery
```

### Desktop UI

WinUI 3 with C# is the native Windows presentation layer. UI owns navigation, window state, rendering, keyboard shortcuts, native dialogs, tray integration and notifications. UI does not access the workspace, execute commands, call model providers or mutate the database directly.

### Core

Rust remains the trusted application core. It owns agent orchestration, model adapters, tool execution, workspace boundaries, Git operations, memory, task state, approvals and all persistence. The core is independently testable without launching the desktop UI.

### Supervisor

The supervisor starts the core, waits for a readiness handshake, captures structured logs, restarts a crashed core only when the task policy allows it, and shuts down the process tree on exit. Child processes are attached to Windows Job Objects so terminal commands cannot survive an application shutdown.

## IPC protocol

The UI and core communicate through a versioned named pipe using length-delimited protobuf messages. Every command has a request ID. Every event has a monotonically increasing sequence ID and task ID. The protocol supports:

- handshake with protocol version and capabilities;
- command request/response;
- streamed task events;
- approval request/grant/deny;
- replay from a sequence ID after UI reconnect;
- explicit error codes and cancellation;
- graceful shutdown and health state.

The UI reconnects and requests missed events instead of assuming that an in-memory stream is complete. Protocol changes require compatibility tests between the UI client and core.

## Persistence

SQLite is the local database, stored below the per-user EvoHime data directory. WAL mode is enabled. Versioned migrations cover projects, tasks, messages, event journal, approvals, memory, settings and update state. The core is the only database client.

Sensitive provider credentials use Windows Credential Manager or DPAPI-backed storage; plaintext tokens are never written to SQLite, logs or event payloads.

Before destructive migrations or application upgrades, the supervisor creates a restorable backup. Backup/restore is a first-class command and is tested on a clean temporary data directory.

## Security and reliability

- Bind no server port for UI-to-core communication.
- Restrict named-pipe access to the current Windows user.
- Validate every workspace path in the core.
- Keep approvals in the core, not in the UI.
- Apply command timeouts, cancellation and output limits.
- Use Windows Job Objects for process-tree cleanup.
- Redact secrets from tracing and persisted events.
- Persist task checkpoints and event sequence IDs.
- Treat model output, memory and repository text as untrusted data.
- Recover the UI from a core restart without losing completed task events.

## User experience

The first stable shell contains:

1. project/repository picker;
2. task list and multiple task tabs;
3. streamed chat;
4. task plan and event timeline;
5. approval dialog with exact diff or command preview;
6. terminal output;
7. files, editor and Git diff;
8. settings, model credentials and permissions;
9. tray menu and Windows notifications.

Memory, plugins, scheduled tasks and multi-agent worktrees follow after the core shell is stable.

## Delivery phases

### Phase 1 — Runtime foundation

Create the native solution, supervisor, Rust core process, SQLite bootstrap, named-pipe handshake, structured logging and clean shutdown/recovery tests.

### Phase 2 — Native shell

Implement project selection, task list, window state, reconnect/replay, tray and native notifications. No old web frontend is embedded.

### Phase 3 — Agent workflow

Implement message streaming, task lifecycle, approvals, cancellation, checkpoints, diff review and terminal event rendering.

### Phase 4 — Developer tools

Implement files/editor, Git status/diff/actions, workspace isolation and controlled child-process execution.

### Phase 5 — Product hardening

Implement memory, project index, provider settings, backup/restore, installer, update, diagnostics and crash recovery.

### Phase 6 — Cleanup

Remove obsolete browser frontend, PostgreSQL-specific startup, legacy HTTP/WS assumptions and stale browser-only documentation. Update CI and project instructions to the native Windows architecture.

## Acceptance criteria

- App starts from a desktop shortcut without opening a browser or console window.
- A user can select a repository, start a task, receive streamed events and review the result.
- Core restart does not lose persisted task state or completed events.
- A cancelled task leaves no child shell process running.
- Protected operations require approval and show an accurate preview.
- App upgrade can restore data from the pre-upgrade backup.
- UI and core can evolve independently through the versioned IPC contract.
- Core tests run without a Windows UI session; UI smoke tests run on supported Windows environments.

