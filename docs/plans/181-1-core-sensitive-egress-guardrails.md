# План 181.1 — Detector/classification и policy contract

## Изменить

1. Extend `sensitive_data_guardrails.rs` with versioned detector pack,
   `SensitiveEntityKind`, confidence and bounded finding/classification summary
   keyed by exact payload hash. Preserve v1 email/secret/bearer/private-key
   behavior.
2. Add local deterministic E.164/labelled phone, payment-card+Luhn,
   IBAN+country/mod-97 and narrowly allowlisted national/tax schemes. Contextual
   numeric findings require field/lexical evidence; regex alone never authorizes
   destructive transform.
3. Add safe normalized view for Unicode/separators/JSON field context/stream
   chunk boundaries while retaining mapping only in-memory; payload changes only
   after explicit action.
4. Extend existing policy snapshot with typed destination classes and
   deterministic precedence for allow/redact/mask/hash/approval/block. Derived
   privacy can only raise upstream privacy.
5. Reuse current input/depth/node/stream budgets and linear bounded detectors;
   no network call or second policy database.

## Зависимости

### Блокирующие

- Existing `sensitive_data_guardrails.rs` v1, policy snapshot and local storage/
  provenance metadata conventions.

### Опциональные

- Additional national schemes may be added only with fixtures and explicit
  detector version; they are not required for initial contract.

## Проверка

Unit/negative fixtures cover valid/invalid checksums, separators, Unicode,
stream boundaries, JSON labels, UUID/hash/version/IP/code false positives,
  bounds, deterministic pack hash and preservation of existing v1 semantics.

## Rollback и evidence

Unknown detector/policy remains typed unknown and cannot silently allow external
egress. Detector-pack activation is versioned; historical classifications stay
readable without raw match text.
