# План 120.2 — Grounded Research Workspace: ingestion, retrieval, pipeline и recovery

Статус: этап 2 для [плана 120.0](./120-0-grounded-research-workspace.md); после [плана 120.1](./120-1-grounded-research-workspace.md).

## Цель

Реализовать bounded source acquisition/parsing/index reuse, evidence retrieval, quick/deep research pipeline, citation validation, contradiction/coverage tracking и restart-safe recovery.

## Зависимости

### Блокирующие

- План 120.1 — contracts, storage, states, limits and provenance.
- Workspace RAG, ArtifactStore, browser/document worker, Model Purpose Routing, Structured Response, policy/approval/budget/cancellation and event systems.

### Опциональные

- Semantic Repository Map, browser web acquisition, optional local embeddings and ContextRefs.

## Реализация

1. Реализовать source adapters для MVP UTF-8 text/Markdown/source, PDF text layer, workspace files/selections, ProjectArtifacts and browser/web snapshots; unsupported/scanned PDF возвращает typed diagnostic/optional OCR path.
2. Реализовать parser registry and bounded structural output (pages/sections/headings/paragraphs/tables/code/list/captions), no scripts/macros/repository execution. Reuse existing RAG indexes where evidence locator contract suffices.
3. Реализовать acquisition chain `search → acquire → immutable SourceRevision → parse → extract/chunk/index → EvidenceItem`; search snippets and mutable URLs never become citation evidence directly.
4. Реализовать hybrid evidence query over lexical/semantic/structural/metadata modes with deterministic ranking, diversity, source policy, root grants, freshness and human-openable locator checks. Same revision/hash reuses parsed/index results.
5. Реализовать ResearchSession: clarify/decompose → initial retrieval → bounded source gaps/acquisition → subtopic loops → contradiction/coverage pass → synthesis → citation attach → validation → artifact. Limit depth/subtasks/calls/tools/new sources/duration/tokens/cost and support cancellation.
6. Route stages through existing purposes (`ResearchPlanning`, `EvidenceExtraction`, `ResearchSynthesis`, `CitationValidation`) and policy snapshots; source content is data separated from instructions and cannot expand tools/grants.
7. Реализовать claim/citation validator with exact revision/locator/quote checks, `MissingEvidence`, `WeakSupport`, `StaleSource`, `Unsupported`, and `VerifiedLocator`; preserve both sides of conflicts and mark inference separately.
8. Реализовать coverage and artifact acceptance: budget/source-limited partial artifacts retain completed evidence, incomplete final synthesis is not Complete, accepted artifact promotes through existing ProjectArtifact/Handoff path.
9. Реализовать drift/re-run/delta and recovery: source v2 is separate, old artifact remains pinned, sessions restart/reconcile, completed ingestion reused, interrupted jobs recover safely, failed synthesis leaves last valid artifact.

## Fault/recovery matrix

- parser/fetch timeout or crash → Failed/Partial diagnostic, collection survives;
- network/tool unknown outcome → no blind duplicate acquisition, reconcile by request/content hash;
- source changed/deleted/inaccessible → stale/unavailable citation, no substitution;
- budget/cancel → completed evidence/subtasks retained, bounded Partial result;
- Core restart during indexing/research → incomplete work recoverable/interrupted, never falsely completed;
- contradictory sources → conflict record preserves both evidence sets;
- denied root/cloud policy → typed denial, no grant expansion or fallback outside policy.

## Критерии выхода

- [ ] Source revision and evidence locator are deterministic and reusable.
- [ ] SelectedOnly performs no hidden web acquisition; acquired web content is snapshotted before citation.
- [ ] DeepResearch is bounded/cancellable and produces explicit coverage.
- [ ] Citation validation rejects wrong revision/locator/quote and does not claim factual certainty.
- [ ] Drift, rerun, delta, cancellation and restart preserve last-good artifacts and provenance.

## Не входит

Unbounded crawling, mandatory OCR/vision, new vector source of truth, arbitrary parser plugins/code, direct UI orchestration и cloud data-boundary bypass.
