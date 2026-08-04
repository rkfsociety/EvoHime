# EvoHime Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first polished native EvoHime shell with a distinctive dark workspace, left navigation, central chat/task area and a narrow pinnable workspace-summary widget.

**Architecture:** Keep WinUI 3 as the presentation layer and Rust Core as the owner of workspace, tasks, Git, tools and persistence. The shell consumes reducer state and sends commands only through the versioned named-pipe IPC. The first UI iteration uses focused views/components and shared resources; it must not grow more business logic inside `MainWindow`.

**Tech Stack:** C# WinUI 3, existing native IPC services, MSTest, Rust Core events, Windows native packaging and CI.

## Global Constraints

- Product remains a single native Windows client: `EvoHime-Setup.exe` installs one user-facing `EvoHime.exe` shortcut.
- Supported target remains Windows 11 22H2 or newer, x64.
- UI never accesses workspace files, Git, shell, Core storage or providers directly.
- UI commands and state updates use the existing versioned named-pipe IPC.
- The visual identity is original: graphite/night-workshop surfaces with violet and turquoise accents; do not copy another product's branding, icons or exact layout.
- Every visible section has a real Ready, Empty, Loading, Running, Degraded or Error state; no decorative dead buttons.
- Installed user versions are never used as a test target; launch checks use a separate staging package.
- Each task ends with focused tests, `git diff --check`, cleanup of generated `bin`, `obj`, `target` and staging artifacts, and a task-only commit.

---

### Task 1: Shell state model and navigation contract

**Files:**
- Create: `desktop/EvoHime.Desktop/Shell/ShellSection.cs`
- Create: `desktop/EvoHime.Desktop/Shell/ShellState.cs`
- Create: `desktop/EvoHime.Desktop/Shell/ShellStateReducer.cs`
- Create: `desktop/EvoHime.Tests/ShellStateReducerTests.cs`
- Modify: `desktop/EvoHime.Desktop/MainWindow.xaml.cs`

**Interfaces:**
- `ShellSection { NewChat, Pulse, Projects, Scheduled, Plugins, Settings }`
- `ShellStateReducer.Apply(ShellState state, ShellAction action) -> ShellState`
- `ShellAction.Navigate(ShellSection section)`, `SelectWorkspace(string path)`, `SetLoadState(...)`

- [ ] **Step 1: Write reducer tests** for initial section, navigation, selected workspace, empty/loading/degraded states and invalid repeated actions.
- [ ] **Step 2: Run `dotnet test desktop/EvoHime.Tests/EvoHime.Tests.csproj -p:Platform=x64 --filter FullyQualifiedName~ShellStateReducerTests`** and verify the new tests fail because the shell state types do not exist.
- [ ] **Step 3: Implement immutable shell state and reducer** with no filesystem, Git or IPC calls.
- [ ] **Step 4: Run the focused tests** and verify all reducer cases pass.
- [ ] **Step 5: Commit** with `git commit -m "feat: add native shell state reducer"`.

### Task 2: Shared visual resources and main layout

**Files:**
- Create: `desktop/EvoHime.Desktop/Resources/ThemeResources.xaml`
- Create: `desktop/EvoHime.Desktop/Resources/ControlStyles.xaml`
- Create: `desktop/EvoHime.Desktop/Controls/EvaIcon.cs`
- Create: `desktop/EvoHime.Desktop/Views/ShellFrame.cs`
- Modify: `desktop/EvoHime.Desktop/App.xaml`
- Modify: `desktop/EvoHime.Desktop/MainWindow.xaml.cs`
- Test: `desktop/EvoHime.Tests/SmokeTests.cs`

**Interfaces:**
- Resource keys: `ShellBackgroundBrush`, `SurfaceBrush`, `SurfaceActiveBrush`, `TextPrimaryBrush`, `TextMutedBrush`, `EvaVioletBrush`, `EvaTurquoiseBrush`, `ShellCornerRadius`.
- `ShellFrame.SetContent(UIElement content)` and `ShellFrame.SetActiveSection(ShellSection section)`.

