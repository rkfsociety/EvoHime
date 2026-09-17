# План 176.0 — Guided Capability Recipes

Статус: предложено по [issue #154](https://github.com/rkfsociety/EvoHime/issues/154). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Добавить versioned runnable reference recipes, которые композируют уже
существующие workflow templates/packages, model/provider resolver, approvals,
knowledge/context, benchmark/evals, observability и provenance. Recipe — это
inspectable safe composition, а не новый workflow engine, playground или
privileged shortcut.

Текущий checkout содержит Core-owned `workflow_templates`, workflow package/
registry, model routing, RAG/context, approval and benchmark/evaluation
primitives. План добавляет descriptor/preflight/run snapshot и guided
projection поверх них, сохраняя их ownership.

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
- Existing cancellation, durable run/recovery and user-owned draft/package
  boundaries; no parallel execution authority.

### Опциональные

- Provider/model availability and optional local/vector backends; missing
  capability produces `ReadyWithWarnings`/`Blocked`, never silent substitution.

## Критерии готовности

- [ ] Core catalog is versioned, immutable and validates definitions before use.
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

## Связанный issue

- [#154 Guided Capability Recipes](https://github.com/rkfsociety/EvoHime/issues/154)
