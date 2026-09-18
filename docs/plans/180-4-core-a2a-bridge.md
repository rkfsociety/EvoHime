# План 180.4 — Verification, release evidence и closure

## Изменить

1. Run adapter contract, network security, capability, capsule, lifecycle,
   artifact, recovery, workflow/team and IPC/Electron tests.
2. Verify no inbound listener/webhook/cloud relay, no dynamic discovery,
   credential forwarding, raw remote payload logging, artifact URL auto-fetch
   or blind resubmit exists.
3. Update canonical architecture/current-state/release evidence with supported
   outbound protocol/profile modes and typed unavailable boundaries; remove plan
   files only after all criteria are evidenced.

## Зависимости

### Блокирующие

- Completed 180.1–180.3 and fresh checks for adapter, network policy, storage,
  workflow and protocol surfaces.

### Опциональные

- A real remote A2A service is optional evidence; deterministic fake transport
  must prove all safety and recovery semantics.

## Проверка

Use focused Rust/network/IPC/Electron checks and required CI evidence; finish
with `git diff --check`, status and review for secrets, public listeners,
unrelated changes and preserved user files.

## Rollback и evidence

Keep the plan active if any external interoperability claim lacks evidence.
Disabling the bridge must not delete profile/task history or alter local-only
workflow behavior.
