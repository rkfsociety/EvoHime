# Installer stale PostgreSQL cleanup design

## Problem

When an installation is interrupted after portable PostgreSQL starts but before
`.setup_complete` is written, the next installer run detects a dirty
installation and immediately calls `remove_dir_all`. On Windows, the running
`postgres.exe` processes keep files in `pg16` open, so cleanup fails with
`Access denied (os error 5)`. Elevating the installer does not release those
file locks.

## Selected approach

Before deleting a dirty installation, the installer will inspect the portable
PostgreSQL deployment at:

```text
<install_dir>\pg16\bin
<install_dir>\pg16\data
```

If PostgreSQL is running on EvoHime's dedicated port and the listening process
is verified to be `postgres.exe` from that exact `pg16\bin` directory, the
installer will stop it through the existing launcher PostgreSQL helper:

```text
pg_ctl stop -D <install_dir>\pg16\data -m fast -w -t 30
```

Only after the verified EvoHime PostgreSQL instance has stopped will the
installer remove the dirty installation and continue with a fresh release
download.

The installer must never stop an unrelated PostgreSQL instance. A process that
cannot be verified against the expected executable directory is treated as
foreign and is not terminated.

## Components and flow

1. The installer acquires its existing single-instance lock.
2. It detects the missing `.setup_complete` marker.
3. A focused cleanup helper checks whether the expected `pg16\bin` and
   `pg16\data` paths exist.
4. The helper uses the existing `postgres::is_running` path-and-port
   verification.
5. If the verified portable database is running, the helper emits a progress
   stage and calls `postgres::stop`.
6. The installer calls the existing strict dirty-directory cleanup.
7. Installation continues by downloading every artifact from the latest
   release into fresh temporary files.

No generic `taskkill`, process-name matching, service manipulation, or
administrator elevation is added.

## Error handling

- If no verified EvoHime PostgreSQL is running, cleanup proceeds normally.
- If a verified instance cannot be stopped, installation aborts with a
  specific error that identifies PostgreSQL shutdown as the failed operation.
- If directory removal still fails after shutdown, the existing strict cleanup
  error is preserved.
- Foreign processes listening on the dedicated port are not killed; the
  eventual cleanup/startup error remains visible instead of risking unrelated
  user data.

## Testing

Windows CI will cover the integration path with a disposable installation
directory:

- a dirty installation with a running portable PostgreSQL instance is stopped
  before removal;
- cleanup succeeds after the database releases its files;
- an unrelated or unverifiable process is never stopped;
- an actual shutdown failure is returned instead of being ignored.

Existing installer artifact-download, checksum, locked-file, and dirty-cleanup
tests remain in the Windows installer job. Verification and compilation run
only in GitHub Actions; no local build is required.

## Success criteria

- Re-running the installer after the observed interrupted-installation state no
  longer fails with `Access denied (os error 5)` solely because EvoHime's own
  portable PostgreSQL is still running.
- The installer stops only a PostgreSQL executable verified inside the dirty
  EvoHime installation.
- Cleanup remains strict and never silently carries partial files into a new
  installation.
