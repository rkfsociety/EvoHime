# План 123.2 — Content-Aware Context Compression: compactors, recovery и runtime integration

Статус: этап 2 для [плана 123.0](./123-0-content-aware-context-compression.md); после [плана 123.1](./123-1-content-aware-context-compression.md).

## Цель

Реализовать deterministic type-aware compaction для крупных data/tool results, token-benefit admission, exact recovery и встраивание в существующий Context Budget/Prompt Cache/Model Gateway pipeline.

## Зависимости

### Блокирующие

- План 123.1 — contracts, storage, loss/security/recovery semantics.
- Context Budget Manager/Ledger, ContextRefs, ArtifactStore/shadow originals, tool result capture, Sensitive Data Guardrails, Model Purpose Routing, Prompt Cache and policy/budget/cancellation.

### Опциональные

- RAG/Map exact range retrieval, registered tool compression hints, local tokenizer and Agent Benchmark Matrix.

## Реализация

1. Реализовать Core classifier с confidence/structural metrics and conservative Unknown behavior; registered tool hint only narrows/helps classification and cannot weaken policy.
2. Реализовать compactors: log/build/test output preserve failures/errors/stacks/commands/counters and bounded first/last regions; JSON/JSONL preserve schema/errors/distinct records/counts/ranges; diff preserves headers/hunks/changed lines/context; search preserves diagnostic/security-priority hits; source code uses signatures/imports/exact ranges, not random invalid truncation.
3. Реализовать deterministic lossless/structure/evidence projections, visible omitted markers, exact region locators/content hashes and validation before acceptance. Semantic summarizer uses `Compaction/Summarization` purpose only where policy permits.
4. Integrate source capture with existing ContextItem/Ledger/ArtifactStore/shadow original/ContextRef lineage; never destroy source of truth or duplicate giant blobs. Apply sensitivity/redaction/retention in correct order and keep external content untrusted.
5. Implement token-benefit admission before replacing original: estimate original/compact/overhead/recovery, compare policy thresholds, choose NoBenefit/original when economics are negative. Stable compact hash feeds Prompt Cache Planner.
6. Implement typed `RecoverContextSlice`: resolve exact source revision, recheck run authority/provider/sensitivity, materialize bounded requested region as ContextItem/ContextRef, record recovery usage/provenance, enforce calls/tokens/depth/repeated-region limits.
7. Insert content stage before existing Context Budget selection and preserve conversation-level compression as a separate later lineage step. Optional failure falls back to ordinary bounded path; redaction/security failure fails closed.
8. Add prompt/tool integration: protected system/safety/approval/user constraints remain lossless; tool results may use registered hints; compactor never interprets logs/JSON/HTML as instructions.
9. Add runtime recovery/retention behavior: source deletion yields RecoveryUnavailable, compactor failures preserve original path, model-request lineage records normal → compressed → recovered → summary transitions.
10. Add token-sink and compression usage metadata report (read-only) with negative/no-op cases, provider counters when available and no automatic policy mutation.

## Fault/recovery matrix

- classifier/parser/compactor failure → typed failure and ordinary context fallback;
- no benefit → original projection, no false savings;
- source revision/retention mismatch → stale/RecoveryUnavailable, no substitution;
- sensitivity/redaction failure → fail closed, never raw fallback;
- malformed compact manifest/decode → reject and preserve original;
- repeated/cascading recovery → bounded stop and explicit incomplete state;
- model/provider unavailable → deterministic compactor only or ordinary policy path;
- Core restart → source/compact lineage recovered from existing durable metadata, in-flight recovery not falsely completed.

## Критерии выхода

- [ ] Log/test, JSON and diff compactors preserve required anchors and are deterministic.
- [ ] Benefit gate includes overhead/recovery and distinguishes measured/estimated savings.
- [ ] Recovery returns exact bounded source regions only under current authority.
- [ ] Existing Context Budget/Ledger/Prompt Cache lineage remains coherent.
- [ ] Protected instructions and sensitivity policy cannot be compressed/bypassed accidentally.
- [ ] Failure/no-benefit paths do not break normal model calls or claim savings.

## Не входит

Unbounded universal compactor, proxy, arbitrary parser/plugin code, large concurrent benchmark, automatic policy learning and cloud source sharing.
