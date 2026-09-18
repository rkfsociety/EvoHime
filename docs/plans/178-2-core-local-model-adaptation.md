# План 178.2 — Pipeline, resource bounds и recovery

## Изменить

1. Implement preflight for source hash/format, target compatibility, hardware
   fit, free disk/RAM/VRAM, adapter identity, policy and benchmark baseline.
2. Schedule through existing durable background/resource-pressure owners with
   disk reservation, bounded concurrency/GPU serialization, Windows process
   isolation/Job Object cancellation and streaming file hashes.
3. Run adapter only through a typed Core boundary. Write to staging, flush and
   hash output, validate format/runtime probe, then atomically promote to the
   managed local model artifact path.
4. Execute calibration and Agent Benchmark Matrix against immutable baseline/
   policy/model profile; unavailable or incompatible evidence cannot pass.
5. Implement restart recovery: queued jobs requeue/cancel, running jobs require
   exact resumable checkpoint or become interrupted, verifying/benchmarking is
   idempotent, staging is quarantined, and published orphans reconcile via the
   existing journal.

## Зависимости

### Блокирующие

- Plan 178.1 and existing background/resource/runtime publication owners.

### Опциональные

- Adapter checkpoint support can be absent; then interruption is terminal and
  safe restart starts only by explicit new idempotency identity.

## Проверка

Use a fake adapter to prove deterministic conversion/quantization, changed
adapter hash rejection, corrupt output rejection, resource admission, cancel,
restart and no activation before promotion. Test Windows path/process policy at
the boundary without installing external toolchains.

## Rollback и evidence

Never delete a source model or active runtime on failed adaptation. Quarantine
staging and mark job rejected/failed with bounded hashes/reason codes. A failed
benchmark cannot be converted to promotion by editing projection metadata.
