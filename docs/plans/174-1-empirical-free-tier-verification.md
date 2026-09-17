# План 174.1 — Free Access Evidence: Core, schema и storage

## Зависимости

### Блокирующие

- [Обзор плана 174](./174-0-empirical-free-tier-verification.md).
- Provider/model descriptors и credential references из плана 173; #125
  reliability metadata остаётся отдельным владельцем.

### Опциональные

- Existing provenance ledger and diagnostics helpers.

## Реализация

- Расширить существующий `FreeAccessState` из плана 125 через единый Core
  contract `FreeAccessEvidence` с provider/model/account scope,
  advertised/observed/activation state, allowance kind, observed limits,
  successful sample count, confidence, TTL/expiry, failure reason и hash.
- Ввести typed activation requirements, allowance kinds и `CreditUnit`; не
  конвертировать provider credits в tokens/currency без authoritative contract.
- Ввести freshness/invalidation state и deterministic precedence: billing or
  account restriction contradicts prior free evidence, а stale/unknown не
  разрешает strict route.
- Добавить bounded transactional metadata storage с revision fence,
  idempotency, anonymized generic observation без account authority после
  удаления credential binding и без raw response/prompt.

## Критерии

- Credential A evidence не читается для B; region/model scope участвуют в key.
- `TrialOnly`, `CreditOnly`, `ActivationRequired`, `PaidOnly`, `Unknown` и
  `VerifiedFreeLimited` не схлопываются в bool.
- Invalid/oversized/conflicting evidence отклоняется или получает typed
  invalidation без частичной записи.
- `FreeAccessEvidence` является evidence authority для существующего free
  state; отдельный boolean free registry не создаётся.

## Non-goals

Сетевой probe, route selection и UI относятся к следующим этапам.
