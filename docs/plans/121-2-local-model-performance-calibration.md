# План 121.2 — Local Model Performance Calibration: runner, runtime integration и recovery

Статус: этап 2 для [плана 121.0](./121-0-local-model-performance-calibration.md); после [плана 121.1](./121-1-local-model-performance-calibration.md).

## Цель

Запускать короткие bounded benchmarks через verified local runtime, собирать доступные telemetry, валидировать samples, агрегировать profiles и безопасно взаимодействовать с #116 resource/health lifecycle.

## Зависимости

### Блокирующие

- План 121.1 — contract, storage, identity and aggregation.
- План 116 runtime manager, supervisor Job Object, model gateway/provider, context/budget/cancellation/resource policies and provenance.

### Опциональные

- Agent Benchmark Matrix linkage, optional GPU/CPU telemetry adapters and local embeddings (не нужны для TPS runner).

## Реализация

1. Реализовать calibration admission: only installed/verified/healthy exact model+runtime, allowed hardware-safe context, bounded config and explicit Manual/AfterInstallPrompt request. Heavy automatic benchmark disabled by default.
2. Реализовать runner with concurrency 1: cold-load measurement, warmup samples, deterministic/versioned prompt fixtures, measured samples for context points and output targets, timeout/cancellation and per-sample health/termination metadata.
3. Integrate runtime adapter metrics without arbitrary command strings. Capture token timestamps for TTFT/prefill/decode where supported, load/latency/tokens and bounded VRAM/RAM snapshots; unavailable telemetry stays unknown.
4. Add baseline contention markers (RAM/GPU/CPU when available), mark `ContendedMeasurement`, conservative confidence and no attempt to control thermal/OS scheduler.
5. Implement aggregation/profile publication: discard invalid/warmup samples, calculate median/percentiles/variance/failure rate, context curve/headroom and derived classes; OOM at larger context does not erase smaller successful point.
6. Coordinate #116 leases/resource ownership: no eviction of in-flight normal call, no OOM-producing parallel calibration, explicit busy/denied outcome, runtime crash cleanup and health reconciliation.
7. Feed exact measured profile into #96 recommendation view and #95 purpose router as ranking/eligibility signal. Keep capability/security/quality/budget gates dominant and explicit user selection unchanged.
8. Implement comparison for same model/config across registered runtimes, exposing per-purpose tradeoffs rather than one universal winner. Add optional Agent Benchmark provenance ref without duplicating low-level samples.
9. Implement recovery: renderer restart does not lose session; Core restart reconciles Queued/Warming/Running as pending/interrupted, never claims complete; installed artifact/runtime health remains correct after cancellation/failure.

## Fault/recovery matrix

- model load/runtime crash → typed failure, supervised cleanup, artifact health not corrupted;
- timeout/cancel → partial session, no MeasuredLocal aggregate, runtime remains usable if health passes;
- OOM at context N → failed point and conservative headroom, prior points retained;
- missing telemetry/stream timestamps → Unknown metric, profile remains honest;
- system contention → ContendedMeasurement/low confidence, no false precision;
- normal call active → calibration waits/denies by resource policy, never evicts in-flight call;
- Core restart → session reconciled/retry offered, no blind duplicate GPU run;
- stale input during run → result marked stale/foreign and not applied to current routing.

## Критерии выхода

- [ ] Cold/warmup/measured lifecycle and context curve run through verified runtime.
- [ ] Cancellation, timeout, crash, OOM and contention are controlled and typed.
- [ ] Aggregates are deterministic for identical valid samples and preserve unknowns.
- [ ] #96/#95 integration never bypasses capability, security, quality or explicit selection.
- [ ] Restart/recovery and resource ownership preserve normal runtime state.

## Не входит

Concurrent throughput MVP, stress/thermal lab, arbitrary launch tuning, driver changes, cloud sharing и model quality evaluation.
