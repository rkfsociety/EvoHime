# План 178.4 — Verification, release evidence и closure

## Изменить

1. Run focused contract, fake-adapter, storage, resource, Windows process,
   cancellation, recovery, calibration, benchmark, promotion, IPC and security
   tests.
2. Verify negative paths: source/output hash mismatch, changed adapter,
   malformed format, insufficient resources, stale baseline, missing checkpoint,
   orphan publication and renderer promotion attempt.
3. Update `docs/architecture.md`, `docs/current-state.md` and
   `docs/release-evidence.md` with actual supported adapter/format and explicit
   unavailable gates. Remove 178 plan files only after closure criteria pass.

## Зависимости

### Блокирующие

- Completed 178.1–178.3 and fresh checks for all affected crates/protocol.

### Опциональные

- Live accelerator performance is optional evidence; it cannot replace the
  deterministic structural/runtime verification gate.

## Проверка

Use the project’s narrow Rust/IPC/Electron checks first, then the required CI
evidence. Finish with `git diff --check`, status and a diff review excluding
the user’s untracked updater directories.

## Rollback и evidence

Keep plan active when an adapter or benchmark gate fails. No source model,
active runtime or user data is removed during rollback; only unreferenced
staging may be quarantined under the existing artifact policy.
