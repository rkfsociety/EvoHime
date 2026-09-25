# План 176.0 — Guided Capability Recipes

Статус: active implementation contract. Исторический источник постановки:
issue #154; функционал этим документом не считается реализованным.

## Цель

Добавить versioned Core-owned guided catalog, который направляет пользователя
к существующим workflow и capability owners. Запуск recipe делегируется
только совместимому существующему `WorkflowRuntime` graph/package; capability
owners могут предоставлять готовность и evidence, но становятся runner только
при уже существующей совместимой workflow binding. Catalog не создаёт второй
workflow engine, provider caller, benchmark executor, registry authority или
privileged shortcut. Для отсутствующего binding он показывает typed
`Unsupported`, а не имитирует запуск или результат.

План 165 в текущем checkout предоставляет только versioned metadata record и
SQLite metadata table `domain_workflow_recipes`; он не содержит graph,
executable binding, IPC или runtime consumer. Guided catalog не считает его
исполняемым владельцем и не переоткрывает его контракт.

Подтверждённые исходные owners и ограничения:

| Recipe id | Существующий owner / binding | Подтверждённая граница |
| --- | --- | --- |
| `model-comparison` | `interactive_model_compare_workbench`, `agent_benchmark_matrix` | Workbench — metadata-only; benchmark executors synthetic/fixture-only, живое сравнение моделей ими не подтверждается; без совместимого workflow binding — `Unsupported`. |
| `prompt-variants` | `workflow_optimization_lab`, `agent_benchmark_matrix` | Offline evaluation использует fixture executor; live provider evaluation не заявлять; fixture run можно открыть только через существующий совместимый workflow binding. |
| `structured-output` | `workflow_templates::plan-implement-review` v1, `CHILD_REPORT_SCHEMA` | Показывает typed child reports; не заявляет произвольную schema-constrained provider generation. |
| `tool-use` | `WorkflowRegistry` и обычная policy/approval проверка | Запуск возможен только для конкретного существующего graph/package с валидными tool bindings. |
| `knowledge-grounding` | `workflow_templates::repository-research` v1, `workspace.knowledge` | Это существующий исполняемый workflow; его runtime и context owner сохраняют полномочия. |
| `multi-agent-review` | `workflow_templates::parallel-security-review` v1 | Это security-review workflow, а не общий гарантированный multi-agent executor. |
| `local-model-fit` | `hardware_fit_evidence` и локальные model/calibration owners | Fit evidence не является разрешением выбрать или запустить модель. |
| `trust-boundaries` | `workflow_templates::parallel-security-review` v1, `sensitive_data_guardrails` | Проверки секретов/прав; не заявлять egress enforcement и не предполагать реализацию плана 181. |

Текущие `WorkflowRuntime` snapshots сохраняют graph, входы, policy и node
outputs в своих штатных workflow tables. Recipe metadata не дублирует эти
значения; persistence policy самого workflow runtime этим планом не меняется.
Новый UI использует bounded `WorkflowRunProjection` и не раскрывает generic
workflow event `payload_json`.

## Архитектурная граница

~~~text
Core recipe catalog -> capability preflight -> existing workflow snapshot/run
-> existing evidence/provenance -> redacted guided UI -> optional user draft
~~~

Renderer не является источником recipe definition, не исполняет prompt/tools и
не может расширить grants. Exact reproduction использует исходные immutable
revisions, если их сохраняют владельцы; при отсутствии pin Core возвращает
typed `unavailable`, не подменяя версию на текущую. Fork создаёт новый
user-owned draft без secrets и grants.

## Этапы

- [Этап 1 — Core catalog, schema и storage](./176-1-guided-capability-recipes.md)
- [Этап 2 — preflight, run и recovery](./176-2-guided-capability-recipes.md)
- [Этап 3 — IPC, guided UI и fork](./176-3-guided-capability-recipes.md)
- [Этап 4 — verification, release evidence и закрытие](./176-4-guided-capability-recipes.md)

## Зависимости

### Блокирующие

- Existing workflow templates/packages/runtime, context/RAG, tool registry,
  approval, provenance and authenticated IPC for bindings that use them.
- An existing run/evidence owner for every recipe reported as runnable;
  metadata-only or fixture-only owners must retain those typed availability
  limits.
- Existing cancellation, durable run/recovery and user-owned draft/package
  boundaries; no parallel execution authority.

### Опциональные

- Provider/model availability and optional local/vector backends; missing
  capability produces `ReadyWithWarnings`/`Blocked`, never silent substitution.

## Критерии готовности

- [ ] Core-held catalog has stable versioned descriptors for the eight listed
  recipe ids; definition updates create a new catalog version.
- [ ] Catalog is an additive guided index over existing workflow/template/
  package definitions; it is not a second execution or definition authority.
- [ ] Each descriptor names its exact existing owner and executable binding;
  unsupported adapters are visibly typed `Unsupported`. Fixture-only evidence
  is never described as a live provider/model result.
- [ ] The eight categories remain discoverable: model comparison, prompt
  variants, structured output, tools, knowledge grounding, multi-agent review,
  local fit and trust boundaries.
- [ ] Preflight exposes required/optional capabilities and policy blocks.
- [ ] A recipe run links immutably to the existing workflow run and snapshots
  recipe/binding hashes and bounded provenance references; lifecycle status and
  recovery remain owned by that run owner.
- [ ] Guided UI is projection-only; mutating tools keep approval and fork never
  copies secrets or grants.
- [ ] Recipe metadata/IPC/UI do not duplicate raw inputs, outputs, secrets or
  transcripts; the existing workflow store's persistence contract is unchanged.
- [ ] Module-scoped CI for every touched published module and directly
  necessary documentation gates cover safety, replay pins, failure and recovery;
  no general native/package acceptance is part of this plan.
- [ ] Canonical docs updated before plan files are removed.

## Non-goals

Учебный сайт, второй workflow/benchmark engine, hidden prompt playground,
automatic publication, self-assessment-only quality verdict and permission
escalation.

## Источник постановки

- issue #154 Guided Capability Recipes (исторический идентификатор постановки)
