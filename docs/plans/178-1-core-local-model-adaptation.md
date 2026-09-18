# План 178.1 — Contract, adapter identity и storage

## Изменить

1. Define versioned `AdaptationRequest`, job state machine and evidence bundle
   around existing local model/runtime/calibration/benchmark types. Include
   source model revision/hash, target format/quantization, adapter ref/hash/
   version, resource policy, benchmark ref, approval and idempotency.
2. Define validated adapter descriptor with operation/format support, resource
   requirements, resumability, network requirement (false by default) and
   typed bounded arguments. No shell/raw command field.
3. Extend existing local-model storage with bounded job, checkpoint, staging,
   verification and promotion metadata. Model weights stay in managed local
   artifact paths owned by the runtime manager.
4. Add atomic CAS transitions `Created`, `Preflighted`, `WaitingForResources`,
   `Running`, `Verifying`, `Benchmarking`, `ReadyForPromotion`, `Promoted`,
   `Rejected`, `Cancelled`, `Failed`, `Interrupted`.

## Зависимости

### Блокирующие

- `local_model_runtime_manager.rs`, `local_model_performance_calibration.rs`,
  `agent_benchmark_matrix.rs`, `hardware_fit_evidence.rs` and their stores.

### Опциональные

- Existing local artifact format can be the first adapter target; no new
  production dependency is introduced.

## Проверка

Contract tests cover canonical hashes, invalid arguments, state transitions,
adapter identity mismatch, source revision/hash and promotion CAS conflicts.
Storage tests prove bounded metadata, migration backup and no model weights or
secrets in SQLite.

## Rollback и evidence

Keep new states behind a disabled adapter/pipeline feature until runtime gates
are implemented. Preserve rejected/staging metadata for diagnosis without
making it a registry entry; document the contract after verification.
