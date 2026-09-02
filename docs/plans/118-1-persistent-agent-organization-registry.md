# План 118.1 — Persistent Agent Organization Registry: Core-контракт, schema и storage

Статус: этап 1 для [плана 118.0](./118-0-persistent-agent-organization-registry.md); issue: [#98](https://github.com/rkfsociety/EvoHime/issues/98). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать durable identity contract, lifecycle, reporting/goal binding invariants, assignment provenance, canonical hash, optimistic revisions, privacy limits и storage boundary без запуска execution.

## Зависимости

### Блокирующие

- План 118.0 — scope, entity boundary, security и acceptance.
- Persistent Goals, Agent Role Profiles, TeamSession/SOP, coordination/handoff, artifact registry, Core policy/approval, event journal и SQLite backup/migration.

### Опциональные

- Model Purpose Routing для profile selection metadata.
- Diagnostics bundle для derived activity export.

## Реализация

0. Сверить overview с live modules, current storage schema, goal/profile/team records и свободными IPC tags; если binding уже существует, собрать evidence вместо второй authority.
1. Ввести bounded types для `PersistentAgent`, organization scope, lifecycle, responsibility kinds, `AgentGoalBinding`, `AgentAssignment`, reporting edge, execution/accountability snapshot, availability и typed errors.
2. Зафиксировать identity key, display-name limits, normalized scope, allowed status transitions, role/profile reference semantics, goal revision pinning и cycle/scope validation. Unknown enum/version/oversized input — fail closed.
3. Определить canonical serialization/hash и audit event payload без credentials, prompts, outputs, raw transcripts или capability grants. Revision increments atomically; stale expected revision даёт typed conflict.
4. Добавить additive transactional storage для immutable agent revisions, current pointer, reporting edges/history, goal bindings, assignments и audit metadata. Derived activity не дублировать в registry; durable availability не хранить как ручную зелёную лампочку.
5. Определить deletion/retirement policy: no destructive delete for identities with history; retired records remain queryable; orphaned role/goal refs become explicit broken binding.
6. Добавить fixtures: valid lifecycle, duplicate identity, stale revision, invalid scope/cycle, retired assignment, exact goal revision, redaction, hash stability, migration rollback/corruption и no-secret persistence.

## Acceptance-to-contract matrix

- `C01` durable identity → immutable revisions, stable id and lifecycle.
- `C02` hierarchy → acyclic scope-compatible reporting edges and audit history.
- `C03` goal ownership → exact revision bindings and role enum without copied Goal state.
- `C04` assignment → typed source refs and provenance, no direct execution authority.
- `C05` accountability → bounded immutable snapshot and content hash.
- `C06` safety → no implicit capability inheritance, secret storage or mutable active-run identity.

## Критерии выхода

- [ ] Types, bounds, hashes, transitions and validators are covered by tests.
- [ ] Storage migration is additive/transactional with backup and rollback evidence.
- [ ] Reporting cycles, cross-scope edges, stale revisions and retired assignments fail closed.
- [ ] Durable records contain metadata only and do not duplicate goals, tasks, costs or credentials.

## Не входит

Runtime assignment execution, model/profile selection, run reconciliation, IPC/UI и derived activity aggregation.
