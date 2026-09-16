# План 174.3 — Free Access Evidence: routing, IPC и UI

## Зависимости

### Блокирующие

- [Probes and recovery](./174-2-empirical-free-tier-verification.md).
- #125 free-aware routing/resilience, plan 173 catalog/profiles and existing
  `model.catalog`/provider settings IPC.

### Опциональные

- Existing routing explanation and diagnostics panels.

## Реализация

- Добавить Core eligibility intersection:
  capabilities + advertised compatibility + fresh non-contradicted empirical
  evidence + activation complete + quota/circuit/reliability eligibility.
- `FreeOnly` fail-closed при `PaidOnly`, `TrialOnly`, `ActivationRequired`,
  `QuotaExhausted`, stale или unknown; `PreferFree` использует отдельную явную
  fallback policy и никогда не маскирует paid request как free.
- Добавить bounded reason codes/explanation (`activation incomplete`,
  `signup credit exhausted`, account quota, billing contradiction, stale) в
  existing routing trace/IPC projection без secrets/raw payload.
- Обновить provider/model UI: advertised, observed, allowance, activation,
  limit scope, confidence и last verified отображаются раздельно; unknown/stale
  показываются как непроверенные.

## Критерии

- Eligibility не меняет reliability class и quality score.
- Один stream сохраняет immutable resolved snapshot; новый probe влияет только
  на следующий call после revision-safe publication.
- UI не может сам включить paid fallback или подтвердить free state.

## Non-goals

Automatic billing, hidden paid fallback, account identity disclosure и raw
provider diagnostics in renderer.
