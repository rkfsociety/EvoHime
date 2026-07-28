# Installer Live Command Log Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Показывать в установщике общее и этапное время, безопасные команды и их живой вывод, не открывая дочерние консольные окна.

**Architecture:** Общий модуль `evohime-launcher::observed_command` запускает `tokio::process::Command`, на Windows всегда применяет `CREATE_NO_WINDOW`, одновременно читает `stdout/stderr` и публикует структурированные события. PostgreSQL и Windows-команды Installer'а получают совместимые observer-варианты API, а GUI преобразует события в журнал и независимо считает общее и этапное время.

**Tech Stack:** Rust 2021, Tokio process/io, Windows process creation flags, eframe/egui, GitHub Actions.

## Global Constraints

- Отображать `Всего HH:MM:SS · этап HH:MM:SS` и обновлять строку непрерывно.
- Показывать команды, живой `stdout/stderr`, результат и длительность.
- Не выводить пароль PostgreSQL, содержимое `pwfile`, DSN, API-токены или секретные переменные окружения.
- Не открывать отдельные консольные окна для `initdb.exe`, `pg_ctl.exe`, `icacls.exe` и PowerShell.
- Не добавлять интерактивный терминал, отмену установки или новый таймаут PostgreSQL.
- Не создавать ветку или worktree; работа выполняется в текущей `main`.
- Не выполнять локальную сборку или тесты; локально разрешены только форматирование и проверка diff.
- Push и GitHub-проверки выполняются только после отдельной прямой команды пользователя.

---

### Task 1: Наблюдаемый запуск дочерних процессов

**Files:**
- Create: `crates/launcher/src/observed_command.rs`
- Modify: `crates/launcher/src/lib.rs`
- Modify: `crates/launcher/Cargo.toml`

**Interfaces:**
- Produces: `CommandStream::{Stdout, Stderr}`.
- Produces: `CommandEvent::{Started, Output, Finished}`.
- Produces: `ObservedCommandResult { status, stdout, stderr, elapsed }`.
- Produces: `run_observed_command(command, safe_display, observer)`.

- [ ] **Step 1: Write failing unit tests**

Add tests to `observed_command.rs` that describe the public contract before the
implementation:

```rust
#[tokio::test]
async fn streams_stdout_stderr_and_terminal_status() {
    let mut command = tokio::process::Command::new("cmd");
    command.args([
        "/C",
        "(echo stdout-line) & (echo stderr-line 1>&2) & exit /b 7",
    ]);
    let mut events = Vec::new();

    let result = run_observed_command(
        command,
        "safe command".to_string(),
        |event| events.push(event),
    )
    .await
    .unwrap();

    assert!(matches!(
        events.first(),
        Some(CommandEvent::Started { display }) if display == "safe command"
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        CommandEvent::Output {
            stream: CommandStream::Stdout,
            line
        } if line == "stdout-line"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CommandEvent::Output {
            stream: CommandStream::Stderr,
            line
        } if line == "stderr-line"
    )));
    assert_eq!(result.status.code(), Some(7));
    assert!(matches!(
        events.last(),
        Some(CommandEvent::Finished { success: false, .. })
    ));
}

#[test]
fn windows_commands_use_create_no_window() {
    assert_eq!(WINDOWS_CREATION_FLAGS, 0x0800_0000);
}
```

The async test is guarded with `#[cfg(windows)]`; add a portable helper-process
test for non-Windows using `sh -c`.

- [ ] **Step 2: Record expected RED state without compiling locally**

Do not run Cargo locally. Confirm by source inspection that
`run_observed_command`, `CommandEvent`, and `WINDOWS_CREATION_FLAGS` do not yet
exist, then stage the tests together with the implementation in the same
eventual push. The user-mandated GitHub run is the executable verification.

- [ ] **Step 3: Implement the process runner**

