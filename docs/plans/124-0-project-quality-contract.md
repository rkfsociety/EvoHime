# План 124.0 — Project Quality Contract: versioned constraints, ratchets и anti-bypass gates

Статус: предложено по [issue #104](https://github.com/rkfsociety/EvoHime/issues/104). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue фиксирует исполнимый план, а не готовность функционала.

## Цель

Добавить в EvoHime Core-owned **Project Quality Contract** — версионный контракт, который определяет измеримый quality bar проекта, связывает его с зарегистрированными verification lanes и typed metrics, размещает проверки по lifecycle phases и не позволяет autonomous execution тихо ослабить требования.

Принцип:

```text
Verification доказывает, что произошло.
Quality Contract определяет, достаточно ли этого.
```

Это слой над [Verification Evidence Ledger (#102)](./122-0-verification-evidence-ledger.md), а не новый test runner, CI, permission system или набор prose-инструкций.

## Текущее основание и граница

В checkout уже есть Core/runtime policy, Continuation Policy, Plan Artifact, Task/Goal/Change Set consumers, ArtifactStore, authenticated IPC и план Verification Evidence Ledger. Отдельных `ProjectQualityContract`, typed metric registry, ratchet evaluator или anti-bypass guard в коде не обнаружено. Новый слой использует #102 как единственного владельца выполнения и evidence freshness, а сам владеет только quality requirements, evaluation и readiness verdict.

Кандидатные поверхности: `crates/evohime-core/src/project_quality_contract.rs`, local-storage contract/revision store, existing verification/continuation/plan/goal/change-set integrations, additive desktop IPC и Electron Quality surface. Точные имена schema revision, protocol tags и существующие adapters подтверждаются на evidence freeze.

## Архитектурная граница

```text
Project/workspace
  -> ProjectQualityContract Registry
  -> applicable constraints + phase profile
  -> #102 Verification Evidence Ledger
  -> typed Metric Extractor Registry
  -> threshold / ratchet / anti-bypass evaluation
  -> QualityReadinessSnapshot
  -> continuation / plan / task / commit / ship consumers
```

Core является authority. Renderer только показывает проекцию и создаёт предложения; модель может предложить constraint или policy transition, но не может сама активировать relaxation и продолжить run.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./124-1-project-quality-contract.md)
- [Этап 2 — metric evaluation, ratchets и anti-bypass runtime](./124-2-project-quality-contract.md)
- [Этап 3 — IPC, client projection и Quality UI](./124-3-project-quality-contract.md)
- [Этап 4 — verification, release evidence и закрытие](./124-4-project-quality-contract.md)

## Зависимости

### Блокирующие

- [Verification Evidence Ledger (#102)](./122-0-verification-evidence-ledger.md): registered lanes, trusted execution, typed evidence, workspace/environment identity и freshness.
- Existing Core policy/capability/approval/audit, SQLite migrations/backup, ArtifactStore и event/recovery primitives.
- Continuation Policy, Persistent Goal, Plan Artifact, Task/Change Set и ship/deploy readiness consumers.

### Опциональные

- Code Diagnostics, Architecture Snapshot, Agent Git Change Sets, Agent Benchmark Matrix, Execution Environment Profiles и Diagnostics Bundle.
- Agent Skills metadata как recommendation-only input, без policy authority.

## Основной контракт направления

Versioned Core types: `ProjectQualityContract`, `QualityConstraint`, `QualityMetricDefinition`, `QualityMetricObservation`, `QualityPhaseProfile`, `QualityScopeSelector`, `QualityBaseline`, `QualityRatchetCandidate`, `QualityContractDelta`, `QualityBypassFinding`, `QualityException`, `QualityReadinessSnapshot` и `QualityPolicyChangeRequest`.

Active revision immutable; material edit создаёт новую revision. Run pins `contract_id`, `revision` и `content_hash`. Required constraints считаются выполненными только compatible Fresh Passed evidence из #102 и authoritative typed metric; model-generated numbers не являются evidence.

Поддерживаются `FixedThreshold`, `NonRegressionRatchet`, `BooleanPass`, `PresenceRequired` и bounded `Range`; начальные dimensions включают Build, tests, typecheck, lint, coverage, security, accessibility, performance, bundle size, architecture, mutation и registered custom checks. Exact executable identity разрешается через #102 lane registry, а не через raw model string.

## Readiness и защита

Phase profiles размещают дешёвые проверки в `EditFast`, полные проверки в `TaskComplete`/`Review`, а release checks в `Commit`/`Ship`/`Deploy`; запуск остаётся у существующих verification primitives. `QualityReadinessSnapshot` различает `Ready`, `ReadyWithExceptions`, `NeedsVerification`, `QualityRegression`, `NeedsPolicyReview`, `Blocked` и `Unknown`.

Semantic contract delta классифицируется как `Tightening`, `Equivalent`, `Relaxation`, `Mixed` или `Unknown`. Relaxation, удаление gate, снижение threshold/scope, добавление skip/ignore/suppression и расширение exception создают anti-bypass finding или policy review; active run не продолжает автономно после material relaxation без explicit разрешения. Review candidate не объявляется детерминированным нарушением без достаточного evidence.

## Критерии готовности направления

- [ ] Versioned Core-owned contract и immutable active revisions.
- [ ] Constraints привязаны к registered #102 lanes и typed metrics, а baseline — к exact evidence/workspace/verifier identity.
- [ ] Fixed thresholds, boolean/presence checks, ranges и non-regression ratchets не допускают auto-lowering.
- [ ] Phase-aware requirements и conservative changed-scope evaluation работают поверх существующих execution primitives.
- [ ] Semantic delta и anti-bypass findings обнаруживают ослабление gate без блокировки legitimate review candidates.
- [ ] Exceptions bounded/auditable и не превращают failed/stale evidence в Passed.
- [ ] Один Core readiness verdict доступен task/plan/goal/continuation/commit/ship consumers.
- [ ] Renderer остаётся projection-only, а diagnostics не раскрывают secrets/raw logs.

## Non-goals первого этапа

Новая CI/CD платформа, обязательный одинаковый bar для всех языков, проверка после каждого keystroke, arbitrary shell execution, silent dependency installation, model authority по metrics, универсальный regex-parser stdout, automatic lowering/rebaseline и превращение Agent Skills в policy authority.

## Связанный issue

- [#104 Project Quality Contract](https://github.com/rkfsociety/EvoHime/issues/104)
