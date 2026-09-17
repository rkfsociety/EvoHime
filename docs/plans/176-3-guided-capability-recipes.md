# План 176.3 — Guided Capability Recipes: IPC, guided UI и fork

## Зависимости

### Блокирующие

- [Preflight, run и recovery](./176-2-guided-capability-recipes.md).
- Existing authenticated IPC, workflow/evidence projections, Electron
  navigation and user-owned workflow/package creation paths.

### Опциональные

- Existing Operations/Workbench panels for contextual evidence display.

## Реализация

- Добавить additive typed IPC for catalog, preflight, run status, inspectable
  evidence and fork; use next free command/event tags after proto inventory.
- Add guided flow `Goal -> Inputs -> Preflight -> What will happen -> Run ->
  Inspect evidence -> Compare/modify -> Save draft`, keeping all decisions in
  Core and payloads bounded/redacted.
- Render the flow through existing ordinary Workbench/Operations/workflow
  surfaces; do not add an internal Core/agent/model tab or a renderer-owned
  recipe catalog.
- Показывать expected steps, models/providers, context sources, tools,
  approval points, artifacts, verification contract and capability gaps without
  secrets, hidden reasoning or raw prompt/output.
- Fork successful run into a new user-owned workflow/package draft with recipe
  provenance by reusing the existing package/draft path and explicit
  credential refs only; never copy secrets/grants or publish automatically.

## Критерии

- Renderer cannot alter definition, requirements, resolved revisions, grants or
  approval result; malformed/unknown IPC major fails closed.
- UI distinguishes blocked, warning, running, completed and failed states and
  exposes evidence/provenance sufficient for reproducibility.
- Existing ordinary navigation remains intact; no internal Core/agent/model tab.

## Non-goals

Direct provider calls, renderer-owned recipe catalog, raw transcript display and
automatic mutating actions.
