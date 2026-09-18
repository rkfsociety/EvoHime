# План 180.3 — Workflow и IPC integration

## Изменить

1. Add A2A as a selector/backend in existing delegated/child execution and
   team contracts. Workflow snapshots pin exact profile/card/protocol revision;
   card discovery cannot add a team member automatically.
2. Propagate typed remote outcomes (`input_required`, `auth_required`,
   `rejected`, `unknown_outcome`, validated completion) to existing workflow
   continuation and approval policies.
3. Add authenticated additive IPC for profile CRUD/inspection, card refresh,
   explicit test/delegation, status/cancel/reconcile and bounded artifact
   projection. Renderer never performs HTTP/discovery/auth.
4. Generate/check Electron adapters and integrate existing external-agent/
   operations surface without a new top-level runtime UI.
5. Keep credentials in existing supervisor/secret owner; IPC/diagnostics expose
   hashes, IDs, states and reason codes only.

## Зависимости

### Блокирующие

- Plan 180.1–180.2, existing workflow/team delegation and authenticated IPC.

### Опциональные

- UI actions may initially be limited to inspect/status; Core/CLI remains the
  authority for explicit delegation and reconcile.

## Проверка

Workflow replay preserves snapshots; remote failure propagates typed state; no
team roster expansion or duplicate submit on IPC reconnect. Protocol/type tests
assert bounded projections and absence of auth/context/artifact payloads.

## Rollback и evidence

If a protocol/card is unsupported, return typed unavailable and leave local
workflow intact. Existing ACP/external-agent paths must remain unchanged when
no A2A profile is configured.
