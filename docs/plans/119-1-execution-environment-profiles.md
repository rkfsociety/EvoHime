# План 119.1 — Execution Environment Profiles: Core-контракт, schema и storage

Статус: этап 1 для [плана 119.0](./119-0-execution-environment-profiles.md); issue: [#99](https://github.com/rkfsociety/EvoHime/issues/99). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать typed composition contract, binding/reference semantics, scope/layering, state/preflight diagnostics, safe-boundary rules, canonical hashes, optimistic revisions и durable activation history без выполнения переключения.

## Зависимости

### Блокирующие

- План 119.0 — scope, entity boundary, security и acceptance.
- Existing Model/Backend, Customization, Workbench/MCP, Skill, Instruction, Policy, Credential Slot и External Agent contracts.
- SQLite backup/migration, event journal, Core authorization и artifact/provenance boundaries.

### Опциональные

- Local Model Runtime Manager и Diagnostics Bundle.
- Typed Context References.

## Реализация

0. Сверить overview с live registries/stores, schema version, layering rules, current highest IPC tags и actual activation/provenance paths; не создавать второй authority.
1. Ввести bounded types для profile/scope/binding kind/ref mode/required flag/options, derived states, diagnostics, diff, activation, rollback и effective snapshot. Ограничить ids, refs, lists, nesting и serialized bytes.
2. Зафиксировать layering precedence между Application/Workspace/Project/ConversationDefault и конфликт resolution. Profile options не могут записать raw model/MCP/skill/policy bodies.
3. Определить canonical JSON/hash для profile, diff и effective snapshot; volatile timestamps и secret values исключить. Unknown kind/version, dangling ref, duplicate binding и oversized input дают typed fail-closed error.
4. Описать compatibility matrix: required/optional, pinned/follow-compatible, binding-specific safe boundary, provider/model capabilities, MCP/workbench restart, policy/data sensitivity and credential slot resolution.
5. Добавить additive durable storage для immutable profile revisions, current scope binding, activation/rollback audit and resolved metadata. Не хранить secrets, prompts, outputs, process handles или duplicate owner state.
6. Добавить fixtures для valid composition, conflicts, stale revisions, scope mismatch, missing required/optional, pinned drift, redaction, hash stability, import/export slot metadata, migration rollback и corruption.

## Acceptance-to-contract matrix

- `C01` composition → typed refs, scopes, layering and immutable revisions.
- `C02` required/optional → all-or-nothing versus explicit Degraded.
- `C03` profile state → Ready/NeedsReview/Degraded/Broken from authoritative refs.
- `C04` preflight → diagnostics and safe-boundary classification.
- `C05` effective snapshot → exact resolved refs/hash and provenance contract.
- `C06` activation history → audit, rollback and optimistic/idempotent mutation.

## Критерии выхода

- [ ] Contract, bounds, canonical hashes, states, layering and errors are tested.
- [ ] Durable records are additive/transactional, backup-protected and rollback-tested.
- [ ] No profile field can carry secrets, arbitrary executable/config paths or capability grants.
- [ ] Pinned/follow-compatible and missing/stale refs have explicit non-success outcomes.

## Не входит

Reference resolution against live registries, activation runtime, process/workbench restart, IPC/UI и external effects.
