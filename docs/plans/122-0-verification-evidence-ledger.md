# План 122.0 — Verification Evidence Ledger: workspace-bound proofs, freshness и readiness gates

Статус: предложено по [issue #102](https://github.com/rkfsociety/EvoHime/issues/102). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить Core-owned **Verification Evidence Ledger** — единый контур определения и запуска verification lanes, фиксации typed evidence против точного содержимого workspace/config/environment, безопасного reuse, freshness invalidation и вычисления readiness для task/plan/change/commit/ship/goal targets.

Ledger отвечает на вопрос «какие проверки выполнены, против какого состояния, каким verifier-ом и достаточно ли они актуальны сейчас?», а не просто хранит зелёный флаг.

## Текущее основание и граница

В checkout уже существуют ArtifactStore/Artifact Handoff (`test.evidence/v1`, `review.report/v1`), Agent Benchmark Matrix, execution ledger, Goal criterion evidence, Task Graph/Plan Artifact, Revision-Safe Workspace Files, Task Worktree Isolation, Continuation Policy, Composable Termination, Code Diagnostics и authenticated IPC. Ledger становится authoritative только для verification execution/freshness/readiness и ссылается на эти системы; он не создаёт новый blob store, CI, permission system или output guardrail.

Кандидатные поверхности: `crates/evohime-core/src/verification_evidence_ledger.rs`, workspace fingerprint/freshness resolver, verification runner/provider registry, readiness evaluator, local-storage store/migration, existing continuation/goal/task/change-set integration, additive desktop IPC и Electron Verification UI. Имена, schema revision и IPC tags подтверждаются на evidence freeze по live checkout.

## Архитектурная граница

```text
workspace/config/environment
  -> Verification Lane definition
  -> Verification Run / trusted executor
  -> Verification Evidence
  -> Freshness Resolver
  -> Readiness Policy / Snapshot
  -> task/plan/goal/change-set/continuation consumers
```

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./122-1-verification-evidence-ledger.md)
- [Этап 2 — runner, freshness, readiness и recovery](./122-2-verification-evidence-ledger.md)
- [Этап 3 — IPC, client projection и UI](./122-3-verification-evidence-ledger.md)
- [Этап 4 — verification, release-evidence и закрытие](./122-4-verification-evidence-ledger.md)

## Зависимости

### Блокирующие

- ArtifactStore/Artifact Handoff, execution ledger, authenticated IPC и existing policy/capability/approval/audit boundaries.
- Revision-Safe Workspace Files, Task Worktree Isolation, Workspace State Checkpoints и existing content/revision primitives.
- Continuation Policy, Composable Termination, Persistent Goal, Task Graph, Plan Artifact и Incremental Change Protocol consumers.
- Existing supervised execution/tool/workflow/external-agent providers; no arbitrary model-generated shell authority.

### Опциональные

- Code Diagnostics Feedback Loop, Agent Git Change Sets и Architecture Snapshot для specialized lanes/targets.
- Execution Environment Profiles for environment identity.
- Diagnostics & Support Bundle for redacted evidence export.

## Основной контракт направления

Core вводит versioned `WorkspaceVerificationFingerprint`, `VerificationSourceScope`, `VerificationLaneDefinition`, `RegisteredVerificationCommand`, `VerificationEnvironmentSnapshot`, `VerificationRun`, `VerificationEvidence`, `ReviewerEvidenceClass`, `VerificationReadinessPolicy`, `VerificationRequirement`, `VerificationReadinessSnapshot` и `VerificationOverride`.

Fingerprint является content-oriented: relevant source/config/untracked files учитываются; staged/unstaged и commit/rebase/amend без изменения содержимого не обязаны менять identity; ignored build/cache исключаются по policy. При недоказуемой зависимости применяется строгий whole-workspace fingerprint, а не guessed selective reuse. Worktrees/roots не смешиваются.

Lane kinds bounded (`UnitTests`, `IntegrationTests`, `Build`, `Lint`, `TypeCheck`, `SecurityReview`, `CodeReview`, `ArchitectureReview`, `ManualQA`, `CustomRegistered`). Executor определяется trusted Core registry/adapter/explicit command revision/manual provider; lane kind не превращается в executable.

Run фиксирует lane/executor/environment revisions, before/after fingerprints, status (`Queued`, `Running`, `Passed`, `Failed`, `Cancelled`, `TimedOut`, `Unavailable`, `ProtocolError`, `Invalidated`, `Unknown`) и typed outcome. Only a verified `Passed` evidence may satisfy positive readiness; non-zero/missing/timeout/cancel/malformed/unknown/fingerprint failure are never green.

Freshness requires compatible workspace content, lane/executor/environment/result/freshness policy and required artifact refs. Selective reuse разрешён только Core-deterministic source scope proof. Readiness verdicts: `Ready`, `Blocked`, `NeedsVerification`, `NeedsHumanReview`, `Incomplete`, `Unknown`; required skipped/unavailable remains blocking.

Reviewer evidence stores independence class; same-model self-review or same-family fallback cannot satisfy a policy requiring distinct family/provider. Human override is explicit/auditable and does not create fake PASS.

## Integration и security

Ledger links outputs through existing ArtifactStore/Handoff, stores metadata/freshness, and feeds Continuation/Termination/Goal/Task/Plan/Change Set without duplicating verifier logic. Renderer displays Core projection and requests Run Missing/Re-run Stale/Open Evidence; it never computes Pass/Fresh/Ready.

Workspace/repository/external review text is untrusted data. Registered commands use exact argv and scoped environment, no shell interpolation/arbitrary model string, no secrets in fingerprints/logs. Verifier does not gain capabilities. Unknown external outcome is reconciled, not blindly retried.

## Критерии готовности направления

- [ ] Есть content-oriented workspace fingerprint и scoped reuse с conservative fallback.
- [ ] Есть versioned lane/executor/run/evidence contracts с exact environment/provenance.
- [ ] Required failures, unavailable, skipped, protocol errors и unknown не становятся Passed.
- [ ] Fresh/Stale/Missing/Failed/Unknown/Inapplicable вычисляются Core-side и durable.
- [ ] Readiness policy/snapshot покрывают task/plan/change/commit/ship/goal targets.
- [ ] Continuation/Termination/Goal/Task/Plan/ChangeSet получают evidence gate без дублирования.
- [ ] ArtifactStore/Handoff хранит большие outputs; Ledger хранит authoritative metadata.
- [ ] Retry/correction bounded, human override auditable, recovery fail-closed.
- [ ] Electron показывает lanes/evidence/freshness/readiness projection-only.

## Non-goals первого этапа

Полная CI-платформа, проверка после каждого keystroke, arbitrary shell execution, commit SHA-only freshness, semantic proof любых зависимостей, новый log/blob store, automatic skip, infinite auto-fix loop, cloud telemetry и расширение permissions через verification policy.

## Связанный issue

- [#102 Verification Evidence Ledger](https://github.com/rkfsociety/EvoHime/issues/102)
