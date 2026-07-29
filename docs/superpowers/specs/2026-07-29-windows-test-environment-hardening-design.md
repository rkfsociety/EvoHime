# Windows Test Environment Hardening Design

**Date:** 2026-07-29  
**Status:** Approved

## Goal

Remove the two remaining local-environment blockers:

1. the launcher streaming test must pass on supported Windows environments
   that provide `pwsh.exe` but not legacy `powershell.exe`;
2. Git and Python tooling must be able to traverse
   `workers/python/.pytest_cache` without permission warnings.

## Launcher test

The test exercises `run_observed_command`, not PowerShell. On Windows it
will use the operating-system `cmd.exe` to emit one stdout line, one stderr
line, and exit with code 7. This removes an unrelated shell dependency
without adding production shell-discovery logic.

The change follows RED-GREEN:

- reproduce the current `program not found` failure;
- replace only the Windows test command;
- run the focused test and the complete `evohime-launcher` test suite.

Production command execution remains unchanged.

## Pytest cache permissions

The cache is disposable generated state and is excluded from source
control. First attempt to restore inherited ACLs from `workers/python`.
If the inaccessible directory cannot be repaired in place, remove exactly
`workers/python/.pytest_cache` and allow pytest to recreate it with the
parent directory's normal inherited permissions.

Before any removal, resolve and verify the absolute path. Do not touch
other worker files or broad directories.

## Verification

- `cargo fmt --all -- --check`
- focused launcher streaming test
- `cargo test -p evohime-launcher`
- Python worker tests with a newly accessible cache
- `git status --short --branch` without the `.pytest_cache` permission warning
- final `cargo clean`

Repository changes are committed on the current `main`. Nothing is pushed
without a separate explicit request.