Implement these exact public shapes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEvent {
    Started { display: String },
    Output { stream: CommandStream, line: String },
    Finished {
        success: bool,
        exit_code: Option<i32>,
        elapsed: Duration,
    },
}

pub struct ObservedCommandResult {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

pub async fn run_observed_command<F>(
    mut command: tokio::process::Command,
    safe_display: String,
    mut observer: F,
) -> io::Result<ObservedCommandResult>
where
    F: FnMut(CommandEvent),
```

Before `spawn`, set `stdout` and `stderr` to `Stdio::piped()`. On Windows call
`command.creation_flags(WINDOWS_CREATION_FLAGS)`, where
`WINDOWS_CREATION_FLAGS` is `0x0800_0000`. Read both streams with
`BufReader::lines()` inside `tokio::select!`, append every line to the matching
captured buffer, and emit it immediately. Wait for the child, calculate elapsed
time from `Instant`, then emit exactly one `Finished`.

The caller supplies `safe_display`; never derive it from the complete command or
environment.

- [ ] **Step 4: Export the module and enable Tokio line I/O**

Add `pub mod observed_command;` to `crates/launcher/src/lib.rs` and add
`"io-util"` to the existing Tokio feature list in `crates/launcher/Cargo.toml`.

- [ ] **Step 5: Format and commit**

Run only:

```powershell
cargo fmt --all
git diff --check
git add crates/launcher/Cargo.toml crates/launcher/src/lib.rs crates/launcher/src/observed_command.rs
git commit -m "feat(launcher): add hidden observed commands"
```

---

### Task 2: Подключение PostgreSQL и команд Installer'а

**Files:**
- Modify: `crates/launcher/src/postgres.rs`
- Modify: `crates/installer/src/icacls.rs`
- Modify: `crates/installer/src/shortcut.rs`
- Modify: `crates/installer/src/dirty_cleanup.rs`
- Modify: `crates/installer/src/lib.rs`

**Interfaces:**
- Consumes: `CommandEvent` and `run_observed_command` from Task 1.
- Produces: `initdb_observed`, `start_observed`, `stop_observed`.
- Produces: `restrict_to_current_user_observed`,
  `restore_deletable_permissions_observed`, and `create_shortcut_observed`.
- Produces: `clear_dirty_installation_safely_observed`.

- [ ] **Step 1: Write failing safe-display and observer tests**

Extend the PostgreSQL tests:

```rust
#[test]
fn initdb_display_contains_path_but_not_password() {
    let display = pg_tool_display(
        Path::new(r"C:\EvoHime\pg16\bin\initdb.exe"),
        &[
            "-D",
            r"C:\EvoHime\pg16\data",
            "--pwfile",
            r"C:\EvoHime\pg16\.initdb-pwfile.tmp",
        ],
    );

    assert!(display.contains("initdb.exe"));
    assert!(display.contains("--pwfile"));
    assert!(!display.contains("secret-password"));
}
```

Add a shortcut test that the safe display is the constant
`powershell.exe -NoProfile -NonInteractive -Command <создание ярлыка EvoHime>`
and does not contain the generated PowerShell script.

- [ ] **Step 2: Record expected RED state without compiling locally**

Confirm the observed function names are unresolved in the source. Do not run
Cargo locally.

- [ ] **Step 3: Route PostgreSQL through the observed runner**

Keep `initdb`, `start`, and `stop` source-compatible by delegating to observed
variants with a no-op observer:

```rust
pub async fn start_observed<F>(
    pg_bin_dir: &Path,
    data_dir: &Path,
    port: u16,
    observer: F,
) -> Result<(), PgError>
where
    F: FnMut(CommandEvent),
```

Add corresponding `initdb_observed` and `stop_observed`. Replace `.output()`
inside `run_pg_tool` with `run_observed_command`. Build the safe display only
from executable path and the explicit argument slice. Preserve
`PgError::CommandFailed` and use captured stderr exactly as before.

- [ ] **Step 4: Route `icacls` and PowerShell through the observed runner**

Add observer variants while retaining existing wrappers:

```rust
pub async fn restrict_to_current_user_observed<F>(
    dir: &Path,
    observer: &mut F,
) -> Result<(), IcaclsError>
where
    F: FnMut(CommandEvent);

pub async fn create_shortcut_observed<F>(
    shortcut_path: &Path,
    target_exe: &Path,
    observer: F,
) -> Result<(), ShortcutError>
where
    F: FnMut(CommandEvent);
```

For PowerShell pass the constant safe display; never show its generated script.
For `icacls`, show the path and permission switches because they contain no
secret. Preserve the existing error types and captured output.

- [ ] **Step 5: Expose command events during dirty-install cleanup**

Add `clear_dirty_installation_safely_observed(install_dir, progress, observer)`.
Use `stop_observed` and `restore_deletable_permissions_observed` within it.
Retain both existing cleanup functions as no-op-observer wrappers so existing
tests and callers remain compatible.

- [ ] **Step 6: Format and commit**

Run only:

```powershell
cargo fmt --all
git diff --check
git add crates/launcher/src/postgres.rs crates/installer/src/icacls.rs crates/installer/src/shortcut.rs crates/installer/src/dirty_cleanup.rs crates/installer/src/lib.rs
git commit -m "feat(installer): observe hidden setup commands"
```

---

### Task 3: Таймеры и живой журнал GUI

**Files:**
- Create: `crates/installer/src/timing.rs`
- Create: `crates/installer/src/log_safety.rs`
- Modify: `crates/installer/src/lib.rs`
- Modify: `crates/installer/src/main.rs`

**Interfaces:**
- Consumes: observed APIs and `CommandEvent` from Tasks 1–2.
- Produces: `InstallationTiming::{started, begin_stage, finish, elapsed}`.
- Produces: `format_elapsed(Duration) -> String`.
- Produces: `redact_command_event(event, secrets) -> CommandEvent`.

- [ ] **Step 1: Write failing deterministic timing tests**

Add to `timing.rs`:

```rust
#[test]
fn stage_elapsed_resets_without_resetting_total() {
    let start = Instant::now();
    let mut timing = InstallationTiming::started(start);
    timing.begin_stage(start + Duration::from_secs(65));

    let elapsed = timing.elapsed(start + Duration::from_secs(70));
    assert_eq!(elapsed.total, Duration::from_secs(70));
    assert_eq!(elapsed.stage, Duration::from_secs(5));
}

#[test]
fn finish_freezes_both_counters() {
    let start = Instant::now();
    let mut timing = InstallationTiming::started(start);
    timing.begin_stage(start + Duration::from_secs(3));
    timing.finish(start + Duration::from_secs(10));

    let elapsed = timing.elapsed(start + Duration::from_secs(99));
    assert_eq!(elapsed.total, Duration::from_secs(10));
    assert_eq!(elapsed.stage, Duration::from_secs(7));
}

#[test]
fn formats_hours_minutes_and_seconds() {
    assert_eq!(format_elapsed(Duration::from_secs(3_661)), "01:01:01");
}
```

Add to `log_safety.rs`:

```rust
#[test]
fn redacts_known_secret_from_command_output() {
    let event = CommandEvent::Output {
        stream: CommandStream::Stderr,
        line: "connection password=generated-secret failed".to_string(),
    };

    let redacted = redact_command_event(event, &["generated-secret"]);
    assert!(matches!(
        redacted,
        CommandEvent::Output { line, .. }
            if line == "connection password=<скрыто> failed"
    ));
}
```

- [ ] **Step 2: Record expected RED state without compiling locally**

Confirm `InstallationTiming` and `format_elapsed` do not exist. Do not run Cargo
locally.

- [ ] **Step 3: Implement deterministic timing state**

Use:

```rust
pub struct InstallationElapsed {
    pub total: Duration,
    pub stage: Duration,
}

pub struct InstallationTiming {
    installation_started: Instant,
    stage_started: Instant,
    finished_at: Option<Instant>,
}
```

`started` initializes both starts. `begin_stage` updates only `stage_started`.
`finish` records the first terminal instant only. `elapsed(now)` uses
`finished_at.unwrap_or(now)`. `format_elapsed` returns zero-padded
`HH:MM:SS`, allowing hours above 23.

- [ ] **Step 4: Implement output redaction**

`redact_command_event` replaces every non-empty known secret in
`CommandEvent::Output.line` with `<скрыто>`. Other event variants pass through
unchanged. Before forwarding PostgreSQL events to the UI, call it with the
generated DB password. Do not place the password in `ProgressEvent`, the GUI
state, or the persistent log.

- [ ] **Step 5: Extend GUI events and state**

Change `ProgressEvent` to include:

```rust
Operation(String),
Command(CommandEvent),
```

Add `timing: Option<InstallationTiming>` to `InstallerApp`. Start it when the
user clicks Install, reset stage time for every `Stage`, and freeze it for
`Done` or `Error`.

Render directly below `current_stage`:

```rust
let elapsed = timing.elapsed(Instant::now());
format!(
    "Всего {} · этап {}",
    format_elapsed(elapsed.total),
    format_elapsed(elapsed.stage)
)
```

Keep `request_repaint_after(150 ms)` so the counters update while the worker is
silent.

- [ ] **Step 6: Format command events into the details log**

For `Started`, append `[HH:MM:SS] > {display}`. For output, append each line with
`[stdout]` or `[stderr]`. For `Finished`, append a success/error line containing
the exit code when available and duration with one decimal place.

Use `chrono::Local` only for the display timestamp; add `chrono = "0.4"` to
`crates/installer/Cargo.toml`. Do not serialize or persist timestamps.

- [ ] **Step 7: Wire every setup operation**

Use the observer variants for cleanup, ACL restriction, `initdb`, `pg_ctl`, and
shortcut creation. Map their `CommandEvent` values to `ProgressEvent::Command`.

Emit `Operation` records after each verified download and extraction, after
authentication configuration, after saving DB configuration, and after
migrations. Messages contain paths and outcomes but never the generated
password or DSN.

- [ ] **Step 8: Format and commit**

Run only:

```powershell
cargo fmt --all
git diff --check
git add crates/installer/Cargo.toml crates/installer/src/lib.rs crates/installer/src/main.rs crates/installer/src/timing.rs crates/installer/src/log_safety.rs
git commit -m "feat(installer): show elapsed time and live commands"
```

---

### Task 4: GitHub-only verification

**Files:**
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: all tests and production code from Tasks 1–3.
- Produces: explicit Windows CI coverage for observed commands and Installer.

- [ ] **Step 1: Extend the Windows Installer Tests job**

Before the existing `Check installer` step add:

```yaml
- name: Test hidden observed commands
  run: cargo test -p evohime-launcher observed_command -- --nocapture

- name: Test installer timing
  run: cargo test -p evohime-installer timing -- --nocapture
```

- [ ] **Step 2: Review all changes without building**

Run only:

```powershell
cargo fmt --all -- --check
git diff --check
git status --short
git diff --stat
```

Confirm no files outside this plan are staged.

- [ ] **Step 3: Commit CI coverage**

```powershell
git add .github/workflows/rust.yml
git commit -m "ci(installer): verify live command reporting"
```

- [ ] **Step 4: Stop before external mutation**

Do not push. Report the commits and ask for the explicit command `пуш`.

- [ ] **Step 5: After explicit push, verify GitHub**

Push `main`, then require:

```text
CI / Windows Installer Tests / Test hidden observed commands = success
CI / Windows Installer Tests / Test installer timing = success
CI / Windows Installer Tests / Check installer = success
CI overall = success
Build Release overall = success
```

Confirm the new release tag follows `v0.0.NNNNNN`, points to the final commit,
and contains `evohime-setup.exe`.
