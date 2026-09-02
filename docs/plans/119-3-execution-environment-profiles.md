# План 119.3 — Execution Environment Profiles: IPC, client projection и UI

Статус: этап 3 для [плана 119.0](./119-0-execution-environment-profiles.md); после [плана 119.2](./119-2-execution-environment-profiles.md).

## Цель

Дать Electron и optional CLI bounded surface для list/get/preview/activate/rollback environment profiles, diagnostics и effective revision без переноса composition или activation authority в renderer.

## Зависимости

### Блокирующие

- План 119.2 — resolver, preflight, activation, rollback, events and recovery.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge и Settings/Operations navigation.

### Опциональные

- Context refs and Diagnostics Bundle.
- Existing CLI command dispatcher, если он уже предоставляет тот же Core transport.

## Реализация

1. После проверки highest tag зарезервировать additive commands/results/events для profile list/get/create-new-revision, preview/diff, preflight, activate, rollback, current effective snapshot and history. Сохранить major, frame limits, correlation, idempotency, replay/resync.
2. Core принимает только bounded refs/options/profile revision and expected version; повторно проверяет scopes, owner registries, policy, credentials slots, data-boundary, safe boundary and authorization. Renderer не может отправить trusted state или effective snapshot.
3. Передавать metadata-only projection: profile id/revision/hash prefix, scope, bindings kind/ref/revision/required, derived state, diagnostics, safe boundary, diff categories, activation status and effective snapshot hash. Secret values, raw credentials, prompts, outputs, arbitrary paths and executable config never cross IPC.
4. Связать `ipc_bridge.rs`, shared API, preload/main adapters, replay/resync and optimistic conflict handling. Unknown/pending activation visibly differs from success.
5. Добавить Settings/Operations → Environments: profile list, current profile by scope, Ready/NeedsReview/Degraded/Broken, model/routing summary, external agent, MCP/workbench count, skills/instructions, policy/data-boundary and budget summary.
6. Добавить Preview Changes with added/removed/changed bindings, policy effects, compatibility warnings and required actions; secret slots show only safe metadata.
7. Добавить Activate/Rollback/Duplicate/Edit as new revision and diagnostics actions. UI shows Core-selected boundary and active-run pinning; it does not compute them.
8. Добавить compact run/conversation badge with profile revision/hash and accessible loading/refreshing/denied/stale/degraded states. Optional CLI delegates to identical Core commands.

## Acceptance-to-projection matrix

- `C01` Profile catalog → refs, revisions, scope and derived state.
- `C02` Preview → machine-readable diff, policy effects and required actions.
- `C03` Activation → Core status, boundary, event and effective hash.
- `C04` Rollback/history → immutable activation records and previous refs.
- `C05` Run context → compact exact profile/effective snapshot metadata.
- `C06` Security → authenticated bounded projection with no raw secrets or forged success.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded and replay-safe.
- [ ] Mutations are Core-validated, idempotent/versioned and return typed conflicts/denials.
- [ ] Preview and UI never assemble or activate environment locally.
- [ ] Sensitive values, arbitrary config paths and raw provider payloads are absent.

## Не входит

Direct filesystem/SQLite access, generic third-party config editor, client-side resolver, arbitrary import activation и separate CLI configuration logic.
