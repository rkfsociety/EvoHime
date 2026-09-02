# План 120.1 — Grounded Research Workspace: Core-контракт, schema и storage

Статус: этап 1 для [плана 120.0](./120-0-grounded-research-workspace.md); issue: [#100](https://github.com/rkfsociety/EvoHime/issues/100). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать source/revision/evidence/collection/session/artifact contracts, locator semantics, trust/provenance, bounds, lifecycle, retention и persistence boundary без запуска ingestion или model pipeline.

## Зависимости

### Блокирующие

- План 120.0 — scope, entity boundary, security и acceptance.
- Local Agentic RAG, ArtifactStore/Handoff, browser/document boundaries, model/tool/network policy, SQLite backup/migration и event journal.

### Опциональные

- Semantic Repository Map/Workspace Sets, ContextRefs и Diagnostics Bundle.

## Реализация

0. Сверить overview с live `research_*`, `workspace_rag`, `research_store`, ArtifactStore, browser/document and IPC code; имеющиеся citation/research contracts переиспользовать, не дублировать.
1. Ввести bounded typed types для collection/source/revision/origin/trust/media/parser, locator/structural unit, evidence item, query, session/mode/policy, subtask, claim/citation/conflict/coverage/artifact/delta и typed errors.
2. Определить immutable identity/hash: source revision keyed by content/origin snapshot, evidence stable within revision, parser/chunker/index profile versioned. Canonicalization excludes volatile fields and raw duplicate source where ArtifactStore is authority.
3. Зафиксировать ingestion/session/artifact states, source retention, stale/deleted/unavailable semantics, selected-source policy, budgets and limits for bytes/files/pages/chunks/results/depth/subtasks/citations.
4. Определить citation contract: claim-to-evidence many-to-many, kinds `DirectSupport/PartialSupport/Context/Contradiction/DerivedFromMultiple`, validation states and locator requirements. Model confidence is signal, not evidence authority.
5. Добавить additive durable storage for collections, source/revision metadata, evidence locators/index refs, sessions/subtasks, claims/citations/conflicts and immutable research artifact metadata. Raw source bytes remain in scoped ArtifactStore/source storage; prompts/credentials are excluded.
6. Add fixtures for source revision immutability, same-hash reuse, locator bounds, unsupported media, stale refs, multi-source claim, contradiction, coverage, retention, redaction, migration rollback/corruption and no prompt injection authority.

## Acceptance-to-contract matrix

- `C01` source identity → immutable revisions/content hash/origin snapshot.
- `C02` evidence → stable id, locator, structural context and source lineage.
- `C03` collection → scoped refs/index policy/retention without content blob duplication.
- `C04` session → source/tool/model/policy snapshots, mode and bounded budget.
- `C05` artifact → claims/citations/coverage/conflicts and immutable revision.
- `C06` security → trust metadata, sensitivity, root grants and redacted persistence.

## Критерии выхода

- [ ] Contract, bounds, canonical hashes, states, locator and citation validation are tested.
- [ ] Storage is additive/transactional with backup/rollback and retention evidence.
- [ ] Unknown/stale/unavailable/unsupported never become Ready, VerifiedLocator or Complete silently.
- [ ] No credentials, raw prompts/outputs or unscoped source bytes enter durable research metadata.

## Не входит

Actual parser/fetch/retrieval execution, research orchestration, IPC/UI and external acquisition.
