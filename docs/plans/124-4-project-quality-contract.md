# План 124.4 — Project Quality Contract: verification, benchmark, release evidence и закрытие

Статус: этап 4 для [плана 124.0](./124-0-project-quality-contract.md); после полного Core/runtime/IPC/UI vertical slice.

## Цель

Подтвердить, что quality bar действительно измеряется и защищается: typed evidence, ratchet, phase placement, anti-bypass, exceptions, autonomous pause/recovery и metadata-only UI. Перенести доказанный контракт в каноническую документацию после реализации.

## Зависимости

### Блокирующие

- План 124.3 и Verification Evidence Ledger (#102) с рабочими lane/evidence/freshness gates.
- Existing Rust, storage, desktop IPC, Electron, packaging, redaction and release-evidence checks.

### Опциональные

- Agent Benchmark Matrix, Code Diagnostics, Architecture Snapshot, Plan Artifact, Goal/Change Set and Diagnostics Bundle.

## Матрица проверки

- Contract lifecycle: schema validation, bounded fields, canonical hash, immutable active revision, supersede/invalid/recovery and optimistic idempotency.
- Typed metrics: structured evidence extraction, legacy adapter revision/hash, units/bounds/aggregation, malformed/ambiguous/model-authored value rejection.
- Evaluation: fixed/range/boolean/presence pass/fail, stale/missing/unavailable/failed/unknown non-success, changed scope and stricter task/plan requirement.
- Ratchets: evidence-backed baseline, regression/stable/improved, manual/opt-in tightening, no automatic lowering, semantic verifier change invalidation and explicit rebaseline.
- Phase gates: EditFast/TaskComplete/Review/Commit/Ship/Deploy selection, budget/missing behavior, #102 execution and invalidation after material edits.
- Anti-bypass: threshold/gate/scope/verifier changes, skip/delete/suppression/ignore/exception/stub findings; deterministic violations separated from review candidates.
- Policy protection: pinned run cannot relax and continue; mixed/unknown delta requires review; bounded exception has reason/scope/actor/expiry and never forges Pass.
- Recovery: restart/interrupted evaluation, stale last-good snapshot, duplicate request, expired exception and bounded correction loop.
- IPC/UI: forged values rejected by Core, replay/resync/idempotency, redaction, semantic diff, accessible status and projection-only behavior.

## Обязательные gates

1. Focused Core/storage/evaluator/ratchet/anti-bypass/recovery tests with migration backup, rollback, corruption and fault injection.
2. Rust formatting, clippy, focused tests and full workspace regression appropriate to touched crates.
3. `npm run check:protocol`, Electron typecheck, unit/contract tests, build and bundle checks; compatibility IPC tests remain green.
4. `git diff --check`, redaction/provenance scan, no secrets/PII/raw verifier output in durable or release evidence.
5. Paired quality fixtures include positive, regression, no-evidence, no-op and legitimate-review-candidate cases; report measured values and verdict provenance, not a savings-only headline.

## Release evidence и закрытие

Evidence bundle содержит commit, schema/protocol/contract/metric/detector versions, fixture/config hashes, command IDs, test IDs, readiness verdicts, ratchet/anti-bypass/exception outcomes and redaction status. Не включать credentials, raw prompts/outputs, private source corpus, absolute machine paths or provider payloads.

После подтверждения criteria перенести фактический contract, storage revision, readiness semantics и security invariants в `docs/architecture.md`, состояние и test totals в `docs/current-state.md`, release gates/evidence procedure в `docs/development-plan.md` и `docs/release-evidence.md`. До этого план остаётся историческим implementation contract; после полной реализации его комплект удаляется по правилам проекта.

## Definition of Done

- [ ] ProjectQualityContract и все required consumers используют одну Core authority.
- [ ] #102 остаётся owner verification execution/evidence/freshness.
- [ ] Thresholds/ratchets/phase gates/anti-bypass/exception semantics покрыты воспроизводимыми тестами.
- [ ] Autonomous run не может понизить quality bar без явного policy review.
- [ ] IPC/UI metadata-only, redacted, replay-safe и accessibility-проверен.
- [ ] Release evidence воспроизводим, обезличен и отражён в canonical docs.

## Связанный issue

- [#104 Project Quality Contract](https://github.com/rkfsociety/EvoHime/issues/104)
