# План 176.2 — Guided Capability Recipes: preflight, run и recovery

## Зависимости

### Блокирующие

- [Core catalog и storage](./176-1-guided-capability-recipes.md).
- Existing workflow runtime/templates, model/context resolver, tool policy,
  approval, benchmark/eval and background recovery owners.

### Опциональные

- Local model/hardware-fit and vector/RAG availability.

## Реализация

- Перед запуском выполнять bounded preflight: required/optional capability,
  provider/model/context availability, policy/approval gates and degraded
  paths; states `Ready`, `ReadyWithWarnings`, `Blocked`, `Unsupported`,
  `InvalidDefinition`.
- Instantiate existing workflow graph into immutable recipe metadata snapshot;
  the actual execution remains an existing workflow run and does not create a
  second scheduler or effect owner;
  compare/prompt recipes use existing benchmark substrate, RAG uses existing
  retrieval/context provenance, tools use existing approval and receipts.
- Implement `ReproduceExact`, `ReRunWithCurrentCompatible` and
  `ForkAndModify` as distinct modes. Exact mode rejects unavailable historical
  revision instead of resolving `latest`; fork reuses the existing
  workflow-package/user-draft provenance path rather than creating a second
  fork store.
- Persist bounded lifecycle events and reconcile cancellation/restart through
  existing workflow/background recovery; missing adapter is typed failure, not
  success.

## Критерии

- Recipe cannot grant capabilities, bypass approval, replace required tools or
  silently fall back to a different provider/model/context.
- Run records exact resolved revisions and remains inspectable after crash or
  cancellation.
- Retrieved/tool text is data-not-instructions and cannot alter recipe policy.
- Recipe lifecycle/recovery is subordinate to the existing workflow and
  background execution owners; no parallel recipe lease or scheduler is added.

## Non-goals

Новый execution scheduler, hidden multi-agent memory and automatic workflow
publication.
