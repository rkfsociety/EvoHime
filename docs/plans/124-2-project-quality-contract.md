# План 124.2 — Project Quality Contract: metric evaluation, ratchets и anti-bypass runtime

Статус: этап 2 для [плана 124.0](./124-0-project-quality-contract.md); после [плана 124.1](./124-1-project-quality-contract.md).

## Цель

Реализовать Core evaluation поверх #102: извлечение typed metrics, phase/changed-scope resolution, threshold/ratchet comparison, semantic policy diff, anti-bypass guard, readiness snapshot и интеграцию с autonomous consumers.

## Зависимости

### Блокирующие

- План 124.1 и готовые #102 lane/evidence/freshness/readiness primitives.
- Existing Continuation Policy, Composable Termination, Goal/Task/Plan/Change Set, policy gate, audit, cancellation and recovery.

### Опциональные

- Code Diagnostics, Architecture Snapshot, Agent Skills recommendations, Agent Benchmark Matrix and Execution Environment Profiles.

## Реализация

1. Реализовать Core-owned metric extractor registry для structured #102 evidence и versioned trusted adapters для legacy command artifacts. Extractor обязан проверять source kind, unit, bounds, evidence identity и aggregation; malformed/ambiguous values становятся Unknown.
2. Resolve effective requirements: active project contract + phase profile + task/plan stricter constraints. Применять conditions по scope/target/component без уменьшения области проверки при недоказуемой зависимости.
3. Реализовать fixed/range/boolean/presence evaluation и typed result states. Только Fresh compatible Passed evidence satisfies required constraint; Missing/Stale/Failed/Unavailable/Skipped/Unknown remain non-success.
4. Реализовать ratchet semantics `regression/stable/improved`, evidence-backed candidate creation, ManualAccept и opt-in AutoTightenOnAcceptedChange. Reject lowering, expired/incompatible baseline and verifier/metric semantic drift until explicit rebaseline.
5. Реализовать semantic contract diff: added/removed/tightened/relaxed constraints, changed lane/metric/scope/phase/exception and verdict `Tightening/Equivalent/Relaxation/Mixed/Unknown`.
6. Реализовать anti-bypass detector against trusted workspace/change-set diff and contract revisions: threshold relax, gate removal/downgrade, verifier command/scope change, deleted/skipped tests, new suppression/ignore, exception expansion, unimplemented stub and silent error handling. Preserve exact diff/evidence refs; assertion removal remains review candidate unless deterministic proof exists.
7. Реализовать policy transition: material relaxation or Unknown delta pauses/escalates active run to `NeedsQualityPolicyReview`; explicit actor/reason/scope/expiry exception is bounded and auditable. Contract changes never mutate old evidence or forge Passed.
8. Собрать immutable `QualityReadinessSnapshot` for task/plan/goal/change/commit/ship/deploy targets, with passed/regressed/missing/stale/failed/bypass/exception lists, contract/workspace refs and verdict. Consumers read this one authority; they do not recalculate metrics.
9. Integrate phase gate requests with #102 scheduling/execution primitives and Continuation/Termination/Goal/Plan/Change Set. Failed quality gate pauses/blocks according to policy; material edits invalidate dependent evidence through #102.
10. Реализовать restart/recovery/idempotency: interrupted evaluation is Unknown, last-good snapshot remains historical, duplicate requests do not duplicate transitions, and bounded reevaluation cannot loop forever.

## Fault/recovery matrix

- missing/stale/incompatible evidence → `NeedsVerification`, never Pass;
- metric parse/unit/bounds failure → Unknown/invalid observation;
- current value below ratchet baseline → QualityRegression;
- verifier/metric identity changed → baseline incompatible and rebaseline review;
- policy relaxation/removed gate → anti-bypass finding and NeedsPolicyReview;
- legitimate assertion refactor → review candidate, not automatic hard block;
- expired exception → constraint applies again;
- Core restart during evaluation → Unknown/interrupted, no false readiness;
- repeated correction attempt → bounded stop with evidence-preserving failure.

## Критерии выхода

- [ ] Evaluation is fully Core-owned and consumes #102 evidence only.
- [ ] Phase and changed-scope resolution is conservative and auditable.
- [ ] Ratchets are monotonic and semantic verifier changes require rebaseline.
- [ ] Anti-bypass findings have confidence/evidence classes and do not over-block review candidates.
- [ ] Active autonomous execution cannot lower its pinned quality bar and continue silently.
- [ ] All readiness consumers use one immutable, recoverable snapshot.

## Не входит

Renderer orchestration, arbitrary command execution, new CI backend, automatic code fixes, automatic policy approval, model quality judgment and external telemetry.
