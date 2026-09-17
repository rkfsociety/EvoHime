# План 176.0 — Guided Capability Recipes

Статус: active implementation contract. Исторический источник постановки:
issue #154; функционал этим документом не считается реализованным.

## Цель

Добавить versioned runnable guided reference layer поверх уже закрытого
Domain Workflow Recipes contract. Этот слой композирует уже существующие
workflow templates/packages, model/provider resolver, approvals,
knowledge/context, benchmark/evals, observability и provenance. Recipe — это
inspectable safe composition, а не новый recipe registry authority, workflow
engine, playground или privileged shortcut. План не переоткрывает закрытый
план 165, а добавляет guided catalog/preflight/projection над его контрактами.

Текущий checkout содержит Core-owned `domain_workflow_recipes`,
`workflow_templates`, workflow package/registry, model routing, RAG/context,
approval and benchmark/evaluation primitives. План добавляет только
descriptor/preflight/run metadata и guided projection поверх них, сохраняя их
ownership и не создавая второй definition/runtime store.

## Архитектурная граница

~~~text
Core recipe catalog -> capability preflight -> existing workflow snapshot/run
-> existing evidence/provenance -> redacted guided UI -> optional user draft
~~~

Renderer не является источником recipe definition, не исполняет prompt/tools и
не может расширить grants. Exact reproduction pin-ит revisions, а fork создаёт
новый user-owned draft без secrets.

## Этапы

- [Этап 1 — Core catalog, schema и storage](./176-1-guided-capability-recipes.md)
- [Этап 2 — preflight, run и recovery](./176-2-guided-capability-recipes.md)
- [Этап 3 — IPC, guided UI и fork](./176-3-guided-capability-recipes.md)
- [Этап 4 — verification, release evidence и закрытие](./176-4-guided-capability-recipes.md)

## Зависимости

### Блокирующие

- Existing workflow templates/packages/runtime, model resolver, context/RAG,
  tool registry/approval, benchmark/evals, provenance and authenticated IPC.
- Closed Domain Workflow Recipes contract (plan 165) and existing
  `workflow_templates`/`workflow_package`/`workflow_registry` owners; recipe
  descriptors must bind to these contracts instead of defining parallel ones.
- Existing cancellation, durable run/recovery and user-owned draft/package
  boundaries; no parallel execution authority.

### Опциональные

- Provider/model availability and optional local/vector backends; missing
  capability produces `ReadyWithWarnings`/`Blocked`, never silent substitution.

## Критерии готовности

- [ ] Core catalog is versioned, immutable and validates definitions before use.
- [ ] Catalog is an additive guided index over existing workflow/template/
  package definitions; it is not a second execution or definition authority.
- [ ] Implemented initial recipes cover model comparison, prompt variants,
  structured output, tools, RAG, multi-agent review, local fit and trust
  boundaries.
- [ ] Preflight exposes required/optional capabilities and policy blocks.
- [ ] Runs snapshot exact recipe/workflow/provider/model/context/verification/
  policy revisions and remain inspectable after restart.
- [ ] Guided UI is projection-only; mutating tools keep approval and fork never
  copies secrets or grants.
- [ ] Tests and CI evidence cover safety, reproducibility, failure and recovery;
  canonical docs updated before plan files are removed.

## Non-goals

Учебный сайт, второй workflow/benchmark engine, hidden prompt playground,
automatic publication, self-assessment-only quality verdict and permission
escalation.

## Источник постановки

- issue #154 Guided Capability Recipes (исторический идентификатор постановки)
