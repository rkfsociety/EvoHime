# План 121.0 — Local Model Performance Calibration: measured inference и benchmark-aware routing

Статус: предложено по [issue #101](https://github.com/rkfsociety/EvoHime/issues/101). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить Core-owned контур коротких воспроизводимых inference benchmarks для уже доступных локальных моделей/runtime. Результатом является versioned `LocalModelPerformanceProfile`, привязанный к exact model artifact, runtime, hardware и launch/context configuration, с measured throughput/latency/memory/stability.

Это дополнение к Local Model Runtime Manager (#96), а не его замена, и не Agent Benchmark Matrix (#16):

```text
Agent Benchmark Matrix = качество agent/model/strategy на задачах
Performance Calibration = скорость/стабильность inference на конкретной машине
```

## Текущее основание и граница

В checkout уже реализован Agent Benchmark Matrix с deterministic/unavailable executor, benchmark store и metadata-only UI. Есть Model Resilience/Model Purpose Routing, provider/backend registry, execution ledger, policy/approval/budget и local inference foundations; план #96 задаёт hardware/model/runtime fit и supervised lifecycle. Новый слой хранит только machine-specific measurements и derived routing signals, не переписывает catalog/model descriptor и не создаёт второй benchmark runner для качества.

Кандидатные поверхности: `crates/evohime-core/src/local_model_performance_calibration.rs`, calibration store, Local Model Runtime Manager integration, model routing/recommendation/provenance, additive desktop IPC, Electron Models/Performance UI и optional `cargo eval` adapter. Имена, schema revision и tags подтверждаются на evidence freeze; текущие `agent_benchmark_matrix` surfaces не расширяются без доказанной необходимости.

## Архитектурная граница

```text
LocalHardwareProfile + LocalModelDescriptor + installed artifact
 + LocalInferenceRuntime + safe launch config + benchmark suite
        -> Calibration Runner
        -> warmup / bounded measured samples / telemetry
        -> validation / aggregation / stability
        -> LocalModelPerformanceProfile
        -> #96 recommendation + #95 purpose routing signal
```

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./121-1-local-model-performance-calibration.md)
- [Этап 2 — runner, runtime integration и recovery](./121-2-local-model-performance-calibration.md)
- [Этап 3 — IPC, client projection и UI](./121-3-local-model-performance-calibration.md)
- [Этап 4 — verification, release-evidence и закрытие](./121-4-local-model-performance-calibration.md)

## Зависимости

### Блокирующие

- План 116 / Local Model Runtime Manager: exact model/runtime/hardware identity, safe context, health gate, resource ownership and supervised runtime.
- План 115 / Model Purpose Routing: performance requirements as bounded ranking/eligibility signal.
- Existing Model Gateway/Resilience, Execution Backend Registry, policy/approval/budget/cancellation, provenance/usage, SQLite migration and authenticated IPC.

### Опциональные

- План 36 / Agent Benchmark Matrix: attaches performance refs to quality runs, but does not own inference measurements.
- Plan 53 diagnostics, plan 105 cache hints, and optional local telemetry adapters.

## Основной контракт направления

Core вводит `LocalInferenceBenchmarkConfig`, `LocalInferenceBenchmarkSample`, `ContextPerformancePoint`, `MemoryHeadroom`, `PerformanceStability`, `LocalModelCalibrationSession`, `LocalModelPerformanceProfile` и `RuntimePerformanceComparison`.

Identity включает model descriptor/artifact hash, runtime id/version, hardware profile hash, driver fingerprint when available, launch config hash, benchmark suite/config hash and context profile. Display name alone никогда не применяет measurement к другой quantization/runtime revision.

Session lifecycle: `Queued → WarmingUp → Running → Aggregating → Completed|Cancelled|Failed`. Warmup не входит в measured aggregate. MVP concurrency = 1; prompt profiles bounded (`ShortInteractive`, `MediumGeneration`, `LongContextPrefill`, `StructuredSmallOutput`), context curve тестируется только до #116 safe ceiling и configured budget.

Measured fields: load time, TTFT, prefill/decode TPS, end-to-end latency, input/output/context tokens, peak VRAM/RAM when telemetry exists, runtime health/failure and validity. Missing telemetry remains `Unknown/Unavailable`. Aggregation uses median/percentiles, variance and failure rate; one record-breaking run не становится authority.

Evidence classes: `MeasuredLocal`, `MeasuredLocalStale`, `EstimatedFromHardware`, `CatalogEstimate`, `Unknown`. Stale input (artifact/runtime/hardware/driver/launch/context/suite major) invalidates applicability without deleting history. Performance classes are derived policy presentation, not hard capability.

Measured profile may influence #96 recommendation and #95 purpose routing only inside already allowed capability/security/quality candidate set. It never silently changes explicit user selection. `MeasurementReady`, `RecommendationChanged` and `ActiveSelectionChanged` remain separate outcomes.

## Безопасность, privacy и non-goals

Calibration запускается только через verified/supervised runtime #116, without arbitrary executable/CLI args, credentials, workspace grants or model-generated config. Runtime crash/OOM is controlled recovery and does not automatically corrupt installed artifact health. Measurements remain local; hardware identifiers, fingerprints, model list and benchmark data are not uploaded automatically. Raw prompts/outputs are not persisted unless an explicit bounded validation contract requires fixture hashes.

Не входят quality/intelligence score, public leaderboard/community sharing, GPU overclock/driver changes, exhaustive all-model stress tests, multi-hour thermal lab, concurrent benchmark MVP, automatic model replacement, exact power/thermal accuracy guarantee и network telemetry.

## Критерии готовности направления

- [ ] Есть durable bounded calibration session и exact-identity performance profile.
- [ ] Warmup/measured samples, median/variance/failure-rate и available TTFT/memory metrics разделены явно.
- [ ] Context performance curve и conservative headroom/comfortable/interactive signals вычисляются Core-side.
- [ ] Artifact/runtime/hardware/config/suite drift даёт stale, а не silent reuse.
- [ ] #96 recommendation и #95 purpose routing используют measurement только как bounded signal.
- [ ] Cancellation, runtime crash/OOM, resource ownership и restart recovery безопасны.
- [ ] UI показывает measured/estimated/stale/not measured и не подделывает results.
- [ ] Measurements локальны, redacted и не расширяют capabilities или security boundary.

## Связанный issue

- [#101 Local Model Performance Calibration](https://github.com/rkfsociety/EvoHime/issues/101)
