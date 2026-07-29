# Windows Test Environment Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** Implemented on 2026-07-29

**Execution record:**

- RED reproduced: the Windows launcher streaming test failed with
  `program not found` because only `pwsh.exe` is installed.
- The first `cmd.exe` command exposed trailing spaces from `echo`; the
  corrected `echo(` form preserved the test's exact output contract.
- Commit `28117ee` replaces only the Windows test dependency; production
  command execution is unchanged.
- Focused launcher test passed `1/1`; the full launcher crate passed 59
  library tests plus bin, integration and doc-test targets.
- The exact pytest cache path was verified. Non-elevated ACL repair and
  ownership takeover were denied, then an elevated `takeown` + `icacls`
  operation completed with exit code 0 for that directory only.
- The repaired cache is owned by `DESKTOP-KI56DT3\USSR`, grants the current
  user full control, Python worker tests passed `25/25`, and Git no longer
  emits the `.pytest_cache` permission warning.
- A first process-discovery filter matched its own PowerShell command line
  and terminated that temporary shell before cleanup. The corrected check
  excludes `$PID` and matches process names only; no EvoHime, Cargo or
  rustc process was running.
- Final `cargo clean` removed 4,204 files (3.4 GiB), and the workspace
  `target` directory no longer exists.

**Goal:** Remove the launcher test's legacy PowerShell dependency and restore a warning-free, disposable Python test cache.

**Architecture:** Keep production command execution unchanged. Make the Windows-only streaming test use the operating-system command processor, repair or recreate only the inaccessible pytest cache, and verify both toolchains before cleaning Rust artifacts.

**Tech Stack:** Rust 2021, Tokio tests, Windows `cmd.exe`/ACL tooling, Python pytest, Git, Cargo.

## Global Constraints

- Work directly in the current `main`; do not create a branch or worktree.
- Do not add production shell-discovery logic for a test-only dependency.
- Resolve and verify `C:\github\EvoHime\workers\python\.pytest_cache` before any ACL change or removal.
- Do not modify or remove any other worker files.
- Commit repository changes after each completed coding or documentation task.
- Push only on a separate explicit request.
- Stop EvoHime processes without additional confirmation when they block verification or cleanup.
- Run `cargo clean` only after all Rust verification is complete.

---

### Task 1: Remove the launcher test's legacy PowerShell dependency

**Files:**
- Modify: `crates/launcher/src/observed_command.rs`

**Interfaces:**
- Consumes: existing `assert_failed_command_events(command: tokio::process::Command)`.
- Produces: the same stdout/stderr/exit-code assertions without a `powershell.exe` dependency.

- [x] **Step 1: Confirm RED**

Run:

```powershell
cargo test -p evohime-launcher --lib observed_command::tests::streams_stdout_stderr_and_terminal_status -- --exact --nocapture
```

Expected: FAIL with `program not found` while the test constructs
`Command::new("powershell")`.

- [x] **Step 2: Use the Windows command processor**

In the existing `#[cfg(windows)]` test, construct:

```rust
let mut command = Command::new("cmd.exe");
command.args([
    "/D",
    "/S",
    "/C",
    "echo stdout-line & echo stderr-line 1>&2 & exit /b 7",
]);
```

Do not change `run_observed_command` or the non-Windows test.

- [x] **Step 3: Verify GREEN**

Run:

```powershell
cargo test -p evohime-launcher --lib observed_command::tests::streams_stdout_stderr_and_terminal_status -- --exact --nocapture
cargo test -p evohime-launcher
cargo fmt --all -- --check
git diff --check
```

Expected: focused and full launcher tests pass; formatting and whitespace
checks are clean.

- [x] **Step 4: Commit**

```powershell
git add -- crates/launcher/src/observed_command.rs
git commit -m "test(launcher): remove legacy PowerShell dependency"
```

---

### Task 2: Restore pytest cache access

**Files:**
- Repair or recreate generated directory: `workers/python/.pytest_cache`
- No tracked source files.

**Interfaces:**
- Consumes: inherited ACLs from `workers/python`.
- Produces: a traversable cache owned by the current development environment.

- [x] **Step 1: Verify the exact target**

Resolve the repository root and cache path. Abort unless the cache path is
exactly:

```text
C:\github\EvoHime\workers\python\.pytest_cache
```

- [x] **Step 2: Attempt in-place ACL repair**

Enable inheritance and reset child ACLs with `icacls` against the exact
verified cache path. Re-check access with `Get-Acl` and directory listing.

- [x] **Step 3: Recreate only if repair is impossible**

The elevated in-place repair succeeded, so the conditional recreation was
correctly not performed.

If the cache remains inaccessible, remove exactly the verified generated
cache directory using a single-shell, explicit-path operation, then let
pytest recreate it. Do not enumerate paths in one shell and delete them in
another.

- [x] **Step 4: Verify Python and Git access**

Run from `workers/python` using the project's configured Python:

```powershell
python -m pytest
```

Then run from the repository root:

```powershell
git status --short --branch
```

Expected: Python tests pass and Git emits no `.pytest_cache` permission
warning.

---

### Task 3: Record and finalize the environment repair

**Files:**
- Modify: `docs/superpowers/specs/2026-07-29-windows-test-environment-hardening-design.md`
- Modify: `docs/superpowers/plans/2026-07-29-windows-test-environment-hardening.md`
- Create: `C:\Users\USSR\.codex\memories\extensions\ad_hoc\notes\<timestamp>-evohime-environment-hardening.md`

**Interfaces:**
- Records exact commands, results, commit hashes, ACL outcome, and standing authorization to stop EvoHime.

- [x] **Step 1: Update execution records**

Set the design and plan statuses to `Implemented`, check only completed
steps, and record actual verification results without claiming blocked
checks passed.

- [x] **Step 2: Commit repository documentation**

```powershell
git add -- docs/superpowers/specs/2026-07-29-windows-test-environment-hardening-design.md docs/superpowers/plans/2026-07-29-windows-test-environment-hardening.md
git commit -m "docs: record Windows environment hardening"
```

- [x] **Step 3: Update memory**

Add one ad-hoc memory note recording the durable workflow and superseding
the earlier known launcher portability limitation.

- [x] **Step 4: Clean and verify**

Confirm no EvoHime/Cargo/Rust process is using build artifacts, then run:

```powershell
cargo clean
```

Verify `C:\github\EvoHime\target` no longer exists and finish with:

```powershell
git diff --check
git status --short --branch
git log --oneline --decorate -8
```
