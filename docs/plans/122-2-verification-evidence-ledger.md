# План 122.2 — Verification Evidence Ledger: runner, freshness, readiness и recovery

Статус: этап 2 для [плана 122.0](./122-0-verification-evidence-ledger.md); после [плана 122.1](./122-1-verification-evidence-ledger.md).

## Цель

Выполнять trusted verification lanes против exact workspace/environment state, публиковать typed evidence, вычислять freshness/readiness и безопасно восстанавливаться после mutation, crash или restart.

## Зависимости

### Блокирующие

- План 122.1 — contract, storage, identity, trust and readiness semantics.
- Existing supervised execution/providers, workspace file/checkpoint/worktree state, ArtifactStore/Handoff, policy/approval/budget/cancellation, event journal and continuation/goal/task systems.

### Опциональные

- Code Diagnostics, Architecture Snapshot, Agent Git Change Sets and Execution Environment Profiles.

## Реализация

1. Реализовать Core fingerprint builder: canonical granted roots, relevant content/config/dependencies, untracked/staged/unstaged normalization, ignored exclusions, root/worktree identity and strict whole-workspace fallback when scope proof is unavailable.
2. Реализовать lane/executor registry and admission: resolve exact revision, trust hash, argv/cwd/env policy/capabilities/timeout/result contract, manual provider and reviewer independence; no arbitrary shell/model command.
3. Реализовать run lifecycle with before fingerprint, environment snapshot, bounded process/provider execution, cancellation/timeout, after fingerprint and mutation policy. Read-only lane with material before/after difference becomes Invalidated or explicitly pre-state evidence.
4. Реализовать typed outcome ingestion: exit/provider/transport/structured result/verifier identity/fingerprint/artifact checks. Missing executable, non-zero, timeout, cancel, malformed/empty mandatory result, unknown transport and missing fingerprint never yield Passed.
5. Publish successful/failed evidence metadata and bounded ArtifactStore/Handoff refs; redacted logs/output remain under existing sensitivity/retention policy. Unknown external side effect is not blindly retried.
6. Реализовать Freshness Resolver: exact identity reuse, optional deterministic source-scope selective reuse, max age, lane/executor/environment/result-policy/artifact invalidation and stale reason provenance. Model cannot decide unaffected scope.
7. Реализовать readiness evaluator: resolve conditional lanes from changed scope/Plan/Goal/Task/ChangeSet policy, require fresh positive evidence, classify unavailable/skipped/failed/stale/unknown, enforce reviewer independence and produce immutable ReadinessSnapshot.
8. Integrate Continuation Policy/Composable Termination/Goal/Task/Plan/Incremental Change consumers. Evidence gate supplies facts; consumer decides Continue/Verify/Pause/Blocked/override according to its policy. Do not duplicate readiness computation.
9. Implement bounded correction loop and recovery: max attempts/same failure/workspace fingerprint/budget, no infinite retry; restart marks Running unknown/interrupted, reconciles evidence/runs, preserves stale history and last-good readiness/artifact refs.
10. Implement explicit human override admission with actor/reason/unresolved requirements/expiry and audit; never mutate evidence outcome to Passed.

## Fault/recovery matrix

- relevant edit/untracked source → fingerprint changes and dependent evidence stale;
- commit/rebase with same content → evidence remains eligible when content identity equal;
- missing executable/non-zero/timeout/cancel → typed non-Passed outcome;
- malformed reviewer/empty mandatory result → ProtocolError;
- verifier mutates workspace → Invalidated/pre-state evidence, no new-state proof;
- external provider unknown → Unknown/reconcile, no blind retry;
- required lane unavailable/skipped → Blocked/NeedsVerification;
- Core restart in Running → Unknown/interrupted, never Passed;
- same failure repeats → correction loop stops at guard and escalates;
- missing ArtifactStore ref → BrokenEvidenceReference, readiness not Ready.

## Критерии выхода

- [ ] Fingerprint and exact/selective freshness are reproducible and conservative.
- [ ] Trusted runner produces typed evidence with before/after/environment provenance.
- [ ] Required failures/unavailability/unknown/skips block readiness.
- [ ] Conditional lane selection and reviewer independence are Core-owned.
- [ ] Continuation/termination/goal/task/plan/change integrations consume one readiness authority.
- [ ] Restart, mutation, cancellation and correction-loop recovery preserve last-good state.

## Не входит

Unbounded CI, arbitrary commands, automatic formatting inside proof lane, external cloud verifier fleet, renderer orchestration и direct model-generated policy changes.
