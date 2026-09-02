# План 123.0 — Content-Aware Context Compression: type-specific compactors и recoverable originals

Статус: предложено по [issue #103](https://github.com/rkfsociety/EvoHime/issues/103). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Расширить существующий Context Budget Manager Core-owned слоем **Content-Aware Context Compression**. Большой tool/result/data item должен при доказанной пользе превращаться в type-specific compact projection с exact source lineage, explicit omitted regions и bounded typed recovery.

Это content-level optimization для одного большого item, а не новый context manager и не замена `compression.rs`, Context Ledger, ContextRefs, Semantic Repository Map или Prompt Cache Planner.

## Текущее основание и граница

В checkout уже есть Context Budget Manager с pruning, hierarchy ordering, model summarization, deterministic fallback и ledger/provenance; Local Agentic RAG/ContextRefs, Prompt Cache Planner, Adaptive Tool Catalog, ArtifactStore/shadow originals, Model Purpose Routing, Sensitive Data Guardrails и Agent Benchmark Matrix. Новый слой использует эти authorities и не создаёт второй journal/blob store/RAG.

Кандидатные поверхности: `crates/evohime-core/src/content_aware_context_compression.rs`, existing `compression.rs`, context ledger/shadow-originals, classifier/compactor registry, typed recovery command, Prompt Cache/Model Purpose integration, benchmark fixtures, additive IPC и bounded renderer diagnostics. Имена и protocol tags подтверждаются на evidence freeze.

## Архитектурная граница

```text
Raw Context Source / Tool Result
  -> canonical source capture/ref
  -> content classifier
  -> registered type-specific compactor
  -> safety/sensitivity validation
  -> token-benefit gate
  -> CompactContextBlock
  -> existing Context Budget Manager / Prompt Cache / Model Gateway

Compact block + omitted region
  -> typed RecoverContextSlice
  -> authority/sensitivity/retention recheck
  -> bounded exact slice/projection
```

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./123-1-content-aware-context-compression.md)
- [Этап 2 — classifiers, compactors, recovery и runtime](./123-2-content-aware-context-compression.md)
- [Этап 3 — IPC, client projection и diagnostics UI](./123-3-content-aware-context-compression.md)
- [Этап 4 — verification, benchmark, release-evidence и закрытие](./123-4-content-aware-context-compression.md)

## Зависимости

### Блокирующие

- Context Budget Manager, Context Ledger/`context_shadowed_originals`, ContextRefs и existing source/artifact retention.
- Sensitive Data Guardrails, Model Purpose Routing, Prompt Cache Planner, Adaptive Tool Catalog и Model Gateway.
- Existing tool result/terminal/event/artifact provenance, policy/capability/approval/budget/cancellation, SQLite and authenticated IPC.

### Опциональные

- Semantic Repository Map/RAG для code/evidence range вместо агрессивного source compression.
- Agent Benchmark Matrix для paired normal-vs-compressed quality/usage evidence.
- Diagnostics & Support Bundle and Execution Environment Profiles for redacted reports.

## Основной контракт направления

Core вводит versioned `ContextContentClassification`, `ContextCompactorDefinition`, `CompactContextBlock`, `OmittedContextRegion`, `RecoverContextSlice`, `ContextCompressionPolicy`, recovery/usage provenance и token-savings diagnostics.

Classification MVP: `PlainText`, `SourceCode`, `UnifiedDiff`, `Json`, `JsonLines`, `Yaml`, `CsvTsv`, `BuildLog`, `TestOutput`, `Diagnostics`, `SearchResults`, `HtmlText`, `AccessibilityTree`, `Unknown`. Unknown/low-confidence выбирает conservative strategy/original, а не aggressive compaction.

Compactor loss classes: `LosslessReencoding`, `StructurePreservingElision`, `EvidencePreservingProjection`, `SemanticSummary`; provenance/UI обязаны различать их. MVP focus — deterministic log/test output, JSON/JSONL, diff and conservative source/search projections. Changed lines, failures, stack traces, security/diagnostic hits, schema/error fields и exact anchors не могут быть удалены ради savings.

`CompactContextBlock` содержит exact source ref/revision/content hash, classifier/compactor/policy versions, loss class, compact hash, size/token estimates, omitted regions, recovery index, sensitivity и explicit incomplete marker. Original bytes остаются в существующем authorized store; если retention удалил их, состояние — `RecoveryUnavailable`.

Recovery разрешён только через Core typed request (`ExactRegion`, `AroundLocator`, `StructuredPath`, `LineRange`, `NextPage`, `ExpandGroup`, `OriginalBounded`) с current authority/sensitivity/provider/retention checks and bounded tokens/calls. Handle не расширяет grants.

Token-benefit gate учитывает compact overhead, expected recovery overhead, minimum absolute/ratio savings и model/profile compatibility. При `NoBenefit` применяется обычный path. Metrics различают `EstimatedSavings`, `ProviderMeasuredSavings`, `CounterfactualBenchmarkSavings`; recovery cost входит в итоговую оценку.

Protected system/safety/approval/user-constraint/action-digest/verifier-contract classes не проходят lossy compression by default. Untrusted source text и prompt injection остаются data. Security/redaction failure не имеет raw fallback.

## Критерии готовности направления

- [ ] Есть Core-owned classifier/compactor registry и type-specific loss semantics.
- [ ] Compact block имеет exact source revision/hash, omitted regions и visible incomplete marker.
- [ ] Original source сохраняется существующим authority/retention path и адресно восстанавливается при разрешении.
- [ ] Log/test, JSON и diff compaction deterministic и сохраняет critical semantics.
- [ ] Compression включается только после measured/estimated token-benefit gate с overhead.
- [ ] Recovery bounded, typed, authority/sensitivity checked и не даёт arbitrary file access.
- [ ] Existing Context Budget/Ledger/Prompt Cache/ContextRefs lineage не дублируется и сохраняется.
- [ ] Normal-vs-compressed benchmark измеряет quality, tokens, latency, recovery и negative/no-op cases.
- [ ] Protected instruction/security classes и untrusted data boundary не ослабляются.
- [ ] Renderer показывает diagnostics/projection-only, без собственной compression/recovery authority.

## Non-goals первого этапа

Новый context manager, MITM proxy, внешний proxy runtime, фиксированный savings percentage, tokenizer/model-weight changes, secret compression для запрещённого provider, lossy policy instructions, automatic source deletion, universal DSL, RAG/Map replacement и automatic policy tuning по token-sink report.

## Связанный issue

- [#103 Content-Aware Context Compression](https://github.com/rkfsociety/EvoHime/issues/103)
