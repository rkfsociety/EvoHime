# План 180.0 — Core-owned A2A remote-agent bridge

Статус: active implementation contract. Исторический источник постановки:
issue #158; функционал этим документом не считается реализованным.

## Цель

Добавить outbound A2A client как protocol/backend extension существующего
External Coding Agent Adapter owner. Core регистрирует explicit remote-agent
profile, получает bounded untrusted card только из profile-owned origin,
строит policy-filtered delegation capsule, выполняет remote task, normalizes
states/artifacts и продолжает existing workflow/team lifecycle.

## Текущее основание

`crates/evohime-core/src/external_coding_agent_adapter.rs` и его storage уже
владеют ACP/external-agent manifest, capability intersection, snapshots,
cancel/timeout и bounded frames. Также есть context namespace/loadouts,
ArtifactStore, research SSRF protections, durable background, provenance и
authenticated IPC. A2A не должен копировать эти authorities.

## Граница

```text
explicit profile -> bounded card snapshot -> capability/privacy preflight
-> immutable context capsule -> outbound submit/status -> validated artifacts
-> existing provenance/workflow continuation
```

V1 не открывает inbound HTTP/webhook listener, не ищет агентов в Интернете,
не создаёт public server/cloud relay, не пересылает credentials и не повторяет
ambiguous submit.

## Этапы

- [Этап 1 — profile/card contract и storage](./180-1-core-a2a-bridge.md)
- [Этап 2 — network lifecycle, capsule и recovery](./180-2-core-a2a-bridge.md)
- [Этап 3 — workflow/IPC integration](./180-3-core-a2a-bridge.md)
- [Этап 4 — verification и closure](./180-4-core-a2a-bridge.md)

## Зависимости

### Блокирующие

- Existing external-agent adapter/ACP owner and its capability/auth/snapshot
  contracts.
- Existing SSRF/network policy, context namespace/loadouts, ArtifactStore,
  approval/privacy, durable background/recovery and workflow/team owners.
- Existing authenticated desktop IPC.

### Опциональные

- Streaming/SSE is optional; bounded polling is the baseline. Push/webhook
  capability is metadata-only and returns typed unavailable.
- Plan 181 may later add stronger sensitive egress scanning; A2A must already
  use existing privacy/policy boundary and cannot bypass it.

## Критерии готовности

- [ ] A2A is a backend kind of the existing external-agent owner; no second
  runtime, scheduler, storage or team roster authority exists.
- [ ] Profiles, card snapshots, protocol/capability intersection and expiry are
  bounded, hashed, profile-owned and fail closed.
- [ ] Public HTTPS SSRF/DNS/redirect/TLS/userinfo protections and explicit
  loopback/private profile boundaries are enforced.
- [ ] Capsule excludes secrets/full workspace and binds parent grant/privacy/
  provenance; remote artifacts validate before ArtifactStore publication.
- [ ] Submit ambiguity becomes `UnknownOutcome`; known IDs reconcile only the
  same task; no blind resubmit or auth downgrade occurs.
- [ ] Workflow, recovery and IPC tests/docs/release evidence are complete; no
  inbound public listener is introduced.

## Source issue

- [issue #158](https://github.com/rkfsociety/EvoHime/issues/158) Core-owned A2A remote agent interoperability bridge.
