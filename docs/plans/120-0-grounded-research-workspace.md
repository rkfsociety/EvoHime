# План 120.0 — Grounded Research Workspace: source-pinned research, citations и reusable evidence

Статус: предложено по [issue #100](https://github.com/rkfsociety/EvoHime/issues/100). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить Core-owned **Grounded Research Workspace** для сбора workspace/uploaded/web/connector материалов, immutable source revisions, bounded evidence retrieval, многошаговых research sessions и цитируемых `ResearchArtifact`.

Research result — не длинный model answer, а versioned artifact с exact source snapshot, evidence lineage, coverage state и machine-readable citations.

## Текущее основание и граница

В checkout уже реализованы Local Agentic RAG (`workspace_rag`, published generations, FTS5/optional embeddings, citation re-read gate), ArtifactStore/Artifact Handoff, Agentic Browser Session, document worker boundary, Context Budget/Refs, Model Purpose Routing, Goal/plan artifacts и security/policy gates. Новый слой расширяет их и не создаёт второй RAG source of truth, memory database, web server или скрытый Python service.

Кандидатные поверхности: `crates/evohime-core/src/research.rs`, `research_fetch.rs`, `research_search.rs`, `research_pipeline.rs`, `research_gate.rs`, `crates/evohime-local-storage/src/research_store.rs`, existing workspace RAG/artifact/browser/document integrations, authenticated IPC, Electron Research UI и canonical docs. Live evidence freeze обязан подтвердить, какие из этих поверхностей уже являются contract, а какие только заготовки.

## Граница сущностей

```text
Workspace/Repository Map = current project tree and derived code index
ProjectArtifact           = generic durable project result
ResearchSource            = logical origin with immutable revisions
EvidenceItem              = human-addressable fragment/fact
KnowledgeCollection       = reusable set of source bindings
ResearchSession           = bounded investigation execution
ResearchArtifact          = grounded synthesis with claims/citations
Conversation Memory       = separate conversational provenance class
```

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./120-1-grounded-research-workspace.md)
- [Этап 2 — ingestion, retrieval, pipeline и recovery](./120-2-grounded-research-workspace.md)
- [Этап 3 — IPC, client projection и UI](./120-3-grounded-research-workspace.md)
- [Этап 4 — verification, release-evidence и закрытие](./120-4-grounded-research-workspace.md)

## Зависимости

### Блокирующие

- Local Agentic RAG/evidence validation and published generation semantics.
- ArtifactStore/Artifact Handoff, browser/document worker boundaries, Context Budget/Refs.
- Model Purpose Routing, Structured Response Contract, execution/tool/network policy, data sensitivity, SQLite migrations, event journal и authenticated IPC.

### Опциональные

- Semantic Repository Map и Workspace Sets для multi-root/code evidence.
- Diagnostics & Support Bundle для redacted research export.
- Typed Context References/Context Mentions, Model Resilience и Execution Environment Profiles для future routing.

## Основной контракт направления

Core вводит versioned `KnowledgeCollection`, `ResearchSource`, immutable `ResearchSourceRevision`, `EvidenceItem`, `ResearchSession`, `ResearchSubtask`, `ResearchClaim`, `ResearchCitation`, `EvidenceConflict`, `ResearchCoverage`, `ResearchArtifact` и `ResearchDelta`.

Source kinds bounded и extensible: `WorkspaceFile`, `WorkspaceSelection`, `UploadedFile`, `PlainText`, `Markdown`, `Pdf`, `WebPage`, `GitHubFile`, `GitHubRepositorySnapshot`, `ProjectArtifact`, `ManualNote`, `ExternalConnectorDocument`. Source revision всегда содержит content hash, media/parser identity, origin snapshot и status; mutable URL не является citation без acquired immutable revision.

Ingestion: `Pending → Acquiring → Parsing → Extracting → Indexing → Ready|PartiallyReady|Failed|Stale → Removing`. Evidence locator обязан вести к human-openable page/section/paragraph/file-line/HTML range/artifact block; chunk id сам по себе не является citation.

Research modes (`QuickResearch`, `DeepResearch`, `Comparison`, `LiteratureReview`, `TechnicalInvestigation`, `LearningNotes`) используют одну bounded pipeline. Session pins source collection/revisions, tool policy, model-purpose snapshots and budget. Source policy (`SelectedOnly`, `SelectedPlusWorkspace`, `SelectedPlusWeb`, `OpenResearchWithinPolicy`) explicitly controls acquisition.

Claim/citation validation проверяет existence, exact source revision, locator, quoted material and stale/deleted state. `VerifiedLocator` подтверждает только адресуемость evidence, не абсолютную истинность claim. Contradictory sources сохраняются обеими сторонами; coverage `Complete/Partial/BudgetLimited/SourceLimited/Failed` не подменяется красивым synthesis.

ResearchArtifact immutable после acceptance; correction/re-run создаёт revision. Accepted artifact может promotion в ProjectArtifact/ContextRef, но memory и evidence остаются разными provenance classes. Старые artifacts не меняются при source drift; re-run создаёт новый artifact и optional ResearchDelta.

## Security и non-goals

Documents, HTML, PDFs, web and connector text — untrusted data, не instructions. Parser не выполняет scripts/macros/repository code; acquisition проходит existing network/connector policy; collection не расширяет workspace grants; LocalOnly запрещает cloud model/embedding. Renderer не выбирает authoritative revision, не строит citation и не подделывает evidence state.

Не входят LMS/образовательная платформа, generic cloud drive, crawling всего интернета, mandatory OCR, arbitrary document code execution, factual-truth guarantee, embeddings as source of truth, automatic indexing вне granted scope и отдельный research runtime/service.

## Критерии готовности направления

- [ ] Есть immutable source/revision/evidence contract с human-openable locators.
- [ ] Knowledge Collections переиспользуют indexed revisions без giant source blob.
- [ ] Ingestion/retrieval/research lifecycle bounded, cancellable и recoverable.
- [ ] ResearchSession pins exact source/model/tool/policy snapshots and budget.
- [ ] Claims/citations/coverage/contradictions machine-readable и validated до artifact completion.
- [ ] Web/tool results snapshot/parse/index до использования как citation.
- [ ] Drift/re-run/ResearchDelta сохраняют старые artifacts неизменными.
- [ ] ProjectArtifact/ContextRef promotion не смешивает memory/evidence authority.
- [ ] Core security/data-boundary, redaction, multi-root grants и prompt-injection handling доказаны.
- [ ] Electron показывает collections/sources/progress/citations/evidence, оставаясь projection-only.

## Связанный issue

- [#100 Grounded Research Workspace](https://github.com/rkfsociety/EvoHime/issues/100)
