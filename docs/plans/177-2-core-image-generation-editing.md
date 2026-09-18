# План 177.2 — Dispatch, artifacts и recovery

## Изменить

1. Встроить image preflight в existing Core execution path: operation,
   privacy/network policy, prompt/count/dimension/pixel/input/output limits,
   artifact ownership and fresh capability epoch.
2. Freeze route, capability and policy snapshots before provider effect; do not
   accept provider URLs as an implicit download instruction.
3. Validate streaming/buffered provider output at a bounded decode boundary:
   declared and actual MIME, magic bytes, dimensions, total pixels/bytes,
   output count and SHA-256 before atomic ArtifactStore publication.
4. Reuse resource-pressure, background execution, cancellation and deadlines.
   Pre-dispatch cancellation makes no provider call; post-dispatch cancellation
   never reports remote cancellation unless confirmed.
5. Implement crash recovery: pre-dispatch work is interrupted/requeued by the
   existing policy, dispatched work becomes `UnknownOutcome` unless the same
   provider job can reconcile, and published artifact hashes are rechecked.

## Почему именно там

The gateway already owns route dispatch and the Core execution/background
owners already provide bounded scheduling and recovery. Artifact validation must
finish before publication so no unverified `ArtifactRef` enters provenance or
workflow continuation.

## Зависимости

### Блокирующие

- Plan 177.1 contracts/storage.
- Existing ModelGateway dispatch, artifact publication and durable background
  execution/recovery owners.

### Опциональные

- A provider reconciliation API may upgrade `UnknownOutcome` to a terminal
  result; without it no blind retry is allowed.

## Проверка

- Fake provider tests for valid output, malformed MIME/magic, oversized/deep or
  dimension-mismatched output, quota exhaustion and URL non-fetch behavior.
- Recovery tests at every lifecycle boundary, including crash after dispatch,
  duplicate idempotency and missing/corrupt artifact.
- Concurrency/cancellation tests prove bounded jobs and zero provider calls
  before dispatch cancellation.

## Rollback и evidence

Gate activation by capability/configuration, not by deleting stored jobs.
Quarantine invalid staging artifacts and retain only bounded reason metadata.
Capture runtime/recovery/security evidence before moving to the IPC stage.
