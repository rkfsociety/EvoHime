# План 181.0 — Core-owned PII-aware sensitive-data egress guardrails

Статус: active implementation contract. Исторический источник постановки:
issue #159; функционал этим документом не считается реализованным.

## Цель

Расширить существующий `Sensitive Data Guardrails v1` до двухфазной
PII-aware egress boundary: локальная bounded классификация поднимает
`required_privacy` до route selection, затем exact final payload проходит
destination-aware allow/transform/approval/block gate прямо перед external
effect. Durable state содержит только redacted metadata/provenance, никогда
raw PII.

## Текущее основание

`crates/evohime-core/src/sensitive_data_guardrails.rs` уже реализует bounded
deterministic text/JSON/stream redaction для email, secret/bearer tokens и
private keys. `crates/model-gateway/src/routing_policy.rs` и
`provider_contract.rs` владеют `PrivacyClass`/route filtering. Existing
approval, context/loadout, provenance, external-agent and IPC owners должны
расширяться, а не дублироваться.

## Граница

```text
context composition -> local classification -> derived required_privacy
-> existing route resolver -> frozen route -> exact final scan
-> transform/approval/block -> existing provider/tool/agent effect
```

V1 не является DLP daemon/proxy, cloud sanitizer, compliance certification,
OCR/image/audio detector, arbitrary renderer regex engine или storage для
reversible raw PII.

## Этапы

- [Этап 1 — detector/classification и policy contract](./181-1-core-sensitive-egress-guardrails.md)
- [Этап 2 — routing и final dispatch gate](./181-2-core-sensitive-egress-guardrails.md)
- [Этап 3 — approvals, recovery и projections](./181-3-core-sensitive-egress-guardrails.md)
- [Этап 4 — verification и closure](./181-4-core-sensitive-egress-guardrails.md)

## Зависимости

### Блокирующие

- Existing Sensitive Data Guardrails v1, ModelGateway `required_privacy` and
  provider/profile privacy metadata (plan 173 where applicable).
- Existing context namespace/loadouts, approvals, execution/provenance ledger,
  tool and external-agent boundaries and authenticated IPC.

### Опциональные

- Plan 180 A2A can consume the common contract when available; this plan must
  still keep current external-agent egress safe without requiring new A2A.
- Future OCR/image/audio classification is explicitly outside V1.

## Критерии готовности

- [ ] Existing v1 is extended, not replaced by a second policy/runtime/storage
  engine; detector pack and policy are versioned/hashed.
- [ ] Local phone/card/IBAN and bounded contextual national/tax detectors use
  structural/checksum/context validation with false-positive fixtures.
- [ ] Content-derived privacy participates before route selection and exact
  final payload is rescanned after route freeze before cloud/external effect.
- [ ] Decision binds payload, route, policy and detector hashes; unknown/failure/
  overflow fail closed externally and no weaker fallback is allowed.
- [ ] Existing approval owner receives metadata-only request bound to exact
  hashes; no raw finding reaches storage, logs, errors, IPC or renderer.
- [ ] Recovery/replay, provider ambiguous outcome and concurrency are safe;
  focused security/routing/recovery tests and canonical docs are updated.

## Source issue

- [issue #159](https://github.com/rkfsociety/EvoHime/issues/159) Core-owned PII-aware sensitive-data egress guardrails.
