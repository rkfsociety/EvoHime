# План 123.1 — Content-Aware Context Compression: Core-контракт, schema и storage

Статус: этап 1 для [плана 123.0](./123-0-content-aware-context-compression.md); issue: [#103](https://github.com/rkfsociety/EvoHime/issues/103). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать classification/compactor/block/omission/recovery/policy contracts, source ownership, loss classes, token gate, protected classes, canonical hashes, bounds и persistence boundary без выполнения compaction.

## Зависимости

### Блокирующие

- План 123.0 — scope, security boundary и acceptance.
- Context Budget/Ledger/shadow originals, ContextRefs, ArtifactStore, Sensitive Data Guardrails, Prompt Cache and Model Purpose contracts.

### Опциональные

- Semantic Repository Map/RAG, Agent Benchmark Matrix and Diagnostics Bundle.

## Реализация

0. Сверить overview с live `compression.rs`, context ledger/shadow storage, ContextRefs, artifact retention and IPC; не создавать второй source/provenance authority.
1. Ввести bounded types для content classification/confidence, structural metrics, compactor definition/version/mode/loss/security class, compact block, omitted region, source/revision/locator, recovery request/strategy, compression policy, benefit decision and diagnostics.
2. Определить source ownership/ref semantics: exact revision/content hash, authorized original store, recovery index, retention state and visible incomplete/RecoveryUnavailable states.
3. Зафиксировать canonical serialization/hash for compact blocks/policies/recovery manifests; deterministic ordering, max bytes/regions/paths/depth/tokens/calls and typed unknown/malformed/oversized errors.
4. Определить lossless/structure-elision/evidence-projection/semantic-summary contracts and protected instruction/security classes. Model-generated summary remains untrusted and cannot validate itself.
5. Define benefit formula with compact overhead, expected recovery cost, min absolute/ratio savings, provider counter availability, fallback mode and model/purpose compatibility. NoBenefit is an explicit non-error outcome.
6. Define typed recovery authority: allowed strategies, source sensitivity/provider policy, exact region hash, bounds, repeated/cascading recovery guards and no arbitrary path/handle input.
7. Add additive metadata storage only where existing ledger/shadow/artifact authority cannot hold it; store policy/compactor revisions, compact lineage/recovery metadata and usage, not a second raw blob store or prompts/secrets.
8. Add fixtures for deterministic hashes, classification, omitted region, source deletion, protected class, no-benefit, estimated/measured distinction, recovery bounds, redaction, migration rollback/corruption.

## Acceptance-to-contract matrix

- `C01` classification/registry → versioned bounded kinds/compactors/loss classes.
- `C02` lineage → source revision/hash, omitted regions and existing store refs.
- `C03` recovery → typed strategies, exact hashes, authority/retention checks.
- `C04` token gate → overhead/savings/NoBenefit and metric distinction.
- `C05` security → protected classes, sensitivity and untrusted data.
- `C06` persistence → metadata-only durable/recoverable contract.

## Критерии выхода

- [ ] Contract, limits, canonical hash, loss classes and benefit gate are tested.
- [ ] Original source authority and retention/RecoveryUnavailable semantics are explicit.
- [ ] Recovery cannot accept arbitrary path/handle or bypass redaction/policy.
- [ ] No sensitive/raw source/prompt/output duplication enters compression metadata.

## Не входит

Actual classifier/compactor algorithms, model calls, runtime recovery, benchmark, IPC/UI and Context Budget integration.
