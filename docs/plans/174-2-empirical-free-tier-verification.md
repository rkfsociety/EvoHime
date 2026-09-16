# План 174.2 — Free Access Evidence: probes, observations и recovery

## Зависимости

### Блокирующие

- [Evidence contract и storage](./174-1-empirical-free-tier-verification.md).
- Existing Model Gateway transport, network capability policy, cancellation,
  timeout, retry/circuit and provider-profile discovery from plan 173.

### Опциональные

- Provider usage headers and reset metadata when safely parseable.

## Реализация

- Реализовать bounded verification policy (`Disabled`, `PassiveOnly`,
  `OnFirstUse`, `PeriodicBounded`, `ManualOnly`) с отдельными request/token/
  provider budgets, TTL, backoff и per-cycle cap.
- Выполнять только минимальный synthetic completion через существующий gateway;
  validate transport, schema, identity, semantic non-empty result or valid
  tool/reasoning-only result and usage semantics. HTTP 2xx alone is failure.
- Нормализовать `EmptyCompletion`, `MalformedResponse`, `BillingRequired`,
  `ActivationRequired`, `QuotaRejected`, `AccountRestricted`,
  `ModelUnavailable` и `ProviderProtocolDrift`; 402/403 contradict free,
  429 updates cooldown/observed limit without marking paid.
- После restart не возобновлять in-flight probe; late result rejected by
  evidence revision. Catalog/credential/region/model changes and TTL trigger
  invalidation or recheck, not silent promotion.

## Критерии

- Probe storm impossible under configured budget and cancellation is bounded.
- User conversation, workspace, memory, raw keys and raw provider bodies never
  enter request, evidence, logs, artifacts or renderer projection.
- Account-wide limits are not multiplied by model count; unknown units remain
  unknown.

## Non-goals

Automatic signup/check-in/community/payment, scraping as authority, quality
benchmarking and a second reliability implementation.
