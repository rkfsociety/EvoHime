# Native Windows Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a stable native Windows application composed of WinUI 3, a Rust agent core, a supervisor, SQLite persistence and versioned named-pipe IPC.

**Architecture:** `EvoHime.Desktop` is a native WinUI 3 presentation process. `evohime-core` owns the agent loop, tools, approvals, memory and SQLite; `evohime-supervisor` owns startup, single-instance coordination, process containment, logs, updates and recovery. The desktop client never accesses files or the database directly.

**Tech Stack:** WinUI 3/C#; Rust; Tokio; protobuf; Windows named pipes; SQLite through `sqlx`; Windows Job Objects; MSTest; Cargo tests; Windows CI and MSIX packaging.

## Execution status

Tasks 1–6 (native skeleton, IPC, SQLite, supervisor, Core integration and shell) выполнены. Approval round-trip из Task 5/6 завершён коммитом `87c5b39` и проверен тестами. Task 7 — текущий следующий блок: Files, Editor, Git, controlled Terminal и approval preview/diff surfaces. Оставшиеся checkbox-пункты ниже — исходный пошаговый чек-лист спецификации; актуальная сводка статусов находится здесь и в `docs/development-plan.md`.

## Global Constraints

- The shipped product UI is WinUI 3 and the runtime is native Windows.
- The first supported target is Windows 11 22H2 or newer, x64.
- One UI and one core instance are allowed per Windows user data directory.
- UI-to-core communication uses a versioned named-pipe protocol with replayable sequence IDs.
- SQLite is the local database and the installer contains only the native runtime components.
- Core owns workspace access, shell execution, Git, approvals, memory and all persistence.
- Secrets use Windows Credential Manager/DPAPI and are excluded from logs, events and diagnostics by default.
- Every implementation task follows RED → GREEN → REFACTOR and ends with a focused test run and task-only commit.
- When Cargo workspace members or CI expectations change, `.github/workflows/rust.yml` is updated in the same task.

---

## File and component map

| Component | Files | Responsibility |
| --- | --- | --- |
| Native desktop | `desktop/EvoHime.Desktop/`, `desktop/EvoHime.Tests/` | WinUI views, view models, IPC client, tray, notifications, native dialogs |
| IPC contract | `crates/desktop-ipc/proto/evohime.desktop.proto`, `crates/desktop-ipc/src/` | Protobuf messages, version checks, envelopes and named-pipe framing |
| Core | `crates/evohime-core/src/` | Agent task commands, event journal, tool/permission integration and readiness |
| Local storage | `crates/evohime-local-storage/src/`, `crates/evohime-local-storage/migrations/` | SQLite connection, migrations, checkpoints, retention and diagnostics metadata |
| Supervisor | `crates/evohime-supervisor/src/` | Mutex, core process lifecycle, Job Objects, logs, recovery and shutdown |
| Packaging | `installer/EvoHime.iss`, `scripts/build-windows-native.ps1`, `crates/evohime-updater/` | Native package metadata, bundled binaries, upgrade backup and rollback |
| CI | `.github/workflows/windows-native.yml`, `.github/workflows/rust.yml` | Rust, C#, IPC compatibility, Windows smoke and package checks |

### Task 1: Toolchain and native solution skeleton

**Files:**
- Create: `desktop/EvoHime.sln`
- Create: `desktop/EvoHime.Desktop/EvoHime.Desktop.csproj`
- Create: `desktop/EvoHime.Desktop/App.xaml`, `desktop/EvoHime.Desktop/App.xaml.cs`
- Create: `desktop/EvoHime.Desktop/MainWindow.xaml`, `desktop/EvoHime.Desktop/MainWindow.xaml.cs`
- Create: `desktop/EvoHime.Tests/EvoHime.Tests.csproj`
- Create: `crates/evohime-core/Cargo.toml`, `crates/evohime-core/src/lib.rs`
- Create: `crates/evohime-local-storage/Cargo.toml`, `crates/evohime-local-storage/src/lib.rs`
- Create: `crates/evohime-supervisor/Cargo.toml`, `crates/evohime-supervisor/src/main.rs`
- Modify: `Cargo.toml`, `.github/workflows/rust.yml`
- Test: `desktop/EvoHime.Tests/SmokeTests.cs`, `crates/evohime-core/src/lib.rs`

