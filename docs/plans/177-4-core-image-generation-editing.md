# План 177.4 — Verification, release evidence и closure

## Изменить

1. Add focused Core/model-gateway/local-storage/IPC tests for contract,
   capability freshness, policy, output validation, artifact round-trip,
   cancellation, recovery, quotas and idempotency.
2. Add security regression fixtures for raw URL non-fetch, invalid magic/MIME,
   raw prompt/image absence from SQLite/logs/IPC and workspace-write approval.
3. Verify generated protocol types, affected Electron tests and the project’s
   static/build gates appropriate to changed crates; do not claim provider
   coverage where no backend exists.
4. Update `docs/architecture.md`, `docs/current-state.md` and
   `docs/release-evidence.md`; remove plan 177 files only after all criteria
   and evidence are canonicalized.

## Зависимости

### Блокирующие

- Completed 177.1–177.3 and fresh checks from affected crates/protocol.
- Existing release evidence format and documentation closure rule.

### Опциональные

- Provider-specific live generation is optional evidence; fake/typed
  unavailable coverage is sufficient for local-first baseline.

## Проверка

Run the focused Rust tests, protocol/type checks and `git diff --check`; inspect
final diff for unrelated changes, user files, generated artifacts and secrets.
Release evidence must distinguish local tests, CI evidence and unavailable
providers.

## Rollback и evidence

If any gate fails, keep plan 177 active and retain the failing evidence pointer;
do not remove plan files or mark the capability implemented. Closure consists
of canonical docs plus deletion of temporary plan files in a later task.
