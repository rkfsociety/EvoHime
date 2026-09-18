# План 179.2 — Resolver и model-call integration

## Изменить

1. Implement Core `PromptStrategyResolver` with fixed filters: task kind,
   frozen route/capabilities, context/call/cost budgets, loadout, privacy/policy,
   evidence freshness, explicit permitted binding/priority and deterministic
   tie-break.
2. Compile prompt composition through existing context builder into separate
   safety/system, role, strategy, user, retrieved/tool-data and output-contract
   boundaries. Strategy cannot grant capabilities or reorder safety policy.
3. Count examples/fragments/follow-ups in existing budget; block or apply only
   declared bounded strategy reduction, never silently truncate safety or
   instruction content.
4. Freeze strategy snapshot before ModelGateway dispatch and add hashes/reason,
   sample count and evidence refs to existing provenance/usage metadata.
5. On route fallback, rerun resolution against the new route and create a new
   snapshot; no online strategy mutation or hidden-reasoning capture.

## Зависимости

### Блокирующие

- Plan 179.1, existing route resolver, context budget/loadouts, structured
  output/tool contracts and model-call provenance.

### Опциональные

- Guided recipe strategy pinning can land with plan 179.3; baseline resolver
  remains usable without recipes.

## Проверка

Resolver tests cover deterministic tie-break, capability mismatch, stale/unknown
evidence, baseline fallback, hint denial, route re-resolution and budget
exhaustion. Security tests prove untrusted data never enters instructions and
strategy never expands grants.

## Rollback и evidence

Keep existing workflow prompt paths as explicit baseline until each call site
has a snapshot gate. If a strategy revision disappears during resume, return
typed `strategy_snapshot_unavailable`, never substitute another revision.
