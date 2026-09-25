# План 176.1 — Guided Capability Recipes: Core catalog, schema и storage

## Зависимости

### Блокирующие

- [Обзор плана 176](./176-0-guided-capability-recipes.md).
- `workflow_templates`, workflow package/registry, capability registry,
  verification/evaluation profiles and existing Core storage owners.
- Exact executable binding to an existing `WorkflowRuntime` graph. A
  metadata-only capability record or fixture-only evaluator is not a runnable
  binding.

### Опциональные

- Existing artifact/provenance references for expected evidence.

## Реализация

- Ввести code-held, immutable `CapabilityRecipeDescriptor` как
  metadata/index projection: stable id/version, category/difficulty, bounded
  input schema, required/optional capabilities, exact owner binding, safe
  preview and typed availability. Definitions refer to exact template/package
  revisions and graph hashes; they never duplicate or dynamically mutate a
  workflow graph.
- Зарегистрировать восемь initial descriptors с ids из overview:
  `model-comparison`, `prompt-variants`, `structured-output`, `tool-use`,
  `knowledge-grounding`, `multi-agent-review`, `local-model-fit` и
  `trust-boundaries`. Binding runnable только если он разрешается в
  существующий workflow graph/package, который допускает текущий
  `WorkflowRuntime`. Metadata-only, fixture-only или отсутствующий adapter
  получает typed `Unsupported`; новый provider caller/executor не создаётся.
- Для каждого descriptor проверять graph, tool/role/context-provider
  bindings, budget и subset capabilities до публикации в catalog. Unknown
  version, owner или binding закрывается typed ошибкой; свободные prompt blobs
  не являются definition.
- Ввести immutable recipe-run sidecar, содержащий только recipe id/version/
  hash, resolved template/package revision и hash, `workflow_run_id`, input
  hash, bounded provenance references и idempotency key. State, cancellation,
  event sequence и recovery читаются из существующего workflow run.
- Установить sidecar через существующий `workflow_store::install_schema`
  после таблицы `workflow_runs` в `LocalDatabase::open_internal`; этот owner
  вызывается и для уже открывавшихся баз. Связь recipe/run записывать
  транзакционно вместе с workflow run до запуска drive; повтор idempotency key
  возвращает ту же связь. Не добавлять отдельную миграцию user_version, пока
  таблица принадлежит этому идемпотентному installer path.
- Recipe sidecar не сохраняет raw input/output, prompt, secret или transcript.
  Исходные workflow inputs/policy/graph/node outputs остаются в штатном
  workflow store по его действующему контракту; migration/privacy scope этого
  плана не меняет.

## Критерии

- Duplicate id/version, invalid graph/binding, oversized input and unknown
  capability requirements fail deterministically; every built-in's exact
  binding and availability are inspectable.
- Historical definition remains immutable; a built-in update creates a new
  version and never mutates old workflow snapshots or recipe-run links.
- Workflow-run row, node rows and recipe-run sidecar link commit atomically
  before effects can start; idempotent retry cannot create a second run.
- Storage recovery preserves recipe-to-workflow references while all lifecycle
  and execution state remain owned by `workflow_store`/`WorkflowRuntime`.
- Existing `LocalDatabase::open_internal` installs the new sidecar for both
  upgraded and already-current databases without changing historical data.

## Non-goals

Запуск recipe, UI implementation и evaluator implementation относятся к
следующим этапам.