**Interfaces:**
- Produces a buildable WinUI solution and three Rust package boundaries.
- `evohime_core::CoreVersion::current() -> &'static str` is the first core health identity.

- [ ] **Step 1: Write the failing build smoke tests.**

```csharp
[TestClass]
public sealed class SmokeTests
{
    [TestMethod]
    public void DesktopAssemblyExposesExpectedAppType()
    {
        Assert.IsNotNull(typeof(App));
    }
}
```

```rust
#[test]
fn core_exposes_version() {
    assert!(!CoreVersion::current().is_empty());
}
```

- [ ] **Step 2: Run the tests before implementation.**

Run: `dotnet test desktop/EvoHime.Tests/EvoHime.Tests.csproj` and `cargo test -p evohime-core core_exposes_version`

Expected: the commands fail because the native solution and core crate do not exist.

- [ ] **Step 3: Install or configure the required Windows App SDK/.NET workload if the Windows build host lacks it, then create the projects and add the Rust members.**

- [ ] **Step 4: Implement the empty WinUI app, `CoreVersion`, and a no-op core executable entrypoint without adding agent behavior.**

- [ ] **Step 5: Run `dotnet test desktop/EvoHime.Tests/EvoHime.Tests.csproj` and `cargo test -p evohime-core`.**

Expected: both pass on the supported Windows build host.

- [ ] **Step 6: Commit.**

```powershell
git add desktop Cargo.toml crates/evohime-core crates/evohime-local-storage crates/evohime-supervisor .github/workflows/rust.yml
git commit -m "feat: add native Windows solution skeleton"
```

### Task 2: Versioned protobuf IPC and named-pipe transport

**Files:**
- Create: `crates/desktop-ipc/proto/evohime.desktop.proto`
- Create: `crates/desktop-ipc/build.rs`, `crates/desktop-ipc/src/lib.rs`, `crates/desktop-ipc/src/transport.rs`
- Create: `crates/desktop-ipc/tests/compatibility.rs`, `crates/desktop-ipc/tests/fixtures/`
- Create: `desktop/EvoHime.Desktop/Services/CoreIpcClient.cs`
- Create: `desktop/EvoHime.Tests/IpcCompatibilityTests.cs`
- Modify: `Cargo.toml`, `desktop/EvoHime.Desktop/EvoHime.Desktop.csproj`

**Interfaces:**
- `CommandEnvelope { protocol_major, protocol_minor, request_id, command }`
- `EventEnvelope { protocol_major, protocol_minor, sequence_id, task_id, event }`
- `CoreIpcClient.ConnectAsync(CancellationToken)`, `SendAsync(CommandEnvelope)`, `ReplayAsync(ulong afterSequence)`
- `ProtocolVersion.IsCompatible(peer)` accepts additive minor versions and rejects different major versions.

- [ ] **Step 1: Add golden fixtures and compatibility tests first.** Test current/current, current/previous minor, unknown additive fields, major mismatch, malformed frame and replay ordering.
- [ ] **Step 2: Run `cargo test -p desktop-ipc` and `dotnet test desktop/EvoHime.Tests --filter FullyQualifiedName~IpcCompatibilityTests`.** Expected: fail because the contract and transport are absent.
- [ ] **Step 3: Define the `.proto` contract with handshake, commands, events, errors, cancellation and migration progress; generate Rust and C# types from the same source.**
- [ ] **Step 4: Implement length-delimited named-pipe framing with bounded frame size, request IDs and explicit protocol errors.**
- [ ] **Step 5: Implement the C# client with reconnect, handshake, sequence replay and cancellation.**
- [ ] **Step 6: Run both test suites and verify the C# client consumes the committed Rust fixture bytes.**
- [ ] **Step 7: Commit.**

```powershell
git add crates/desktop-ipc desktop/EvoHime.Desktop/Services desktop/EvoHime.Tests/IpcCompatibilityTests.cs
git commit -m "feat: add versioned named-pipe IPC"
```

### Task 3: SQLite storage, migrations and event journal

