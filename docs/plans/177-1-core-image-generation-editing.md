# План 177.1 — Contract, capability и storage

## Изменить

1. Добавить Core-owned versioned image capability descriptor с `Supported`,
   `Unsupported`, `Unknown`, `Stale`, операциями generate/edit/mask-edit,
   bounded MIME/size/pixel/count limits, epoch и provenance.
2. Добавить bounded `ImageRequest`, `ImageJob`, `ImageResult` и `GeneratedImage`
   contracts с prompt/input/mask artifact references, privacy/policy hashes,
   deadline и idempotency; raw prompt и bytes не сохранять в durable metadata.
3. Расширить ModelGateway capability projection additive-полями, не создавая
   второй provider registry. Route selection остаётся существующим owner.
4. Расширить local-storage domain facade существующей image-job metadata
   schema. Binary content остаётся только в существующем ArtifactStore policy;
   job table хранит hashes, refs, states, snapshots и reason codes.
5. Зафиксировать atomic state transitions, optimistic revision/CAS и quota
   invariants, включая ownership/hash/task binding для input artifacts.

## Почему именно там

Capability и route metadata принадлежат `model-gateway`; lifecycle и policy
принадлежат Core; durable metadata должна использовать существующие SQLite
facades рядом с ArtifactStore, иначе появится второй storage authority.

## Зависимости

### Блокирующие

- `provider_contract.rs`, `routing_policy.rs`, existing artifact ownership and
  local-storage migration/backup conventions.

### Опциональные

- Provider-specific adapters can initially return `Unsupported` without
  changing contract shape.

## Проверка

- Unit tests for canonical hashing, bounds, operation/mask invariants, MIME and
  state transition matrix.
- Storage tests for migration, CAS conflict, idempotency and no raw prompt/
  binary persistence.
- `git diff --check` and relevant Rust checks after implementation.

## Rollback и evidence

Migration must be additive and backup-protected; disabling the capability must
leave historical metadata readable. Record contract/storage evidence in
`docs/architecture.md`, `docs/current-state.md` and `docs/release-evidence.md`
only when implementation is actually shipped.
