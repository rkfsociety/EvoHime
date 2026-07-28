# Installer complete stale-stack cleanup design

## Problem

An interrupted EvoHime installation can leave more than one kind of state
behind:

- `evohime-launcher.exe`, `evohime-server.exe`, `pg_ctl.exe`, or
  `postgres.exe` may still be running from the installation directory;
- PostgreSQL may remain alive without listening on its normal port, so a
  port-only ownership check cannot find it;
- `pg16\data` intentionally has inheritance disabled and access restricted to
  one user SID, which can prevent a later elevated or differently-tokened
  installer from traversing and deleting the dirty tree;
- `remove_dir_all` reports only the root cleanup failure, leaving the user with
  an unhelpful `Access denied (os error 5)`.

The installer must turn this state into a clean installation without killing
unrelated applications or silently installing over partial files.

## Selected approach

Before deleting a dirty installation, the installer will perform a bounded
cleanup sequence:

1. enumerate Windows processes and select only processes whose resolved
   executable path is a descendant of the canonical dirty installation path;
2. attempt the existing graceful shutdown paths for recognizable EvoHime
   components;
3. wait briefly, then terminate only verified residual processes from the
   installation directory;
4. restore deletable inherited ACLs across the dirty tree;
5. remove the tree with path-aware errors and bounded retries.

The current installer process is always excluded. Process names alone are
never sufficient evidence for termination.

## Process discovery and shutdown

`evohime-win-support` will expose a Windows process snapshot helper. It will
enumerate PIDs with Toolhelp32, resolve each executable with the existing
`resolve_process_exe_path`, canonicalize both the executable and installation
directory, and retain only strict descendants of the installation directory.

The installer will:

- use `postgres::stop` first when the verified portable PostgreSQL instance is
  detectable through its normal port;
- allow known HTTP components a short graceful shutdown opportunity when their
  existing endpoint and authentication material are available;
- refresh the process snapshot;
- call the existing Windows termination primitive only for residual verified
  PIDs;
- wait until every terminated PID exits or a bounded timeout expires.

A PID that cannot be resolved to an executable under the installation
directory is never terminated. The installer's own PID is never terminated.

This also handles the observed state where `pg_ctl.exe` and `postgres.exe`
remain alive but no longer own port `55432`: they are found by executable
location rather than port.

## ACL recovery

ACL recovery runs only for a dirty installation that is about to be deleted.
It does not weaken permissions on a completed installation.

The installer will run `icacls` against the dirty root to grant the current
interactive user inheritable full control and reset child ACLs to inheritance:

```text
icacls <install_dir> /grant:r <current-user>:(OI)(CI)F /T /C /Q
icacls <install_dir> /reset /T /C /Q
```

Command status and stderr are checked. Failure aborts installation with an ACL
specific message. After the fresh PostgreSQL data directory is created, the
existing `restrict_to_current_user` step applies its secure restricted ACL
again.

## Path-aware strict deletion

The dirty tree will be removed without following symlinks or Windows reparse
points. The deletion helper traverses directory entries bottom-up, removes
files and links as entries, then removes directories. Every I/O error is
wrapped with the exact path being processed.

Deletion is attempted a bounded number of times with short asynchronous delays
to absorb normal Windows handle-release latency. Between attempts, the
installer refreshes the verified-process set and terminates any EvoHime process
that reappeared from the dirty directory.

If cleanup still fails, the final error includes:

- the exact file or directory path that could not be removed;
- the Windows OS error;
- any verified residual EvoHime PID and executable path;
- whether ACL restoration or process termination failed earlier.

Partial cleanup is never treated as success.

## Components

### `evohime-win-support`

- Enumerate process IDs and resolved executable paths.
- Filter canonical executable paths to a specific canonical directory.
- Terminate and wait for verified PIDs using existing liveness primitives.
- Keep all path ownership decisions independent of process names.

### `evohime-installer::icacls`

- Add a dirty-tree ACL recovery operation separate from
  `restrict_to_current_user`.
- Preserve command output in typed errors.

### `evohime-installer::dirty_cleanup`

- Coordinate graceful shutdown, verified residual termination, ACL recovery,
  retries, and strict deletion.
- Emit typed errors that retain the failed path and residual process evidence.

### Installer UI

The progress stream will distinguish:

```text
Закрываю оставшиеся процессы EvoHime...
Восстанавливаю права незавершённой установки...
Очищаю незавершённую установку...
```

The final user-visible error will retain the typed cleanup details.

## Testing

Windows GitHub Actions will run all installer and win-support tests.

Tests will create disposable executable stubs inside and outside a temporary
installation tree and verify:

- every process executing from inside the dirty installation is closed;
- a same-named process outside the installation remains alive;
- the current test/installer PID is excluded;
- a process that ignores graceful shutdown is terminated after the timeout;
- a residual process or termination failure aborts cleanup with its PID/path;
- restricted child ACLs are reset and the tree becomes deletable;
- a nested file opened without delete sharing triggers retries and reports its
  exact path;
- cleanup succeeds after a handle is released during the retry window;
- completed installations are never subjected to process termination or ACL
  reset.

The existing real temporary `postgres.exe`/`pg_ctl.exe` integration tests,
artifact checksum tests, and installer compilation check remain enabled.
Compilation and test execution occur only in GitHub Actions.

## Safety boundaries

- Never terminate by process name alone.
- Never terminate a process whose executable path cannot be resolved.
- Never terminate a process outside the canonical dirty installation.
- Never follow symlinks or reparse points during recursive deletion.
- Never repair ACLs or delete content for an installation containing
  `.setup_complete`.
- Never continue installation after an incomplete cleanup.
- Never create a branch or worktree; changes go directly to `main`.

## Success criteria

- Re-running the installer closes all residual executables from the dirty
  EvoHime installation, including processes no longer listening on expected
  ports.
- Restricted PostgreSQL ACLs cannot cause the observed generic root-level
  `Access denied (os error 5)` failure.
- Unrelated processes and completed installations remain untouched.
- Any remaining failure identifies the exact path and verified residual
  process evidence.
- The corrected setup executable is built, tested, and published by GitHub
  Actions.