**Files:**
- Create: `crates/evohime-local-storage/src/connection.rs`, `migrations.rs`, `event_journal.rs`, `retention.rs`, `diagnostics.rs`
- Create: `crates/evohime-local-storage/migrations/0001_initial.sql`, `0002_event_indexes.sql`
- Create: `crates/evohime-local-storage/tests/migration_tests.rs`, `event_journal_tests.rs`, `retention_tests.rs`
- Modify: `crates/evohime-local-storage/Cargo.toml`

**Interfaces:**
- `LocalStore::open(path) -> Result<LocalStore>`
- `LocalStore::migrate(reporter) -> Result<MigrationReport>`
- `append_event(EventRecord) -> Result<u64>`
- `replay_after(task_id, sequence) -> Result<Vec<EventRecord>>`
- `compact_completed(before) -> Result<CompactionReport>`

- [ ] **Step 1: Write tests for fresh migration, interrupted resumable migration, sequence monotonicity, replay ordering, indexed queries and active-task retention.**
- [ ] **Step 2: Run `cargo test -p evohime-local-storage` and observe failure because SQLite storage is absent.**
- [ ] **Step 3: Implement SQLite WAL connection setup, migration metadata, idempotent checkpoints and progress callbacks.**
- [ ] **Step 4: Implement projects, tasks, event journal, approval records and diagnostics metadata with indexes on `(task_id, sequence_id)`, status and timestamps.**
- [ ] **Step 5: Implement checkpoint-based compaction that never removes active-task events and supports export before deletion.**
- [ ] **Step 6: Run the focused suite, including a temporary database and forced migration interruption.**
- [ ] **Step 7: Commit.**

```powershell
git add crates/evohime-local-storage
git commit -m "feat: add SQLite local storage and event journal"
```

### Task 4: Supervisor, single-instance policy and Windows process containment

**Files:**
- Create: `crates/evohime-supervisor/src/mutex.rs`, `process_tree.rs`, `logging.rs`, `readiness.rs`, `diagnostics.rs`
- Create: `crates/evohime-supervisor/tests/single_instance_windows.rs`, `process_tree_windows.rs`
- Create: `desktop/EvoHime.Desktop/Services/SupervisorClient.cs`
- Modify: `crates/evohime-supervisor/src/main.rs`, `.github/workflows/rust.yml`

**Interfaces:**
- `SupervisorCommand::OpenProject(PathBuf) | Focus | Shutdown | ExportDiagnostics`
- `SupervisorState::{Starting, Migrating, Ready, Degraded, Stopping, Failed}`
- `SupervisorClient.StartAsync(project)`, `ForwardToPrimaryAsync(command)`, `StopAsync()`

- [ ] **Step 1: Write Windows tests for second-launch forwarding, stale mutex owner recovery, child-process termination and readiness timeout.**
- [ ] **Step 2: Run `cargo test -p evohime-supervisor` on Windows and verify the tests fail before the implementation exists.**
- [ ] **Step 3: Implement a per-user named mutex and forwarding pipe; the secondary process sends its open request, focuses the primary UI and exits.**
- [ ] **Step 4: Implement Job Object assignment, hidden child startup, graceful shutdown and forced tree termination after timeout.**
- [ ] **Step 5: Implement rolling JSONL logs under `%LOCALAPPDATA%\\EvoHime\\logs` and fatal lifecycle entries in Windows Event Log.**
- [ ] **Step 6: Implement readiness handshake and states for startup, migration progress, degraded core and recovery.**
- [ ] **Step 7: Run Windows integration tests and verify no child process remains after supervisor exit.**
- [ ] **Step 8: Commit.**

```powershell
git add crates/evohime-supervisor desktop/EvoHime.Desktop/Services/SupervisorClient.cs .github/workflows/rust.yml
git commit -m "feat: add Windows supervisor and single-instance lifecycle"
```

### Task 5: Rust core command loop and agent integration

**Files:**
- Create: `crates/evohime-core/src/core.rs`, `commands.rs`, `events.rs`, `readiness.rs`, `workspace.rs`
- Create: `crates/evohime-core/tests/core_commands.rs`, `reconnect_replay.rs`
- Modify: `crates/agent-runtime/`, `crates/tool-runtime/`, `crates/permissions/`, `crates/model-gateway/`

