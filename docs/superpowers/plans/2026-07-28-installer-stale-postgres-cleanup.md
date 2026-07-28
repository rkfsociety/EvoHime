# Installer Stale PostgreSQL Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop only EvoHime's verified portable PostgreSQL before removing an interrupted installation.

**Architecture:** Keep executable ownership verification in the launcher PostgreSQL module, where port-to-PID resolution already lives. Add an installer cleanup coordinator that injects the running check and shutdown operation for deterministic Windows tests, while its public wrapper binds those hooks to `postgres::is_running` and `postgres::stop`.

**Tech Stack:** Rust 2021, Tokio, existing `evohime-launcher::postgres`, Windows GitHub Actions.

## Global Constraints

- Work directly on the current `main`; do not create a branch or worktree.
- Never stop a PostgreSQL process unless its executable resolves to the expected `<install_dir>\pg16\bin\postgres.exe`.
- Preserve strict cleanup: any shutdown or removal failure aborts installation.
- Do not use `taskkill`, process-name matching, service manipulation, or elevation.
- Do not build or test locally; run compilation and tests only in GitHub Actions.
- Commit each completed task and push it to `origin/main`.

---

### Task 1: Make PostgreSQL executable verification independently testable

**Files:**
- Modify: `crates/launcher/src/postgres.rs`
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: `evohime_win_support::resolve_process_exe_path(pid)`.
- Produces: private `is_expected_postgres_executable(exe_path: &Path, pg_bin_dir: &Path) -> bool`, used by `is_running`.

- [ ] **Step 1: Write failing path-ownership tests**

Add tests showing that the exact portable executable is accepted and a
same-named executable in another directory is rejected:

```rust
#[test]
fn expected_postgres_executable_requires_exact_bin_directory() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("EvoHime").join("pg16").join("bin");
    let foreign_bin = root.path().join("foreign").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&foreign_bin).unwrap();
    std::fs::write(bin.join("postgres.exe"), b"expected").unwrap();
    std::fs::write(foreign_bin.join("postgres.exe"), b"foreign").unwrap();

    assert!(is_expected_postgres_executable(
        &bin.join("postgres.exe"),
        &bin
    ));
    assert!(!is_expected_postgres_executable(
        &foreign_bin.join("postgres.exe"),
        &bin
    ));
}
```

- [ ] **Step 2: Push the red test and verify failure in GitHub Actions**

Add the focused launcher command to `Windows Installer Tests`, commit and push
the red test, then run remotely:

```text
cargo test -p evohime-launcher postgres::tests::expected_postgres_executable_requires_exact_bin_directory
```

Expected: compilation fails because `is_expected_postgres_executable` does not
exist.

- [ ] **Step 3: Implement the minimal path verifier**

Extract the executable comparison from `is_running`:

```rust
fn is_expected_postgres_executable(exe_path: &Path, pg_bin_dir: &Path) -> bool {
    let expected = pg_bin_dir.join("postgres.exe");
    exe_path
        .canonicalize()
        .ok()
        .zip(expected.canonicalize().ok())
        .is_some_and(|(actual, expected)| actual == expected)
}
```

Make `is_running` delegate to this helper after resolving the listener PID.

- [ ] **Step 4: Push and verify green in GitHub Actions**

Expected: the focused launcher test and existing launcher PostgreSQL tests pass.

- [ ] **Step 5: Commit**

```text
git add crates/launcher/src/postgres.rs .github/workflows/rust.yml
git commit -m "test(launcher): pin portable postgres ownership"
git push origin main
```

### Task 2: Stop stale portable PostgreSQL before strict cleanup

**Files:**
- Create: `crates/installer/src/dirty_cleanup.rs`
- Modify: `crates/installer/src/lib.rs`
- Modify: `crates/installer/src/main.rs`
- Modify: `crates/installer/tests/setup_cleanup_windows.rs`
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: `postgres::is_running(&Path, postgres::PG_PORT)` and `postgres::stop(&Path, &Path)`.
- Produces: `clear_dirty_installation_safely(install_dir: &Path) -> Result<bool, DirtyCleanupError>`.

- [ ] **Step 1: Write failing cleanup-order tests**

In `dirty_cleanup.rs`, add a Windows unit test that locks a file without delete
sharing, then releases that lock from the verified shutdown hook:

```rust
let locked = Arc::new(Mutex::new(Some(open_without_delete_sharing(&locked_path))));
let shutdown_calls = Arc::new(AtomicUsize::new(0));
let result = clear_with_hooks(
    &install_dir,
    |_| true,
    {
        let locked = Arc::clone(&locked);
        let shutdown_calls = Arc::clone(&shutdown_calls);
        move |_, _| async move {
            shutdown_calls.fetch_add(1, Ordering::SeqCst);
            locked.lock().unwrap().take();
            Ok(())
        }
    },
).await;

assert!(result.unwrap());
assert!(!install_dir.exists());
assert_eq!(shutdown_calls.load(Ordering::SeqCst), 1);
```

Add a second test with `|_| false` and an atomic shutdown counter; assert the
counter remains zero. Add a third test whose shutdown hook returns
`Err("stop failed".to_string())`; assert the result matches
`DirtyCleanupError::PostgresStop(_)` and `install_dir.exists()` remains true.

- [ ] **Step 2: Push the red tests and verify failure in GitHub Actions**

Update the Windows job to run:

```text
cargo test -p evohime-installer
```

Expected: compilation fails because `clear_dirty_installation_safely` and the
coordinator do not exist.

- [ ] **Step 3: Implement the cleanup coordinator**

Create `dirty_cleanup.rs` with:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DirtyCleanupError {
    #[error("не удалось остановить PostgreSQL незавершённой установки: {0}")]
    PostgresStop(String),
    #[error(transparent)]
    Remove(#[from] std::io::Error),
}

pub async fn clear_dirty_installation_safely(
    install_dir: &Path,
) -> Result<bool, DirtyCleanupError> {
    clear_with_hooks(
        install_dir,
        |bin| postgres::is_running(bin, postgres::PG_PORT),
        |bin, data| async move {
            postgres::stop(&bin, &data)
                .await
                .map_err(|error| error.to_string())
        },
    )
    .await
}
```

The private generic `clear_with_hooks` must:

1. return `Ok(false)` for a completed or nonexistent installation;
2. derive `pg16/bin` and `pg16/data`;
3. invoke shutdown only when both paths exist and the ownership check is true;
4. await shutdown before calling the existing `clear_dirty_installation`;
5. return shutdown and removal failures without ignoring them.

- [ ] **Step 4: Wire the installer progress and error path**

In `run_installation_fallible`, replace the direct cleanup call with
`clear_dirty_installation_safely`. Emit:

```text
Обнаружена незавершённая установка, останавливаю встроенную базу и очищаю...
```

Preserve the outer error context with the installation directory.

- [ ] **Step 5: Push and verify green in GitHub Actions**

The Windows job must run:

```text
cargo test -p evohime-artifacts
cargo test -p evohime-launcher postgres::
cargo test -p evohime-installer
cargo check -p evohime-installer
```

Expected: all commands succeed, including the locked-file cleanup regression
test and the executable ownership tests.

- [ ] **Step 6: Commit**

```text
git add crates/installer/src/dirty_cleanup.rs crates/installer/src/lib.rs crates/installer/src/main.rs crates/installer/tests/setup_cleanup_windows.rs .github/workflows/rust.yml
git commit -m "fix(installer): stop stale portable postgres"
git push origin main
```

### Task 3: Release and remote installation verification

**Files:**
- No planned file changes; inspect the existing Actions and release results.

**Interfaces:**
- Consumes: successful `main` CI artifacts.
- Produces: a release containing the corrected `evohime-setup.exe`.

- [ ] **Step 1: Verify the complete `main` CI run**

Inspect every job in the GitHub Actions run for the implementation commit.
Expected: all required jobs succeed; specifically, `Windows Installer Tests`
shows zero failed tests and `cargo check` exits with code 0.

- [ ] **Step 2: Verify the release workflow**

Inspect the release run triggered from the verified commit. Confirm that the
published `evohime-setup.exe` belongs to that commit and the release assets
complete successfully.

- [ ] **Step 3: Report the exact release URL and commit**

Do not claim the user-visible bug fixed until both the regression test and
release build are green. Provide the commit SHA, CI run URL, release URL, and a
short instruction to download the new setup executable.
