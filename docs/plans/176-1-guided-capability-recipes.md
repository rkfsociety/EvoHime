# План 176.1 — Guided Capability Recipes: Core catalog, schema и storage

## Зависимости

### Блокирующие

- [Обзор плана 176](./176-0-guided-capability-recipes.md).
- `workflow_templates`, workflow package/registry, capability registry,
  verification/evaluation profiles and existing Core storage/migrations.

### Опциональные

- Existing artifact/provenance references for expected evidence.

## Реализация

- Ввести `CapabilityRecipeDescriptor`, definition и validation contract:
  stable id/version, category/difficulty, bounded inputs, required/optional
  capabilities, workflow/evidence bindings, safe preview and status.
- Зарегистрировать восемь initial built-in definitions as versioned Core data;
  definitions compile/validate against existing workflow graph and tool/role
  registries, not arbitrary prompt blobs.
- Ввести immutable `CapabilityRecipeRun` snapshot with recipe/workflow,
  provider/model/context, verification and policy refs plus input hash/state.
- Persist only bounded metadata/revisions/idempotency/provenance via existing
  storage owner; no raw prompt, secret, transcript or response body.

## Критерии

- Duplicate id/version, invalid graph/binding, oversized input and unknown
  capability requirements fail deterministically.
- Historical definition remains immutable; a built-in update creates a new
  version and never mutates old run snapshots.
- Storage recovery preserves inspectable run metadata without becoming a second
  workflow registry.

## Non-goals

Запуск recipe, UI implementation и evaluator implementation относятся к
следующим этапам.
