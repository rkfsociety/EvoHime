# EvoHime — доменный контракт: локальные модели, калибровка и планировщик

Канонический контракт домена локального инференса, калибровки, адаптации и совместимости с железом. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).

## Local Model Runtime Manager v1 (план 116)

Core владеет versioned hardware snapshot без tracking identifiers, immutable
model/runtime/artifact/session metadata, conservative fit, activation policy и
resource admission. Durable records используют optimistic revision fence;
authenticated additive IPC command 257/event 102 отдаёт только redacted
projection. Artifact download принимает HTTPS или loopback HTTP, bounded size,
explicit range resume, streaming hash/size verification и atomic promotion в
Core-managed root.

Supervisor запускает allowlisted local adapter в Job Object. `Ready` требует
bounded `/v1/models` loopback probe с точным model id; approved profile
преобразуется в существующие `local-managed` gateway/resilience references,
session capability остаётся ephemeral. Unknown, unverified, staging, traversal,
stale revision и health mismatch fail closed.

## Local Model Performance Calibration v1 (план 121, реализован 2026-09-09)

Calibration владеет только machine-specific bounded metadata и не заменяет
Agent Benchmark Matrix, Local Model Runtime Manager или Model Purpose Routing.
`CalibrationIdentity` связывает model artifact hash, runtime/version, hardware
hash, optional driver, launch config, context profile и suite hash. Samples
разделяют warmup и measured, недоступные TTFT/TPS/RAM/VRAM остаются `Unknown`, а
aggregate использует bounded median/p90/variance/failure rate. Отменённая,
неполная или stale-сессия не становится `MeasuredLocal`.

SQLite schema v96 добавляет durable calibration sessions и immutable performance
profile metadata без raw prompts/outputs, credentials или executable args.
Core admission повторно проверяет verified ready model/runtime/artifact и
hardware-safe identity. При закрытии plan 121 versioned inference-stream
adapter отсутствовал, и calibration корректно возвращал typed
`unavailable_adapter`. Текущий source checkout добавляет adapter только как
часть отдельного Core-owned plan 178; этот owner не меняет
routing/recommendation и принимает только локальный Supervisor runtime с
hash-pinned моделью.

Для evidence freeze highest proto tags были `260/105`; до dedicated `261/106`
используется существующий authenticated Local Model Runtime Manager transport.
Electron panel — metadata-only projection, явно различающая unavailable,
estimated, stale и measured состояния.

## Core-Owned Local Model Adaptation v1 (plan 178)

Adaptation принимает только already-managed GGUF F16/F32 source с проверенными
artifact hash/size, exact model revision, target из allowlist
`Q4_K_M`/`Q5_K_M`/`Q8_0`, frozen benchmark suite/baseline и approval-policy
identity. Поддержанный converter/runtime — llama.cpp `b10981` Windows x64 CPU
asset с фиксированными archive и executable/DLL SHA-256; установка начинается
только по явному действию. Arbitrary URL, command line, executable path,
SafeTensors/Python/CUDA conversion и accelerator-fit inference не поддержаны.

Core хранит bounded request/job/evidence, frozen benchmark inputs, disk
reservations и publication journal в SQLite schema 178; model weights остаются
в managed filesystem. Admission повторно проверяет source record/hash, свежий
host pressure, RAM и свободное место вместе с reservations других
nonterminal jobs. `WaitingForResources` jobs проходят FIFO retries из
существующего durable-background poll task с повторным preflight; output идёт
в staging и проверяется по hash, size и GGUF structure. Supervisor запускает
только hash-verified package с typed arguments в Windows Job Object; v1 держит
не более одного quantizer/inference процесса и cancellation завершает process
tree. Restart прерывает conversion без resumable checkpoint, повторяет
verification, сохраняет benchmark для точного явного retry и reconciles
prepared publication по journal. Активная модель не меняется при подготовке:
promotion требует Core-side approval и exact job/output/benchmark revisions и
hashes.

Calibration использует loopback OpenAI-compatible SSE adapter с bounded
request/response; benchmark вызывает только реальную локальную inference
модель, а deterministic/fixture executors не могут создать promotion
evidence. IPC остаётся authenticated через существующий Local Model Runtime
Manager command; Electron получает bounded job/evidence projection и отправляет
запросы, не выдавая approval и не меняя policy.

## Local Inference Scheduler v1 (план 128, MVP реализован 2026-09-09)

`evohime_core::local_inference_scheduler` хранит bounded typed contract с
schema/revision/status/priority/hash и fail-closed admission. При отсутствии
versioned inference-stream adapter статус всегда `Unavailable`; модуль не
создаёт worker queue, не измеряет inference и не дублирует Model Gateway.

### Hardware Fit Evidence Catalog v1 (план 168)

Core владеет bounded portable hardware-fit observations: нормализованными
hardware/model/runtime/context identity, агрегированными метриками,
confidence и canonical SHA-256. Raw machine identifiers, prompts, credentials
и внешний payload не сохраняются и не проецируются. Exact local calibration
имеет приоритет над portable prior, а текущая conservative memory admission
может заблокировать исторически подходящую конфигурацию.

SQLite schema v168 хранит revisioned metadata с optimistic fencing и лимитом
2048 observations. Authenticated additive desktop IPC использует command 276 и
event 121; Electron получает только redacted metadata через main/preload API.
Каталог не исполняет код, не загружает данные автоматически и не создаёт
второй hardware profiler, model catalog или benchmark runner.
