# План 123.3 — Content-Aware Context Compression: IPC, client projection и diagnostics UI

Статус: этап 3 для [плана 123.0](./123-0-content-aware-context-compression.md); после [плана 123.2](./123-2-content-aware-context-compression.md).

## Цель

Показать Core-provided compression diagnostics, omitted/recovery state, savings provenance and token-sink summaries, сохраняя renderer projection-only и не раскрывая raw source/prompt/output.

## Зависимости

### Блокирующие

- План 123.2 — classifiers/compactors/recovery/runtime events and stable metadata.
- Authenticated IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge, Context/Operations panels and Artifact/evidence open route.

### Опциональные

- Prompt Cache, Agent Benchmark, Diagnostics Bundle and ContextRef UI integration.

## Реализация

1. После проверки highest tag зарезервировать additive commands/events/results для compression status/policy summary, compact block metadata, token-savings report, recovery request/status, source open and benchmark/diagnostic summary. Preserve major, bounds, correlation, idempotency and replay.
2. Core validates block/source/revision/region/actor/max tokens/current run authority/sensitivity; renderer cannot submit fake hash, savings, compactor validation or recovery availability.
3. Передавать metadata-only projection: source kind/short hash, compactor/version/loss class, original/compact estimated tokens, provider-measured counters when available, omitted region summaries, recovery count/tokens, fallback/no-benefit/failure reason and retention state.
4. Связать `ipc_bridge.rs`, shared API, preload/main adapters, reconnect/replay and bounded Context/Operations projection. Unknown/estimated/measured/RecoveryUnavailable remain distinct.
5. Add per-call diagnostics: compression active, source kind, size/token before-after, compactor, loss class, omitted groups, recovery usage and estimated-vs-provider-measured distinction. Do not show giant removed line lists by default.
6. Add bounded Token Sink report: period/run scope, largest context flows, repeated payloads, candidates and negative-savings cases; report is read-only and cannot mutate policy.
7. Add explicit user action to request an omitted region only through Core; show authorized exact evidence/context result or typed denial/stale/recovery-unavailable outcome.
8. Ensure accessible states for original/compact/fallback/no-benefit/partial/recovery-limited/security-denied and safe display of untrusted source text.

## Acceptance-to-projection matrix

- `C01` compression → type/compactor/loss/source metadata.
- `C02` savings → estimate/provider counters/overhead/fallback distinction.
- `C03` omission/recovery → bounded regions, count/tokens, status and typed outcome.
- `C04` lineage → ContextItem/Ledger/Prompt Cache refs and hashes.
- `C05` token sinks → read-only negative/no-op report.
- `C06` security → no raw payload/secret or forged state.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded and replay-safe.
- [ ] Renderer never computes/accepts compression, savings or recovery authority.
- [ ] UI distinguishes measured/estimated, loss class, fallback and unavailable source.
- [ ] Raw source, prompts, outputs, secrets and arbitrary handles stay out of projection.

## Не входит

Client-side compaction/retrieval, full log viewer, arbitrary recovery path, policy editing from reports и automatic compression approval by model.
