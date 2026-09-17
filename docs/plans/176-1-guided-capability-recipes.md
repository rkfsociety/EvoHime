# План 176.1 — Guided Capability Recipes: Core catalog, schema и storage

## Зависимости

### Блокирующие

- [Обзор плана 176](./176-0-guided-capability-recipes.md).
- `workflow_templates`, workflow package/registry, capability registry,
  verification/evaluation profiles and existing Core storage/migrations.

### Опциональные

- Existing artifact/provenance references for expected evidence.

## Реализация

- Ввести `CapabilityRecipeDescriptor` как metadata/index projection над
  существующими `WorkflowTemplate`, `WorkflowPackage` и закрытым plan-165
  contract; definition и validation contract должны ссылаться на их revision/
  hash, а не дублировать их графы:
  stable id/version, category/difficulty, bounded inputs, required/optional
  capabilities, workflow/evidence bindings, safe preview and status.
- Зарегистрировать восемь initial built-in descriptors as versioned Core data;
  descriptors compile/validate against existing workflow graph and tool/role
  registries, not arbitrary prompt blobs. Для отсутствующего underlying
  contract используется typed `Unsupported`, а новый executor не создаётся.
- Ввести immutable `CapabilityRecipeRun` metadata snapshot with
  recipe/workflow, provider/model/context, verification and policy refs plus
  input hash/state; execution remains owned by existing workflow run/snapshot
  authority.
- Persist only bounded metadata/revisions/idempotency/provenance via existing
  storage owner; no raw prompt, secret, transcript or response body.

## Критерии

- Duplicate id/version, invalid graph/binding, oversized input and unknown
  capability requirements fail deterministically.
- Historical definition remains immutable; a built-in update creates a new
  version and never mutates old run snapshots.
- Storage recovery preserves inspectable run metadata without becoming a second
  workflow registry or execution store.

## Non-goals

Запуск recipe, UI implementation и evaluator implementation относятся к
следующим этапам.
