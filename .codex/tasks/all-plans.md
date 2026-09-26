# Active task: Plans 180–181, 184–188

Objective: implement active EvoHime plans in dependency order. Plan 178 is closed and pushed; Plan 179 is implemented and its stage files have been removed after canonical documentation was updated. Stop after completing Plan 179; wait for Roman before starting Plan 180.

## Current state

- Plan 179 source, Core storage migration, authenticated operations, runtime selection/recovery, Electron metadata UI, canonical docs and release markers are committed as working-tree changes pending task-only commit and push.
- `docs/plans/README.md` now lists only 180–181 and 184–188 as active; Plan 179 contract/state are in architecture/current-state and provisional acceptance in release-evidence.
- Core/UI bundle versions: `0.0.000381` / `0.0.000123`.
- No tests/builds/CI have run locally. After the explicit push requested by Roman, verify exact-SHA Core, UI bundle, Rust docs and module-router workflow outcomes and update release evidence.
- Do not start Plan 180 until Roman explicitly resumes.
