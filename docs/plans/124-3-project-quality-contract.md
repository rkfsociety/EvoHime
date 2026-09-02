# План 124.3 — Project Quality Contract: IPC, client projection и Quality UI

Статус: этап 3 для [плана 124.0](./124-0-project-quality-contract.md); после [плана 124.2](./124-2-project-quality-contract.md).

## Цель

Дать Electron bounded Quality surface для active contract, constraints, phase requirements, typed observations, baselines/ratchets, semantic changes, anti-bypass findings, exceptions и readiness — без переноса authority в renderer.

## Зависимости

### Блокирующие

- План 124.2: Core commands/events, evaluation, policy transitions and readiness snapshots.
- Authenticated desktop IPC, replay/resync, generated TypeScript protocol, main/preload bridge и existing Project/Workbench/Operations surfaces.

### Опциональные

- Verification UI from #102, Plan Artifact, Goal/Task/Change Set and Diagnostics Bundle projections.

## Реализация

1. После проверки highest protocol tag зарезервировать additive commands/events/results для contract catalog, draft/activation proposal, readiness evaluation, ratchet accept, policy delta/details, finding details, bounded exception request and phase status. Preserve major version, bounds, correlation, idempotency and replay.
2. Core validates every target, contract revision/hash, constraint/metric refs, actor, exception scope/expiry and evidence refs. Renderer cannot submit Pass/Fresh/Ready, metric value, baseline, trusted lane, detector result or approval as authoritative.
3. Передавать metadata-only projection: contract revision/hash prefix, scope/status, dimensions, thresholds, phase, lane/metric refs, observed value/unit, evidence/workspace short hashes, baseline/ratchet state, delta class, finding confidence/status, exception metadata and readiness verdict. Never expose secrets, raw commands with sensitive env, full logs or model transcript.
4. Связать `ipc_bridge.rs`, shared API, preload/main adapters, reconnect/replay and bounded error mapping. Unknown, stale, failed, review-required, exception and ready states remain distinct.
5. Добавить Project/Workbench → Quality views: active contract summary, constraints by phase, current readiness, metric/evidence provenance, ratchet candidate, semantic policy diff, anti-bypass findings and bounded exceptions.
6. Add explicit actions delegated to Core: inspect contract, run required/missing phase through #102, request policy review, accept ratchet where policy permits, request bounded exception and open authorized evidence/artifact. No automatic activation or relaxation from UI.
7. Show effective requirements as project minimum plus stricter task/plan requirements; indicate `ReadyWithExceptions`, `QualityRegression`, `NeedsPolicyReview`, `NeedsVerification` and `Unknown` with accessible labels and actionable next state.
8. Add renderer tests for forged metric/readiness rejection, stale/failed projection, semantic delta, ratchet direction, exception visibility, replay/resync, redaction and keyboard/screen-reader states.

## Acceptance-to-projection matrix

- `C01` contract → exact revision/status/hash and bounded rules.
- `C02` evidence → typed observation, provenance and freshness from Core.
- `C03` ratchet → baseline/current/candidate and monotonic action state.
- `C04` bypass → finding kind/confidence/diff refs without raw secret data.
- `C05` readiness → one Core verdict plus missing/regressed/exception reasons.
- `C06` security → authenticated bounded IPC; no client-side authority or capability expansion.

## Критерии выхода

- [ ] Renderer only projects Core results and cannot forge quality state.
- [ ] Policy changes show semantic delta, not merely raw JSON diff.
- [ ] Exceptions and review candidates are visibly distinct from green Ready.
- [ ] IPC replay, bounds, redaction and accessibility tests are green.

## Не входит

Client-side verifier execution/evaluation, direct filesystem/SQLite access, full CI/log viewer, silent contract activation, automatic override approval and arbitrary evidence opening.
