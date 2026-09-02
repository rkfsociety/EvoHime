# План 119.2 — Execution Environment Profiles: resolver, activation и recovery

Статус: этап 2 для [плана 119.0](./119-0-execution-environment-profiles.md); после [плана 119.1](./119-1-execution-environment-profiles.md).

## Цель

Разрешать profile refs через существующие registries, выполнять bounded preflight и атомарную активацию на safe boundary, строить effective snapshot, обновлять provenance и восстанавливаться после restart без partial switch.

## Зависимости

### Блокирующие

- План 119.1 — contract, storage, hashes, state and compatibility matrix.
- Model routing, backend/external agent, MCP/workbench, skill/instruction, policy/approval/budget/credential and data-boundary authorities.
- Existing run lifecycle, event journal, cancellation, artifact/provenance and recovery.

### Опциональные

- Local Model Runtime Manager for local readiness.
- Scheduler/Event Trigger Runtime for future profile addressing, not a new scheduler.

## Реализация

1. Реализовать Core resolver с deterministic ordering: resolve scope layers → refs/revisions → owner health/capabilities → policy/data boundary → conflicts → derived state and diagnostics.
2. Реализовать preflight: required refs, pinned revision availability, FollowCompatible rules, provider/model purpose capabilities, external adapter/runtime, MCP/workbench definitions, skill/instruction revisions, credentials slots, workspace scope, hard ceilings, budget and conflicting bindings.
3. Реализовать binding-specific safe-boundary calculation. `NewRunOnly` never mutates active run; `NextTurn` разрешён только для explicitly rehydratable bindings; process/grant snapshots force NewRunOnly.
4. Реализовать effective snapshot builder and provenance: exact resolved revisions/hashes, policy/tool/skill/instruction/model/external refs, credential slot ids and activation event; no raw secret/prompt/output.
5. Реализовать all-or-nothing activation transaction for required bindings with owner-subsystem prepare/commit/rollback contracts. Optional failures produce explicit Degraded projection. Unknown activation outcome is reconciled, not blindly repeated.
6. Реализовать drift detection: missing/changed refs lead NeedsReview/Broken; historical snapshots remain immutable; FollowCompatible records actual selected revision; security-sensitive refs never silently float to latest.
7. Реализовать rollback as new activation of previous valid profile/snapshot at the next allowed boundary. Never rewrite completed run provenance.
8. Реализовать restart/recovery: restore profile/revision/current scope and activation log, reconcile incomplete activation, mark stale profile state, keep active runs pinned, and leave last valid environment available when candidate activation fails.

## Fault/recovery matrix

- missing required binding/credential → Broken and no partial activation;
- missing optional binding → Degraded with diagnostic;
- capability/policy/data-boundary conflict → typed denial;
- MCP/workbench restart required → safe boundary delay, no hidden hot switch;
- provider/external runtime unknown outcome → pending/reconcile, no blind retry;
- profile revision changes during run → old effective snapshot retained;
- Core restart during activation → incomplete event reconciled or failed, previous state preserved;
- rollback precondition stale → conflict, history unchanged.

## Критерии выхода

- [ ] Valid profile resolves deterministically and activates atomically.
- [ ] Required failure cannot leave mixed model/MCP/policy state.
- [ ] Optional failure is explicit Degraded and does not widen capabilities.
- [ ] Effective snapshots are immutable and attached to run provenance.
- [ ] Safe boundaries, drift, rollback and restart recovery are reproducible.

## Не входит

New provider/MCP/skill/runtime implementations, arbitrary config file writes, generic plugin execution, direct renderer activation и new scheduler.