- [ ] **Step 1: Add a startup smoke assertion** that the desktop assembly exposes the shell frame and its resource keys.
- [ ] **Step 2: Run the focused smoke test** and verify it fails before the resource/frame implementation exists.
- [ ] **Step 3: Implement the night-workshop resources** with compact spacing, keyboard-visible focus, disabled/hover/active states and no product-specific copied assets.
- [ ] **Step 4: Implement the three-zone frame**: left navigation host, central workspace host and top command strip; keep the summary widget outside the central content flow.
- [ ] **Step 5: Run `dotnet build desktop/EvoHime.Desktop/EvoHime.Desktop.csproj -p:Platform=x64 --nologo`** and the focused smoke tests.
- [ ] **Step 6: Commit** with `git commit -m "feat: add EvoHime night-workshop shell frame"`.

### Task 3: Left navigation and project/recent lists

**Files:**
- Create: `desktop/EvoHime.Desktop/Views/NavigationView.cs`
- Create: `desktop/EvoHime.Desktop/Views/ProjectListView.cs`
- Create: `desktop/EvoHime.Desktop/Views/RecentChatListView.cs`
- Create: `desktop/EvoHime.Desktop/Shell/NavigationPresenter.cs`
- Create: `desktop/EvoHime.Tests/NavigationPresenterTests.cs`
- Modify: `desktop/EvoHime.Desktop/Shell/ShellStateReducer.cs`

**Interfaces:**
- `NavigationPresenter.Build(ShellState state) -> NavigationSnapshot`
- `NavigationSnapshot.Sections`, `Projects`, `RecentChats`, `SelectedSection`, `SelectedWorkspace`
- Events: `SectionSelected`, `WorkspaceSelected`, `NewChatRequested`

- [ ] **Step 1: Test** active-section rendering, workspace selection, empty project list and recent-chat ordering.
- [ ] **Step 2: Run the focused tests** and confirm the presenter is absent/failing.
- [ ] **Step 3: Implement navigation** with `Новый чат`, `Пульс`, `Проекты`, `Запланировано`, `Плагины`, settings, projects and recent chats.
- [ ] **Step 4: Connect workspace selection** to the existing `WorkspaceSettings` and `CoreIpcClient` path without moving Core ownership into UI.
- [ ] **Step 5: Verify keyboard navigation, active state and empty states** in a Windows staging launch.
- [ ] **Step 6: Commit** with `git commit -m "feat: add EvoHime navigation and project list"`.

### Task 4: Central chat/workspace and task-state display

**Files:**
- Create: `desktop/EvoHime.Desktop/Views/ChatWorkspaceView.cs`
- Create: `desktop/EvoHime.Desktop/Views/ChatMessageView.cs`
- Create: `desktop/EvoHime.Desktop/Views/TaskStatusView.cs`
- Create: `desktop/EvoHime.Desktop/Views/EmptyWorkspaceView.cs`
- Create: `desktop/EvoHime.Tests/ChatWorkspaceViewModelTests.cs`
- Modify: `desktop/EvoHime.Desktop/Services/NativeShellState.cs`
- Modify: `desktop/EvoHime.Desktop/MainWindow.xaml.cs`

**Interfaces:**
- `ChatWorkspaceState { Messages, ActiveTask, Connection, InputText, CanSend, CanStop }`
- `ChatWorkspaceViewModel.SendAsync(string prompt, CancellationToken)`, `StopAsync(CancellationToken)`
- Existing native events remain the source of truth for task status and timeline.

- [ ] **Step 1: Test** empty workspace, message ordering, active task, Stop availability and degraded IPC state.
- [ ] **Step 2: Run tests** and verify the new view model is not implemented.
- [ ] **Step 3: Implement** welcome state, message list, multiline input, `Запустить`, `Stop`, task status and event timeline using existing IPC services.
- [ ] **Step 4: Add explicit states** for connecting, running, completed, failed, stopped and reconnecting.
- [ ] **Step 5: Run C# tests and Windows UI smoke** that opens the staging client, enters a test prompt and confirms the controls remain responsive.
- [ ] **Step 6: Commit** with `git commit -m "feat: add native chat workspace"`.

### Task 5: Narrow workspace-summary popover and pinning

