# План 117.1 — Architecture Snapshot & Drift Review: Core-контракт, schema и storage

Статус: этап 1 для [плана 117.0](./117-0-architecture-snapshot-drift-review.md); issue: [#97](https://github.com/rkfsociety/EvoHime/issues/97). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать typed contract, canonical serialization/hash, identity matching, evidence provenance, coverage/omission rules, freshness states и persistence decision без запуска extraction или UI.

## Зависимости

### Блокирующие

- План 117.0 — scope, contract, security boundary и acceptance.
- Semantic Repository Map, Artifact Handoff Registry, Incremental Change Protocol, Core policy/root grants, event journal и SQLite migration/backup.

### Опциональные

- Typed Context References/Context Mentions для будущих ref-проекций.
- Diagnostics bundle для release evidence.

## Реализация

0. Сверить issue и overview с live contract, существующими artifact/change-protocol stores, schema version и свободными IPC tags; не создавать второй authority.
1. Ввести bounded Rust-типы для snapshot, components, relationships, boundaries, evidence refs, coverage, omissions, freshness/status, delta и change review. Ограничить counts, lengths, nesting, evidence refs и serialized bytes.
2. Определить canonical JSON/hash без volatile timestamps и без raw source content; зафиксировать schema/version compatibility, root-qualified paths, stable-key algorithm version и typed errors для unknown/oversized/malformed input.
3. Реализовать validation matrix: endpoint existence, relationship endpoints, evidence scope/revision, state transitions, candidate/verified separation, omission binding, compatible snapshot pair и deterministic ordering.
4. Добавить additive durable records только для accepted snapshots, deltas, expected reviews и refresh metadata через существующий artifact/storage boundary. Не хранить prompts, raw source, credentials или process handles.
5. Добавить fixtures для valid/invalid, duplicate, stale, cross-root collision, redaction, uncertain identity, omission expiry, hash stability, migration rollback и corrupt record.

## Acceptance-to-contract matrix

- `C01` versioned snapshot → schema, canonical hash и workspace fingerprint.
- `C02` evidence-backed facts → root-qualified refs, states, confidence и provenance.
- `C03` coverage/omissions → bounded profile, diagnostics и revision-scoped resolution.
- `C04` deterministic delta → stable matching, uncertain matches и typed change classes.
- `C05` expected-vs-actual → review contract, policy verdict и evidence refs.
- `C06` recovery/artifacts → immutable accepted revision, last-good pointer и stale semantics.

## Критерии выхода

- [ ] Contract, bounds, hashes, state transitions и identity matching покрыты тестами.
- [ ] Storage additive/transactional с backup-before-migrate, rollback и corruption evidence.
- [ ] Unknown/candidate/stale/unsupported не повышаются до verified или accepted.
- [ ] Durable records не содержат raw source, secrets, prompts, outputs и executable authority.

## Не входит

Extractor implementation, model synthesis, runtime refresh orchestration, IPC/UI и automatic workspace scanning.
