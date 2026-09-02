# План 122.3 — Verification Evidence Ledger: IPC, client projection и UI

Статус: этап 3 для [плана 122.0](./122-0-verification-evidence-ledger.md); после [плана 122.2](./122-2-verification-evidence-ledger.md).

## Цель

Дать Electron bounded Verification surface для lanes, runs, evidence, freshness, readiness, missing/stale reruns, evidence opening и explicit override без переноса проверки в renderer.

## Зависимости

### Блокирующие

- План 122.2 — runner/resolver/readiness commands/events/recovery.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge, ArtifactStore evidence open route и existing Operations/Project navigation.

### Опциональные

- Architecture/Change Set panels, Diagnostics Bundle and ContextRefs.

## Реализация

1. После проверки highest tag зарезервировать additive commands/events/results для lane catalog, run missing/stale/selected, cancel, evidence/readiness details, open artifact/evidence, policy/target status and override request. Preserve major, bounds, correlation, idempotency and replay.
2. Core validates target/scope/actor/lane revision/command trust/policy/budget and recomputes fingerprints/readiness. Renderer cannot provide fingerprint, Pass/Fresh/Ready, trusted command or override approval.
3. Передавать metadata-only projections: lane/run/evidence ids, revisions, status/outcome, workspace/environment short hashes, verifier identity/version, duration, freshness reason, reviewer class, artifact refs, readiness requirements/verdict and bounded redacted summaries.
4. Связать `ipc_bridge.rs`, shared API, preload/main adapters and reconnect/replay. Running/Unknown/Unavailable/Skipped/Override states remain distinct from Passed/Fresh/Ready.
5. Добавить Project/Workbench → Verification: lane table, current target readiness, Fresh/Stale/Missing/Failed/Unknown breakdown, Run Missing/Re-run Stale/Re-run Selected, exact evidence/artifact opening and readiness details.
6. Show per-lane environment/command/reviewer provenance, invalidation reason, conditional scope and required/optional status. Do not expose secrets/env values/full private output by default.
7. Add task/plan/commit/ship/goal target views and compact badges; show human override as unresolved requirements, never as green PASS. Optional CLI delegates to same Core commands.
8. Add accessibility for blocked/needs verification/needs human review/incomplete/unknown and safe handling of untrusted review text.

## Acceptance-to-projection matrix

- `C01` lanes/runs → status, revision, scope, executor and environment metadata.
- `C02` evidence → outcome, freshness, reviewer class and artifact refs.
- `C03` readiness → required/missing/stale/failed/unavailable/unknown and verdict.
- `C04` actions → Core-mediated rerun/cancel/open/override outcomes.
- `C05` consumers → task/plan/goal/change/commit/ship projections.
- `C06` security → authenticated bounded data and no forged readiness.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded and replay-safe.
- [ ] Mutations Core-validated/idempotent with typed stale/denied/unknown outcomes.
- [ ] UI never computes fingerprints, freshness, evidence validity or readiness.
- [ ] Raw secrets, environment values, private outputs and arbitrary commands stay out of projection.

## Не входит

Direct filesystem/SQLite access, client-side verifier execution, client-side readiness calculation, automatic override approval и full CI log viewer.
