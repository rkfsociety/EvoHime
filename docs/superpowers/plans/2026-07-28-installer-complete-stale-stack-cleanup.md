# Installer Complete Stale-Stack Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a repeated installation close every verified EvoHime process, repair dirty-tree ACLs, and delete the interrupted installation with exact-path errors.

**Architecture:** Add reusable Windows process discovery to `evohime-win-support`, keep ACL and path-aware deletion primitives in focused installer modules, and let `dirty_cleanup` orchestrate graceful PostgreSQL shutdown, verified residual termination, ACL recovery, and bounded deletion retries. Every destructive decision is based on a canonical executable path under the dirty installation, never on a process name.

**Tech Stack:** Rust 2021, Tokio, Windows Toolhelp32/process APIs through `windows` 0.62, `icacls`, GitHub Actions Windows runners.

## Global Constraints

- Work directly on the current `main`; do not create a branch or worktree.
- Never terminate by process name alone.
- Never terminate a process whose executable path cannot be resolved.
- Never terminate a process outside the canonical dirty installation.
- Never terminate the current installer PID.
- Never follow symlinks or reparse points during recursive deletion.
- Never repair ACLs or delete content for an installation containing `.setup_complete`.
- Never continue after incomplete cleanup.
- Do not build or test locally; compilation and tests run only in GitHub Actions.
- Push every task commit to `origin/main`.

---

### Task 1: Discover and stop processes executing from one installation tree

**Files:**
- Create: `crates/win-support/src/process_snapshot.rs`
- Modify: `crates/win-support/src/lib.rs`
- Modify: `crates/win-support/Cargo.toml`
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: `resolve_process_exe_path(pid)`, `is_process_alive(pid)`, and `terminate_process(pid)`.
- Produces: `OwnedProcess { pid: u32, exe_path: PathBuf }`, `processes_in_directory(root: &Path, excluded_pid: u32) -> io::Result<Vec<OwnedProcess>>`, and `terminate_and_wait(processes: &[OwnedProcess], timeout: Duration) -> Result<(), ProcessCleanupError>`.

- [ ] **Step 1: Write failing Windows ownership and termination tests**

Copy `%SystemRoot%\System32\cmd.exe` into a temporary
`EvoHime\versions\current` directory, run the copy with a 30-second timeout,
and run a second `cmd.exe` from outside the tree. Assert that discovery returns
the copied process only:

```rust
let found = processes_in_directory(&install_dir, std::process::id()).unwrap();
assert!(found.iter().any(|process| process.pid == inside.id()));
assert!(!found.iter().any(|process| process.pid == outside.id()));
assert!(!found.iter().any(|process| process.pid == std::process::id()));
```

Call `terminate_and_wait(&found, Duration::from_secs(5))`, assert the inside
process exits, and assert the outside process remains alive. Clean up the
outside child in the test utility.

- [ ] **Step 2: Push RED and verify the focused GitHub failure**

Add this command to `Windows Installer Tests`:

```text
cargo test -p evohime-win-support process_snapshot::
```

Commit and push the test-only change. Expected: compilation fails because
`process_snapshot` and its exported interfaces do not exist.

- [ ] **Step 3: Implement the Toolhelp32 snapshot**

Enable:

```toml
"Win32_System_Diagnostics_ToolHelp",
```

Use `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)`,
`Process32FirstW`, and `Process32NextW` to enumerate PIDs. Skip PID `0` and
`excluded_pid`, resolve each executable, canonicalize it, and retain it only
when:

```rust
canonical_exe.starts_with(&canonical_root) && canonical_exe != canonical_root
```

Deduplicate by PID and return stable PID order for deterministic diagnostics.

- [ ] **Step 4: Implement bounded verified termination**

