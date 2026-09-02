# План 122.1 — Verification Evidence Ledger: Core-контракт, schema и storage

Статус: этап 1 для [плана 122.0](./122-0-verification-evidence-ledger.md); issue: [#102](https://github.com/rkfsociety/EvoHime/issues/102). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать fingerprint/source scope, lane/executor/run/evidence/readiness contracts, independence, freshness, override, limits, canonical hashes и durable boundary без запуска verifier-ов.

## Зависимости

### Блокирующие

- План 122.0 — scope, authority boundary, security и acceptance.
- ArtifactStore/Handoff, workspace/revision/checkpoint/worktree primitives, policy/audit/event journal и SQLite migration/backup.

### Опциональные

- Architecture Snapshot, Code Diagnostics и Agent Git Change Sets.
- Execution Environment Profiles and Diagnostics Bundle.

## Реализация

0. Сверить overview с live execution ledger, artifact/goal/task/change/continuation contracts, current schema and IPC tags; existing evidence must be reused where authoritative.
1. Ввести bounded types для workspace ref/fingerprint/source scope, lane kind/revision, executor/command/provider, environment snapshot, run/status/outcome, evidence class, reviewer independence, readiness target/policy/requirement/snapshot/override and typed errors.
2. Определить content identity algorithm and normalization: include relevant files/config/dependencies/untracked source; exclude policy-approved ignored cache/build; commit metadata alone is not content. Store root/worktree identities and scope hash.
3. Определить lane/executor trust contract: exact argv/cwd scope/env policy/capabilities/timeout/output policy/trust hash, manual provider and model-review independence; command revision change requires review.
4. Зафиксировать canonical serialization/hash, immutable revisions, idempotency/optimistic version, max sizes/counts, freshness policy and state transitions. Unknown/malformed/oversized data fail closed.
5. Define evidence outcome/readiness semantics: only Passed + compatible Fresh evidence satisfies required positive requirement; unavailable/skipped/unknown/protocol/failed/stale are explicit non-success. Human override cannot forge evidence.
6. Add additive storage for lane/command revisions, runs, evidence metadata, freshness inputs, readiness policies/snapshots, overrides and artifact refs. Do not duplicate large outputs, source content, secrets, prompts, credentials or live handles.
7. Add fixtures for clean/dirty/untracked/ignored fingerprint, scope, same-content commit, worktree isolation, command trust, invalid outcomes, reviewer classes, readiness, redaction, migration rollback/corruption and bounded retention.

## Acceptance-to-contract matrix

- `C01` fingerprint → content identity/scope/root/normalization contract.
- `C02` lanes/executors → versioned trusted definitions and exact environment.
- `C03` run/evidence → typed statuses, outcomes, provenance and fail-closed rules.
- `C04` freshness → exact/selective reuse, age and invalidation semantics.
- `C05` readiness → policies, requirements, verdicts and override distinction.
- `C06` persistence → durable metadata, artifact refs, idempotency and recovery records.

## Критерии выхода

- [ ] Contract, bounds, hashes, identity and transitions are tested.
- [ ] Storage is additive/transactional with backup/rollback/corruption evidence.
- [ ] Required non-success outcomes cannot be represented as Passed/Ready.
- [ ] Durable data excludes secrets, raw source/transcripts/output and executable authority.

## Не входит

Actual workspace scanner, process/provider runner, freshness execution, readiness orchestration, IPC/UI and continuation integration.
