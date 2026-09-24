# План 188.0 — Core-owned research experiment tree и bounded autoresearch loop

Статус: active implementation contract. Исходный issue #164 удаляется после сохранения плана; план не означает, что функциональность уже реализована.

## Цель

Расширить metadata-only autonomous_metric_experiment_runtime до bounded campaign/node/run lifecycle: изолированный task worktree, exact recorded commit/content-addressed source snapshot, existing invocation owner и frozen typed metrics.

## Основание checkout

autonomous_metric_experiment_runtime.rs/store и task_worktree_isolation.rs/store сейчас задают metadata contracts, а не полноценный executor. ArtifactStore, child/task runtime, versioned invocation owners, Benchmark Matrix, evidence, resource pressure, PolicyGate и receipts — интеграционные owners; их следует расширить без параллельного runtime.

## Граница

Frozen baseline → bounded hypotheses/tree nodes → existing Task Worktree/Git owner → recorded commit/source snapshot → existing invocation owner → typed evidence → deterministic compatible comparison → Candidate/Reject/NeedsMoreEvidence/Stop.

## MVP сценарии

- Оптимизация производительности: baseline commit → isolated node/worktree → recorded change → existing benchmark invocation → structured latency/memory metric → compatible comparison.
- Сравнение agent/prompt strategy: sibling hypotheses на одном frozen baseline, одинаковый Agent Benchmark Matrix suite и сопоставимые attempts.
- Research-backed изменение: source/evidence ref может обосновать hypothesis, но только frozen run metric/evidence подтверждает улучшение.

Candidate проходит существующий review/verification/promotion path; research runtime сам не делает merge, push или публикацию.

## Изменяемые контракты

- Campaign, hypothesis, node, run snapshot, frozen metric/evaluator, hard budgets, lineage, idempotency and fencing.
- Additive lifecycle extension to existing task worktree, invocation, artifact, benchmark and evidence owners.

## Recovery и rollback

Каждый Git/archive/dispatch/evaluation boundary имеет durable state и deterministic reconciliation. Unknown/partial outcomes не улучшают baseline. Rollback останавливает admissions и сохраняет committed lineage; cleanup не удаляет source commits или evidence.

## Этапы

- [Этап 1 — campaign/node/run/metric contracts and lineage](./188-1-core-research-experiment-tree.md)
- [Этап 2 — worktree snapshot, invocation dispatch, budgets/recovery](./188-2-core-research-experiment-tree.md)
- [Этап 3 — comparison, bounded projections and explicit promotion](./188-3-core-research-experiment-tree.md)
- [Этап 4 — Git/runtime/security recovery evidence and closure](./188-4-core-research-experiment-tree.md)

## Зависимости

### Блокирующие

- Task Worktree Isolation/Git/change-set, ArtifactStore, child/task runtime and durable recovery; verify and extend real worktree lifecycle through current owners.
- Versioned Workflow/Invocation/Recipe owner, Benchmark Matrix, evidence ledger and resource pressure.
- PolicyGate, approval, receipts, authenticated IPC and explicit promotion/review path.

### Опциональные

- Планы 184/185 могут поставлять trace/scenario evidence refs, но не блокируют campaign lifecycle.
- Plan 179 strategy profiles and grounded research refs are optional hypothesis evidence.

## Критерии готовности

- [ ] Campaign/hypothesis/node/run имеют immutable hash lineage, idempotency/fencing и hard limits на depth, nodes, runs-per-node, parallelism, wall clock, token/cost, CPU/GPU, no-improvement patience, consecutive failures и cancellation.
- [ ] Authoritative run связан с clean exact commit и content-addressed snapshot; dirty source rejected.
- [ ] Execution только через versioned invocation owner; sibling worktrees/approvals isolated; raw shell запрещён.
- [ ] Metrics frozen/typed; partial/unknown/unavailable/incompatible/hard-constraint failure не считается improvement.
- [ ] Crash recovery проходит Git/archive/dispatch/evaluation boundaries без duplicate effects/nodes.
- [ ] Candidate только review-only: no auto merge/push/publish; projections bounded and secret-free.

## Non-goals

- Второй Git/workflow/benchmark engine, unbounded self-improvement, mandatory cloud/Kubernetes/Slurm/Ray.
- Хранение source bytes/datasets/weights/prompts/full logs/provider transcripts в experiment tables; auto merge/push/PR.

## Verification

Плановые gates: lifecycle/hash/budget unit checks → isolated Windows Git/worktree and invocation tests → comparison/artifact/security checks → crash matrix, parallelism and stop-condition CI evidence.

## Release evidence

Сохранять campaign/node/run/baseline/metric contract hashes, exact commit/source snapshot and invocation refs, evaluator result, reason codes and budgets. Source archives/logs remain in their existing owners.

## Критерии выхода overview

- [ ] Actual Git/worktree executor owner и exact commit boundary проверены до запуска этапа 2.
- [ ] Hard stop conditions, explicit promotion and no-dirty-source rule согласованы до этапа 1.

## Исходная постановка

- [Issue #164 — Core-owned research experiment tree и bounded autoresearch loop](https://github.com/rkfsociety/EvoHime/issues/164); сохранённый номер исходной постановки, issue удаляется после публикации плана.