**Files:**
- Create: `desktop/EvoHime.Desktop/Views/WorkspaceSummaryPopover.cs`
- Create: `desktop/EvoHime.Desktop/Views/WorkspaceSummaryWidget.cs`
- Create: `desktop/EvoHime.Desktop/Shell/PopoverController.cs`
- Create: `desktop/EvoHime.Tests/PopoverControllerTests.cs`
- Modify: `desktop/EvoHime.Desktop/Services/NativeShellState.cs`

**Interfaces:**
- `WorkspaceSummaryState { Changes, WorkspacePath, Branch, Processes, Sources, CanCommit, CanCompare }`
- `PopoverController.Open()`, `Close()`, `TogglePinned()`, `HandleEscape()`, `HandleOutsideClick()`
- `WorkspaceSummaryWidget.IsPinned` and fixed narrow width within the agreed desktop range.

- [ ] **Step 1: Test** open/close, Escape, outside click, pin/unpin and the invariant that pinned width remains narrow.
- [ ] **Step 2: Run the tests** and confirm the controller is absent/failing.
- [ ] **Step 3: Implement** the compact summary with changes, local workspace, branch, commit/send, compare branch, background processes and sources.
- [ ] **Step 4: Keep unavailable actions honest**: disabled with an explanation until their Core command exists; never fake a successful commit or comparison.
- [ ] **Step 5: Verify** popover behavior visually and by accessibility tree in a staging build.
- [ ] **Step 6: Commit** with `git commit -m "feat: add narrow workspace summary widget"`.

### Task 6: Real section screens and empty states

**Files:**
- Create: `desktop/EvoHime.Desktop/Views/PulseView.cs`
- Create: `desktop/EvoHime.Desktop/Views/ProjectsView.cs`
- Create: `desktop/EvoHime.Desktop/Views/ScheduledView.cs`
- Create: `desktop/EvoHime.Desktop/Views/PluginsView.cs`
- Create: `desktop/EvoHime.Desktop/Views/SettingsView.cs`
- Create: `desktop/EvoHime.Desktop/Views/SectionEmptyState.cs`
- Create: `desktop/EvoHime.Tests/SectionViewStateTests.cs`

- [ ] **Step 1: Test** Ready, Empty, Loading, Degraded and Error rendering for each section without external services.
- [ ] **Step 2: Implement** real containers and truthful empty states; each unavailable action explains what is required.
- [ ] **Step 3: Connect Pulse** to existing Core/supervisor status and event data.
- [ ] **Step 4: Connect Projects and Settings** to current workspace persistence and update/diagnostics services.
- [ ] **Step 5: Run focused tests and a navigation smoke test** through every first-version section.
- [ ] **Step 6: Commit** with `git commit -m "feat: add native shell section screens"`.

### Task 7: Shell integration, CI and release verification

**Files:**
- Modify: `desktop/EvoHime.Desktop/App.xaml.cs`
- Modify: `desktop/EvoHime.Desktop/MainWindow.xaml.cs`
- Modify: `desktop/EvoHime.Tests/SmokeTests.cs`
- Modify: `scripts/native-workflow.tests.ps1`
- Modify: `docs/current-state.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add the end-to-end shell smoke contract**: launch staging EXE, observe the main window, navigate sections, open the summary, pin/unpin it and close cleanly.
- [ ] **Step 2: Run the local native workflow smoke, C# tests and `git diff --check`**; fix all failures before packaging.
- [ ] **Step 3: Run the full Windows native package build** in a separate staging directory and verify the installed user version is untouched.
- [ ] **Step 4: Update current-state and roadmap** with the shell milestone and the next developer-workflow milestone.
- [ ] **Step 5: Remove `bin`, `obj`, `target` and staging artifacts** after verification.
- [ ] **Step 6: Commit** with `git commit -m "feat: complete EvoHime shell milestone"`.
- [ ] **Step 7: Push only the completed task commit** and wait for Rust, Windows, package and release checks.

## Verification gate

The shell milestone is complete only when the staging EXE opens a visible native window, every navigation item has a truthful state, the central workspace accepts a test prompt, the summary popover opens and closes correctly, pinning keeps it narrow, no child process remains after clean exit, Windows CI is green and the release workflow accepts the package.
