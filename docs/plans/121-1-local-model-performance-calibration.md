# План 121.1 — Local Model Performance Calibration: Core-контракт, schema и storage

Статус: этап 1 для [плана 121.0](./121-0-local-model-performance-calibration.md); issue: [#101](https://github.com/rkfsociety/EvoHime/issues/101). Функционал этим документом не считается реализованным.

## Цель

Зафиксировать exact identity, benchmark config/sample/profile contracts, context/memory/stability aggregation, staleness, confidence, retention и privacy boundary без реального запуска inference.

## Зависимости

### Блокирующие

- План 121.0 — scope, distinction from Agent Benchmark Matrix и acceptance.
- План 116 contract, Model Purpose Routing/ModelProfile, policy/budget/provenance, SQLite backup/migration и event journal.

### Опциональные

- План 36 для quality-run linkage.
- Diagnostics/telemetry and cache integrations.

## Реализация

0. Сверить overview с live Local Model Runtime/ModelProfile/Backend/Benchmark Matrix surfaces, current schema and free IPC tags; не создавать второй inference/quality authority.
1. Ввести bounded types для benchmark suite/config, warmup/measured run kind, prompt/context profiles, sample termination/health/validity, metrics, headroom, stability, confidence/evidence class, calibration session/profile/comparison and typed errors.
2. Зафиксировать exact identity/hash over model artifact, runtime/version, hardware hash, driver, launch config, context semantics and suite revision. Display name and catalog estimate cannot key reuse.
3. Определить metric semantics and unknown rules: TTFT/prefill/decode/latency/load/memory/tokens; no invented value when provider telemetry is unavailable. Define median/p10/p90/variance/failure rate and performance class thresholds as versioned policy.
4. Определить context curve points, headroom confidence, `ComfortableContext`, `InteractiveContext`, `MaximumMeasuredContext`, contamination/`ContendedMeasurement` and insufficient-sample semantics.
5. Добавить additive durable storage for calibration sessions, aggregate profiles, bounded recent samples, config/suite references, comparison metadata and stale history. No raw prompt/output, credentials, executable identity or automatic upload.
6. Define retention: aggregate profiles durable, recent samples bounded/cleanable, referenced historical profile remains addressable; cancellation/failed session cannot become `MeasuredLocal`.
7. Добавить fixtures for hash identity, warmup exclusion, missing telemetry, context OOM, variance, stale inputs, redaction, retention, migration rollback/corruption and no capability expansion.

## Acceptance-to-contract matrix

- `C01` exact profile identity → immutable hashes and stale rules.
- `C02` measurement semantics → sample fields, unknowns, warmup and aggregation.
- `C03` context/memory → curve/headroom/derived signals within #116 ceiling.
- `C04` stability/confidence → variance/failure/contended evidence classes.
- `C05` durable session → lifecycle, retention, cancellation and recovery metadata.
- `C06` privacy/security → local-only metadata and no runtime authority.

## Критерии выхода

- [ ] Contract, bounds, hash, metric units, aggregation and state transitions are tested.
- [ ] Storage is additive/transactional with backup/rollback and retention evidence.
- [ ] Unknown/stale/insufficient/cancelled results cannot become current measured evidence.
- [ ] No secrets, raw prompts/outputs, arbitrary args or external sharing enter records.

## Не входит

Inference runner, telemetry implementation, runtime resource arbitration, routing integration, IPC/UI и actual benchmark execution.
