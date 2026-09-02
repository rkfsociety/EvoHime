# План 124.1 — Project Quality Contract: Core-контракт, schema и storage

Статус: этап 1 для [плана 124.0](./124-0-project-quality-contract.md); issue: [#104](https://github.com/rkfsociety/EvoHime/issues/104). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать bounded contracts, canonical hashes, revision lifecycle, typed metric/baseline semantics, phase/scope policy, exception and anti-bypass records без запуска verifier-ов и без дублирования #102 evidence.

## Зависимости

### Блокирующие

- План 124.0 и Verification Evidence Ledger (#102): lane/evidence/freshness/target semantics.
- Existing policy/audit/event journal, workspace identity, ArtifactStore refs, SQLite migration/backup и optimistic/idempotent stores.

### Опциональные

- Plan Artifact, Goal/Task/Change Set contracts, Code Diagnostics и Architecture Snapshot.

## Реализация

1. Ввести bounded types для contract scope/status/revision/source, dimensions, metric value/unit, comparator/evaluation mode, severity/failure policy, phase/profile, scope selector, constraint and lane/metric refs.
2. Реализовать immutable `Draft -> Active -> Superseded/Invalid/NeedsReview` lifecycle. Material changes получают новую revision и canonical content hash; active run stores exact contract snapshot.
3. Зафиксировать `QualityMetricDefinition` registry: typed value types (`Boolean`, `Integer`, `Decimal`, `DurationMs`, `Bytes`, `Percentage`, `Count`, `SeverityRank`), source evidence kinds, extractor revision/hash и aggregation. Model output без #102 evidence не принимается.
4. Определить `QualityMetricObservation` и `QualityBaseline`: evidence ref, workspace fingerprint, verifier/lane revision, metric definition revision, environment/freshness and accepted actor/time. Baseline нельзя создать из prose/model statement.
5. Определить fixed/range/boolean/presence/ratchet comparisons, precision/unit normalization, missing/stale/incompatible/failed semantics and no implicit pass. Ratchet metadata supports manual acceptance or explicitly enabled auto-tighten only.
6. Зафиксировать phase profiles, required/optional constraints, duration/concurrency budget and missing-evidence behavior. Changed scope (`WholeProject`, `ChangedFiles`, `ChangedLines`, `Paths`, `Component`) must carry a proof/baseline ref or conservatively become stricter/Unknown.
7. Добавить canonical `QualityContractDelta`, `QualityRatchetCandidate`, `QualityException`, `QualityBypassFinding` and `QualityPolicyChangeRequest`; distinguish deterministic violation, deterministic change-needs-approval and review candidate.
8. Добавить transactional metadata storage/migrations for contract revisions, definitions, observations/baselines, phase policies, deltas, findings, exceptions and readiness refs. Reuse #102/artifact refs; do not store raw logs, source, prompts, credentials or executable handles here.
9. Добавить corruption/rollback, size/count/expiry, redaction, idempotency and optimistic version fixtures; preserve audit history and last valid active revision.

## Acceptance-to-contract matrix

- `C01` contract authority → immutable active revision, status, scope, hash and pinned run snapshot.
- `C02` typed metrics → registered extractor, exact evidence/workspace/verifier refs and typed values only.
- `C03` baselines/ratchets → evidence-backed baseline, monotonic direction and incompatible rebaseline.
- `C04` phases/scope → bounded profiles and conservative unknown scope.
- `C05` policy protection → semantic delta, findings, exceptions and explicit transitions.
- `C06` persistence → atomic migration/rollback, no duplicated raw artifact authority and redacted metadata.

## Критерии выхода

- [ ] Invalid/relaxed/unknown contract cannot become active silently.
- [ ] Typed observations require compatible #102 evidence and never accept model-authored metrics.
- [ ] Ratchet data cannot move backward automatically; changed verifier semantics invalidate old baseline.
- [ ] Contract storage is transactional, bounded, recoverable and secret-free.

## Не входит

Workspace scanner, verifier process execution, metric algorithm implementations, readiness orchestration, anti-bypass diff scanning, IPC/UI and autonomous continuation integration.