**Interfaces:**
- `Core::start(config) -> Result<CoreHandle>`
- `CoreHandle::dispatch(CommandEnvelope) -> Result<()>`
- `CoreHandle::snapshot() -> Result<CoreSnapshot>`
- `CoreHandle::replay(task_id, after_sequence) -> Result<Vec<EventEnvelope>>`
- `CoreEventSink::publish(EventEnvelope)`

- [ ] **Step 1: Write tests for project opening, task creation, streamed deltas, cancellation, approval pause/resume, core restart replay and model failure.**
- [ ] **Step 2: Run `cargo test -p evohime-core` and confirm the new behavior fails.**
- [ ] **Step 3: Move the agent loop behind `CoreHandle` without exposing UI-specific types.**
- [ ] **Step 4: Map existing tool/protocol events to the desktop IPC event envelope and persist each event before publishing it.**
- [ ] **Step 5: Add workspace path validation, output/time limits and cancellation propagation through the tool runtime.**
- [ ] **Step 6: Run focused core tests plus existing agent-runtime and tool-runtime suites.**
- [ ] **Step 7: Commit.**

```powershell
git add crates/evohime-core crates/agent-runtime crates/tool-runtime crates/permissions crates/model-gateway
git commit -m "feat: expose agent runtime through native core commands"
```

### Task 6: Native shell and state-driven task UI

**Files:**
- Create: `desktop/EvoHime.Desktop/Views/ProjectPickerPage.xaml`, `TaskWorkspacePage.xaml`, `SettingsPage.xaml`
- Create: `desktop/EvoHime.Desktop/ViewModels/ProjectPickerViewModel.cs`, `TaskWorkspaceViewModel.cs`, `TaskStateReducer.cs`
- Create: `desktop/EvoHime.Desktop/Controls/TaskTimeline.xaml`, `ChatView.xaml`, `TaskListView.xaml`
- Create: `desktop/EvoHime.Tests/TaskStateReducerTests.cs`, `ReconnectReplayTests.cs`
- Modify: `desktop/EvoHime.Desktop/MainWindow.xaml`, `App.xaml.cs`

**Interfaces:**
- `TaskStateReducer.Apply(TaskState, EventEnvelope) -> TaskState`
- `TaskWorkspaceViewModel.SendMessageAsync(string)`, `CancelTaskAsync(Guid)`, `ReconnectAsync()`
- `ProjectPickerViewModel.OpenProjectAsync(string path)`

- [ ] **Step 1: Write reducer tests for event ordering, duplicate sequence IDs, reconnect replay, task failure and approval state.**
- [ ] **Step 2: Run the C# unit tests and confirm they fail before the reducer exists.**
- [ ] **Step 3: Implement the immutable state reducer; stale or duplicate events must be ignored without throwing.**
- [ ] **Step 4: Implement native pages and bind only to reducer state and IPC commands.**
- [ ] **Step 5: Add tray, focus-on-forwarded-launch and Windows notifications for approval/completion/failure.**
- [ ] **Step 6: Run C# unit tests and a Windows UI smoke test that opens a project and receives a synthetic task event.**
- [ ] **Step 7: Commit.**

```powershell
git add desktop/EvoHime.Desktop desktop/EvoHime.Tests
git commit -m "feat: add native task workspace shell"
```

### Task 7: Developer workflow surfaces

**Files:**
- Create: `desktop/EvoHime.Desktop/Views/ApprovalPage.xaml`, `DiffPage.xaml`, `TerminalPage.xaml`, `FilesPage.xaml`, `GitPage.xaml`
- Create: `desktop/EvoHime.Desktop/Services/NativeFileDialogService.cs`, `CredentialStore.cs`, `DiagnosticsExportService.cs`
- Create: `desktop/EvoHime.Tests/ApprovalPreviewTests.cs`, `CredentialStoreTests.cs`, `DiagnosticsExportTests.cs`
- Modify: `crates/evohime-core/src/commands.rs`, `crates/evohime-core/src/events.rs`

