# План 186.0 — Core-owned local typed-decision inference fast path

Статус: active implementation contract. Исходный issue #162 удаляется после сохранения плана; план не означает, что функциональность уже реализована.

## Цель

Добавить side-effect-free local inference path для bounded choice/score/probability через существующие Local Inference Scheduler и Local Model Runtime Manager; calibrated confidence может только сузить или ужесточить уже разрешённое Core decision.

## Основание checkout

local_inference_scheduler.rs прямо фиксирует metadata-only admission и отсутствие versioned adapter. Local Model Runtime Manager владеет verified artifacts/runtime/session/residency; resource pressure, Model Purpose Routing, Benchmark Matrix и performance calibration уже существуют.

## Граница

Core consumer → typed request/policy/privacy preflight → scheduler → model manager → verified local adapter → schema/calibration gate → typed result/disposition → existing consumer.

## Изменяемые контракты

- Typed decision request/result, adapter descriptor, manager-owned capability, quality calibration profile and policy disposition.
- Bounded consumer integration and IPC status projection; no second gateway, registry, scheduler or cache.

## Recovery и rollback

Inference is side-effect-free: interrupted work is retried only with identical frozen inputs/model/adapter/policy hashes and a new attempt identity. Calibration mismatch is unavailable/needs_review. Adapter admission can be disabled without deleting model artifacts or historical evidence.

## Этапы

- [Этап 1 — request/result, adapter, capability и calibration contracts](./186-1-core-local-typed-decision-fast-path.md)
- [Этап 2 — verified local inference execution](./186-2-core-local-typed-decision-fast-path.md)
- [Этап 3 — safe consumer integration и bounded projections](./186-3-core-local-typed-decision-fast-path.md)
- [Этап 4 — quality/security/resource evidence и closure](./186-4-core-local-typed-decision-fast-path.md)

## Зависимости

### Блокирующие

- Local Model Runtime Manager и verified artifact/session contracts.
- Local Inference Scheduler и host resource/hardware pressure owners.
- Model Purpose Routing, PolicyGate, Agent Benchmark Matrix и Local Model Performance Calibration.

### Опциональные

- План 178 adaptation/training pipeline не нужен для MVP.
- Планы 179/184 могут стать downstream consumers/evidence, но не меняют ownership fast path.

## Критерии готовности

- [ ] Versioned bounded request/result поддерживает choice, score, probability и batched questions.
- [ ] Хотя бы один verified side-effect-free adapter исполняется через existing supervisor/runtime boundary.
- [ ] Autonomous confidence возможен только с compatible fresh calibration (ECE + proper probabilistic metric); quality calibration отделена от hardware calibration.
- [ ] Model registry/gateway/scheduler/cache/resource manager не дублируются; отказ даёт typed unavailable/needs_review без hidden fallback.
- [ ] Consumer получает только constrained hint из разрешённого множества; ML не добавляет route/tool/grant и не отменяет deterministic guardrails.
- [ ] Benchmark включает quality, calibration, privacy/security false negatives, latency/batch/memory; docs/evidence обновлены.

## Non-goals

- Замена generative agent/Gateway; fine-tuning pipeline; модель как effect authority.
- Обязательные Python/CUDA/cloud install, arbitrary model plugin или silent slow-path fallback.

## Verification

Плановые gates: contract/calibration validation → verified adapter tests → consumer authority-isolation checks → frozen quality/security/resource benchmarks and Windows acceptance.

## Release evidence

Сохранять adapter/model/calibration hashes, compatible benchmark suite, quality and performance metrics, bounded failure reason codes and CI results; never persist raw input state or hidden activations.

## Критерии выхода overview

- [ ] Existing scheduler/manager/gateway ownership и consumer fail behavior подтверждены до этапа 1.
- [ ] Версия первой adapter protocol и правило новой production dependency определены до dispatch implementation.

## Исходная постановка

- [Issue #162 — Core-owned local typed-decision inference fast path](https://github.com/rkfsociety/EvoHime/issues/162); сохранённый номер исходной постановки, issue удаляется после публикации плана.
