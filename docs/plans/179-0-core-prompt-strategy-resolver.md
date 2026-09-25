# План 179.0 — Core-owned prompt strategy profiles and resolver

Статус: active implementation contract. Исторический источник постановки:
issue #157; функционал этим документом не считается реализованным.

## Цель

Ввести versioned immutable `PromptStrategyProfile` и deterministic fail-closed
resolver, который выбирает способ сборки model-call по task kind, frozen model/
provider capability, context budget, loadout, privacy/policy и compatible
evaluation evidence. Resolver соединяет существующие owners, но не становится
вторым workflow/benchmark engine или prompt playground.

## Текущее основание

В checkout уже зарегистрированы `workflow_optimization_lab.rs`,
`agent_benchmark_matrix.rs`, `context_budget.rs`, `context_loadouts.rs`,
model routing и guided recipes, контракт которых зафиксирован в
[`../architecture.md`](../architecture.md#guided-capability-recipes-v1). Нужно вынести strategy identity,
compatibility и selection snapshot из hardcoded workflow strings, сохранив
workflow orchestration, gateway, context and evaluation ownership.

## Граница

```text
task/role -> PromptStrategyResolver
-> registry + route/capability + budget/loadout + policy + evidence
-> frozen strategy snapshot -> existing context builder/ModelGateway
-> provenance + bounded projection
```

V1 поддерживает direct, few-shot, decomposition, retrieval-grounded, tool-use,
structured-output и bounded multi-sample strategies без hidden chain-of-thought.

## Этапы

- [Этап 1 — profile registry, assets и storage](./179-1-core-prompt-strategy-resolver.md)
- [Этап 2 — resolver и model-call integration](./179-2-core-prompt-strategy-resolver.md)
- [Этап 3 — evaluation, promotion и IPC](./179-3-core-prompt-strategy-resolver.md)
- [Этап 4 — verification и closure](./179-4-core-prompt-strategy-resolver.md)

## Зависимости

### Блокирующие

- Existing ModelGateway/routing and provider capability snapshots, context
  budget/loadouts and structured output/tool contracts.
- Existing Agent Benchmark Matrix and Workflow Optimization Lab; they remain
  the only evaluation/candidate-search owners.
- Guided Capability Recipes contract в
  [`../architecture.md`](../architecture.md#guided-capability-recipes-v1) для
  strategy references в recipes.
- Existing SQLite/domain facade, provenance and authenticated IPC.

### Опциональные

- Completed provider-profile freshness in
  [`../architecture.md`](../architecture.md) can improve compatibility evidence
  but does not create a second strategy-registry dependency.

## Критерии готовности

- [ ] Immutable profiles, bindings, example descriptors and snapshots have
  canonical hashes, bounded fields and explicit lifecycle.
- [ ] Resolver order and tie-break are deterministic; missing promoted strategy
  uses the declared baseline, never an online heuristic or silent mutation.
- [ ] Strategy cannot override safety policy, grants or data/instruction trust
  boundaries; reusable examples require provenance/privacy validation.
- [ ] Benchmark/promotion uses frozen compatible suite/policy, holdout and
  security/cost/latency gates, with explicit Core promotion.
- [ ] Active runs retain exact strategy revision; recovery fails closed on a
  missing/corrupt historical revision.
- [ ] IPC/UI exposes bounded metadata and reason/evidence, not raw prompts or
  hidden reasoning; focused tests and canonical docs are updated.

## Source issue

- [issue #157](https://github.com/rkfsociety/EvoHime/issues/157) Core-owned prompt strategy profiles and evidence-based resolver.