- [ ] **Step 1: Write tests for exact approval previews, denial/apply-once semantics, secret redaction in diagnostics and credential round trips through a test vault.**
- [ ] **Step 2: Run focused C# and Rust tests and confirm the new surfaces fail.**
- [ ] **Step 3: Implement approval/diff/terminal/files/Git commands through the core; no UI control may execute a shell command directly.**
- [ ] **Step 4: Implement DPAPI/Credential Manager storage and redacted diagnostic archive creation.**
- [ ] **Step 5: Add bounded terminal output, cancellation and file path validation tests.**
- [ ] **Step 6: Run the full native workflow smoke test against a temporary Git repository.**
- [ ] **Step 7: Commit.**

```powershell
git add desktop/EvoHime.Desktop desktop/EvoHime.Tests crates/evohime-core
git commit -m "feat: add native developer workflow surfaces"
```

### Task 8: Packaging, updates and recovery

**Files:**
- Create: `desktop/EvoHime.Package/EvoHime.Package.wapproj`, `Package.appxmanifest`
- Create: `desktop/EvoHime.Desktop/Services/UpdateService.cs`
- Create: `crates/evohime-supervisor/src/backup.rs`, `update.rs`
- Create: `desktop/EvoHime.Tests/UpgradeRecoveryTests.cs`
- Modify: `installer/EvoHime.iss`, `.github/workflows/rust.yml`, `README.md`

- [ ] **Step 1: Write tests for backup-before-migration, interrupted upgrade rollback, package file completeness and diagnostics export.**
- [ ] **Step 2: Run packaging tests and confirm they fail before package metadata and recovery code exist.**
- [ ] **Step 3: Implement versioned package manifests, bundled core/supervisor binaries and first-launch data-directory setup.**
- [ ] **Step 4: Implement backup, update staging, migration gate and rollback marker before replacing binaries.**
- [ ] **Step 5: Implement crash-safe startup recovery and a user-visible degraded state when the core cannot become ready.**
- [ ] **Step 6: Build the MSIX/package on Windows and install it into a clean test profile.**
- [ ] **Step 7: Commit.**

```powershell
git add installer/EvoHime.iss desktop/EvoHime.Desktop/Services/UpdateService.cs crates/evohime-supervisor crates/evohime-updater .github/workflows/rust.yml README.md
git commit -m "feat: package native Windows app with recovery"
```

### Task 9: Finalize native CI and release packaging

**Files:**
- Verify that the installer contains only the native runtime components after parity checks
- Modify: `Cargo.toml`, `start-dev.ps1`, `AGENTS.md`, `docs/current-state.md`, `docs/roadmap.md`, `.github/workflows/rust.yml`
- Create: `docs/native-windows-development.md`, `scripts/build-windows-native.ps1`

- [ ] **Step 1: Add a repository guard test that fails if the shipped product contains an unsupported runtime entrypoint or launches outside the native client.**
- [ ] **Step 2: Run the guard before cleanup and record the expected failures from the old architecture.**
- [ ] **Step 3: Finalize native build/startup paths only after native workflow tests pass.**
- [ ] **Step 4: Update project instructions, roadmap, README and CI to make the native Windows app the only supported product.**
- [ ] **Step 5: Run `cargo fmt --all -- --check`, `cargo test --workspace`, `dotnet test desktop/EvoHime.Tests`, the native packaging build and the repository guard on Windows.**
- [ ] **Step 6: Remove Rust target artifacts with `cargo clean` after verification, inspect `git diff --check`, and verify only task files are modified.**
- [ ] **Step 7: Commit.**

```powershell
git add -A
git commit -m "feat: make native Windows app the only supported client"
```

## Verification gates

Each phase must pass its focused tests before the next phase starts. The final gate requires a clean Windows test profile, a temporary Git repository, a forced core restart during a streamed task, an approval deny/apply-once cycle, a cancelled shell descendant check, a migration interruption/retry, diagnostics redaction inspection and a clean install/upgrade/rollback cycle.

## Plan self-review

- IPC compatibility, replay, request IDs and explicit errors are covered by Task 2.
- Long migration progress, backup and resumability are covered by Tasks 3 and 8.
- Single-instance behavior, Job Objects, logs and recovery are covered by Task 4.
- SQLite indexes, compaction and export are covered by Task 3.
- Credential protection and optional master-password deferral are covered by Task 7.
- Native UI, Core separation and release packaging are covered by Tasks 5, 6 and 9.
- No task depends on a branch or worktree; implementation stays on the current `main` branch.
