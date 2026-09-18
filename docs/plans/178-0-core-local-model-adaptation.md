# План 178.0 — Core-owned local model adaptation pipeline

Статус: active implementation contract. Исторический источник постановки:
issue #156; функционал этим документом не считается реализованным.

## Цель

Добавить в Core один bounded pipeline `source -> compatible format -> optional
quantized variant -> structural/runtime verification -> calibration/benchmark
-> explicit promotion`, используя существующие local model runtime manager,
performance calibration, hardware-fit evidence, Agent Benchmark Matrix,
resource pressure, durable background execution и approval/promotion owners.

## Текущее основание

В checkout уже есть контракты и storage для
`local_model_runtime_manager.rs`, `local_model_performance_calibration.rs`,
`hardware_fit_evidence.rs`, `agent_benchmark_matrix.rs` и соответствующие
SQLite modules. Они задают fit, artifact, calibration and benchmark evidence,
но не образуют единый adaptation job или adapter trust boundary.

## Граница

```text
source descriptor/hash -> preflight/resources -> frozen adapter
-> staging conversion/quantization -> structural/runtime verification
-> calibration + frozen benchmark -> ReadyForPromotion
-> explicit Core promotion -> existing runtime registry
```

V1 не требует Python/CUDA/toolchain install, cloud training, hidden promotion,
second registry/scheduler или storing weights in SQLite/ArtifactStore. LoRA/
QLoRA/train/merge остаются extension points, не MVP.

## Этапы

- [Этап 1 — contract, adapter identity и storage](./178-1-core-local-model-adaptation.md)
- [Этап 2 — pipeline, resource bounds и recovery](./178-2-core-local-model-adaptation.md)
- [Этап 3 — IPC, promotion и projection](./178-3-core-local-model-adaptation.md)
- [Этап 4 — verification и closure](./178-4-core-local-model-adaptation.md)

## Зависимости

### Блокирующие

- Existing local model runtime, calibration, hardware-fit and model artifact
  owners identified above.
- Existing Agent Benchmark Matrix, resource-pressure and durable background
  execution/recovery owners.
- Existing approval/explicit promotion and authenticated IPC boundaries.

### Опциональные

- A single Windows-capable adapter package is the MVP; additional formats,
  accelerators and training operations are optional adapters.

## Критерии готовности

- [ ] Versioned request/job/adapter/evidence contracts use immutable hashes and
  bounded typed arguments; no arbitrary shell command is accepted.
- [ ] Source, adapter, target and output are hash/format/runtime verified before
  publication; no half-registered model can become active.
- [ ] Disk/RAM/VRAM/concurrency/process cancellation use existing resource and
  background owners; network/toolchain install is denied by default.
- [ ] Calibration and benchmark compare against a frozen compatible baseline;
  `ReadyForPromotion` requires explicit Core action.
- [ ] Restart, staging quarantine, resumable checkpoint and orphan publication
  recovery are deterministic and fail closed.
- [ ] IPC/UI remain projection-only and tests/docs/release evidence are fresh.

## Source issue

- [issue #156](https://github.com/rkfsociety/EvoHime/issues/156) Core-owned local model adaptation pipeline (historical ID).