For every supplied `OwnedProcess`, re-resolve its executable immediately before
termination and require it to equal the stored canonical path. This prevents
PID reuse from terminating a different process. Call `terminate_process`, then
poll `is_process_alive` until all PIDs exit or `timeout` expires. Return:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProcessCleanupError {
    #[error("не удалось завершить процесс {pid} ({path})")]
    Terminate { pid: u32, path: PathBuf },
    #[error("процессы EvoHime не завершились вовремя: {processes:?}")]
    Timeout { processes: Vec<OwnedProcess> },
}
```

- [ ] **Step 5: Push GREEN and verify GitHub**

Expected: win-support snapshot tests pass and the outside process remains alive.

- [ ] **Step 6: Commit**

```text
git add crates/win-support/src/process_snapshot.rs crates/win-support/src/lib.rs crates/win-support/Cargo.toml .github/workflows/rust.yml
git commit -m "feat(win-support): stop processes owned by install tree"
git push origin main
```

### Task 2: Restore dirty-tree ACLs and report the exact undeletable path

**Files:**
- Modify: `crates/installer/src/icacls.rs`
- Create: `crates/installer/src/strict_remove.rs`
- Modify: `crates/installer/src/lib.rs`
- Modify: `crates/installer/tests/icacls_windows.rs`
- Create: `crates/installer/tests/strict_remove_windows.rs`

**Interfaces:**
- Produces: `restore_deletable_permissions(dir: &Path) -> Result<(), IcaclsError>`.
- Produces: `remove_tree_once(root: &Path) -> Result<bool, StrictRemoveError>` and `remove_tree_with_retries(root: &Path, attempts: usize, delay: Duration) -> Result<bool, StrictRemoveError>`.

- [ ] **Step 1: Write failing ACL recovery test**

Create a temporary nested tree, call `restrict_to_current_user`, then call the
new recovery function:

```rust
restore_deletable_permissions(&data).await.unwrap();
std::fs::remove_dir_all(&data).unwrap();
assert!(!data.exists());
```

Inspect `icacls` output and assert child entries inherit permissions after the
reset.

- [ ] **Step 2: Write failing exact-path deletion tests**

Open `dirty\nested\locked.bin` with Windows share mode `0`, call
`remove_tree_once`, and assert:

```rust
let error = remove_tree_once(&dirty).unwrap_err();
assert_eq!(error.path(), locked_path);
assert_eq!(error.source_error().raw_os_error(), Some(5));
```

For the retry test, release the lock from a short-lived thread during the retry
window and assert `remove_tree_with_retries` removes the complete tree. Add a
directory symlink or junction fixture and assert its target outside the dirty
tree remains untouched.

- [ ] **Step 3: Push RED and verify GitHub**

Commit and push test-only changes. Expected: unresolved imports for
`restore_deletable_permissions`, `remove_tree_once`, and
`remove_tree_with_retries`.

- [ ] **Step 4: Implement ACL recovery**

Reuse the checked `run_icacls` helper with:

```rust
let grant = format!("{username}:(OI)(CI)F");
run_icacls(dir, &["/grant:r", &grant, "/T", "/C", "/Q"]).await?;
run_icacls(dir, &["/reset", "/T", "/C", "/Q"]).await?;
```

Preserve command status, arguments, stdout, and stderr in `IcaclsError` so a
failed child path is visible in the installer error.

- [ ] **Step 5: Implement non-following strict deletion**

Use `symlink_metadata`. On Windows, treat any entry with
`FILE_ATTRIBUTE_REPARSE_POINT` as a link and remove the entry without
recursing. Traverse normal directories bottom-up. Wrap every operation:

```rust
#[derive(Debug, thiserror::Error)]
#[error("не удалось удалить {path}: {source}")]
pub struct StrictRemoveError {
    path: PathBuf,
    #[source]
    source: std::io::Error,
}
```

`remove_tree_with_retries` must reject `attempts == 0`, retain the final
path-aware error, and sleep asynchronously only between attempts.

- [ ] **Step 6: Push GREEN and verify GitHub**

Run remotely:

```text
cargo test -p evohime-installer --test icacls_windows
cargo test -p evohime-installer --test strict_remove_windows
```

Expected: ACL, locked nested path, retry, and reparse-point safety tests pass.

- [ ] **Step 7: Commit**

```text
git add crates/installer/src/icacls.rs crates/installer/src/strict_remove.rs crates/installer/src/lib.rs crates/installer/tests/icacls_windows.rs crates/installer/tests/strict_remove_windows.rs
git commit -m "feat(installer): recover ACLs and report blocked paths"
git push origin main
```

### Task 3: Coordinate complete stale-stack cleanup

**Files:**
- Modify: `crates/installer/src/dirty_cleanup.rs`
- Modify: `crates/installer/src/main.rs`
- Modify: `crates/installer/tests/setup_cleanup_windows.rs`
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: `processes_in_directory`, `terminate_and_wait`, `restore_deletable_permissions`, `remove_tree_with_retries`, and the existing portable PostgreSQL graceful stop.
- Produces: `clear_dirty_installation_safely(install_dir: &Path) -> Result<bool, DirtyCleanupError>` with typed process, ACL, and exact-path removal evidence.

- [ ] **Step 1: Extend the real executable integration test for portless residuals**

Start temporary `postgres.exe`, `pg_ctl.exe`, and an additional copied
`evohime-server.exe` under the dirty tree. Make the PostgreSQL stub close its
listener without exiting to reproduce the observed portless state. Call
`clear_dirty_installation_safely` and assert all three child processes exit and
the dirty tree disappears.

Start a same-named child outside the dirty tree and assert it remains alive.
Add a completed installation fixture with `.setup_complete` and assert its
inside process remains alive.

- [ ] **Step 2: Push RED and verify the observed failure**

Expected: cleanup returns an access-denied/path-aware error because the current
coordinator stops only the port owner and does not terminate all verified
residual processes.

- [ ] **Step 3: Implement the coordinator sequence**

For a dirty installation only:

1. attempt existing verified `postgres::stop`;
2. enumerate all residual processes below `install_dir`;
3. terminate and wait up to five seconds;
4. call `restore_deletable_permissions`;
5. call `remove_tree_with_retries` with five attempts and a 250 ms delay;
6. before each retry, refresh and terminate the verified process set again.

Extend `DirtyCleanupError` with transparent typed variants:

```rust
Process(#[from] ProcessCleanupError),
Permissions(#[from] IcaclsError),
Remove(#[from] StrictRemoveError),
```

Include residual `OwnedProcess` evidence in a final failure.

- [ ] **Step 4: Update installer progress messages**

Emit these stages in order through the existing progress channel:

```text
Закрываю оставшиеся процессы EvoHime...
Восстанавливаю права незавершённой установки...
Очищаю незавершённую установку...
```

Keep the final error context rooted at the installation directory while
retaining the typed child path/PID details.

- [ ] **Step 5: Push GREEN and verify the complete Windows job**

The Windows job must run:

```text
cargo test -p evohime-artifacts
cargo test -p evohime-win-support
cargo test -p evohime-launcher postgres::
cargo test -p evohime-installer
cargo check -p evohime-installer
```

Expected: every command succeeds, including portless residual-process,
outside-process, completed-installation, ACL, exact-path, retry, checksum, and
compilation checks.

- [ ] **Step 6: Commit**

```text
git add crates/installer/src/dirty_cleanup.rs crates/installer/src/main.rs crates/installer/tests/setup_cleanup_windows.rs .github/workflows/rust.yml
git commit -m "fix(installer): close complete stale stack before reinstall"
git push origin main
```

### Task 4: Verify and publish the corrected setup executable

**Files:**
- No planned file changes; inspect GitHub Actions and release outputs.

**Interfaces:**
- Consumes: the final implementation commit on `main`.
- Produces: a release whose tag and `evohime-setup.exe` were built from that exact commit.

- [ ] **Step 1: Verify full CI**

Confirm every job in the final CI run succeeds, with special attention to
`Windows Installer Tests`. Record the run URL and head SHA.

- [ ] **Step 2: Verify Build Release**

Confirm `Build evohime-setup.exe (Windows)` and every packaging job succeeds.
Record the run URL and head SHA.

- [ ] **Step 3: Verify the published release**

Confirm the new tag resolves to the final commit, the release is neither draft
nor prerelease, and `evohime-setup.exe` plus its SHA256 asset have the release
publication timestamp.

- [ ] **Step 4: Report**

Provide the final commit SHA, CI URL, release URL, and direct corrected setup
download URL. Do not claim the user-visible failure fixed before all three
remote checks are green.
