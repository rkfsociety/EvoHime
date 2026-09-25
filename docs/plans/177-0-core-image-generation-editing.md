# План 177.0 — Core-owned image generation and editing

Статус: active implementation contract. Исторический источник постановки:
issue #155; функционал этим документом не считается реализованным.

## Цель

Добавить в Rust Core типизированную capability и lifecycle для генерации и
редактирования изображений через уже существующий ModelGateway, policy,
durable execution, ArtifactStore и authenticated IPC. Результатом являются
проверенные локальные artifacts и bounded metadata; renderer не получает
полномочия на provider calls, workspace writes или raw binary IPC.

## Текущее основание

Точки расширения в checkout: `crates/model-gateway/src/provider_contract.rs` и
`routing_policy.rs` для route/capability/privacy, `crates/evohime-core` для
agent execution/background/recovery, `crates/evohime-local-storage/src/artifact_store.rs`
для artifact ownership и `crates/desktop-ipc/proto/evohime.desktop.proto` для
additive authenticated projections. Existing `vision` означает image input и
не является output capability.

## Граница

```text
image request -> Core preflight -> frozen route/capability snapshot
-> existing ModelGateway -> bounded output validation
-> ArtifactStore atomic publication -> durable result/provenance -> IPC metadata
```

Новый image server, отдельный provider gateway/store, обязательный cloud/GPU,
arbitrary URL fetch, silent workspace write и OCR/image-PII classification не
входят в V1. Отсутствие compatible backend даёт typed `unavailable`.

## Этапы

- [Этап 1 — contract, capability и storage](./177-1-core-image-generation-editing.md)
- [Этап 2 — dispatch, artifacts и recovery](./177-2-core-image-generation-editing.md)
- [Этап 3 — IPC и projection](./177-3-core-image-generation-editing.md)
- [Этап 4 — verification и closure](./177-4-core-image-generation-editing.md)

## Зависимости

### Блокирующие

- Existing ModelGateway route/capability/privacy and provider profile owners;
  plan 173 Cloud Provider Profiles must remain the source of provider metadata.
- Existing ArtifactStore, execution ledger, cancellation/deadline and durable
  background/recovery owners.
- Existing authenticated desktop IPC and workspace effect gate.

### Опциональные

- Core `FreeAccessEvidence` in [`../architecture.md`](../architecture.md) can
  supply fresh provider evidence, but its absence must result in
  `Unknown/Stale`, not an automatic remote fallback.
- Local model backends may implement the capability later; they are not a
  blocking dependency for the typed Core contract.

## Критерии готовности

- [ ] Generate/edit request, capability, job state and result contracts are
  versioned, bounded, hashed and persisted through existing owners.
- [ ] Route/capability/privacy snapshots are frozen before dispatch; stale or
  unknown output capability fails closed.
- [ ] Provider output is validated by bytes, MIME/magic, dimensions, pixels,
  count and hash before ArtifactStore publication.
- [ ] Pre-dispatch cancellation is terminal; ambiguous post-dispatch outcomes
  are not blindly retried and recover through existing semantics.
- [ ] IPC exposes metadata and existing artifact references only; renderer is
  projection-only and workspace export remains explicit.
- [ ] Focused contract, security, artifact, recovery, IPC and documentation/
  release-evidence checks pass.

## Source issue

- [issue #155](https://github.com/rkfsociety/EvoHime/issues/155) Core-owned image generation and editing capability (historical ID).
