# План 178.3 — IPC, promotion и projection

## Изменить

1. Add authenticated additive IPC for list/status, preflight/start/cancel,
   verify/benchmark evidence and explicit promote/reject actions.
2. Generate/check Electron types and adapters; expose source/target hashes,
   state, fit/resource summaries, evidence status and rejection reason, never
   adapter secrets, local command lines, model weights or raw training data.
3. Route promotion through Core/runtime manager and approval policy with exact
   job/output/benchmark hashes; renderer can request but cannot grant or alter
   promotion.
4. Integrate with existing operations/model-management surface, not a second
   model registry or mandatory top-level tab. Missing adapter reports typed
   `unavailable`.

## Зависимости

### Блокирующие

- Plan 178.1–178.2, existing IPC protocol/auth, local runtime manager and
  approval/promotion owners.

### Опциональные

- UI can initially be read-only while CLI/Core tests exercise explicit actions.

## Проверка

Protocol compatibility/typecheck, bounded projection and replay tests; assert
that arbitrary adapter commands, paths, promotion flags and secrets are rejected
at the IPC/Core boundary. Verify reconnect never repeats conversion/promotion.

## Rollback и evidence

Additive IPC fields and disabled UI preserve existing model management. If a
promotion gate is unavailable, return typed blocked/unavailable and leave the
job in `ReadyForPromotion` or `Rejected`, never auto-activate.
