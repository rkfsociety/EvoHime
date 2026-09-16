# План 173.2 — Cloud Provider Profiles: runtime, discovery и recovery

## Зависимости

### Блокирующие

- [Core-контракт и storage](./173-1-cloud-provider-profiles.md).
- `ModelGateway`, общий `OpenAICompatibleProvider`, существующие retry,
  health/circuit и #125 resilience владельцы.

### Опциональные

- Provider-native adapter для Gemini/Mistral/Hugging Face только при
  подтверждённом incompatibility fixture.

## Реализация

- Подключить profile registry к `ModelGateway::from_config`, route resolution и
  `fetch_model_catalog`; endpoint и auth scheme разрешаются только из trusted
  profile или явной пользовательской конфигурации.
- Реализовать bounded authenticated discovery там, где provider API это
  поддерживает, normalization capabilities/limits/lifecycle, deterministic
  deduplication и cache states `Fresh`, `Stale`, `Unavailable`,
  `CredentialRejected`, `DiscoveryUnsupported`.
- Перед execution выполнять capability/credential/health preflight; 401, 429,
  model-not-found, protocol mismatch и outage остаются разными typed outcomes
  и поступают в существующий resilience/fallback policy.
- После restart восстанавливать только versioned metadata snapshots; in-flight
  request не возобновлять, stale catalog не разрешает paid или unsupported
  route, late discovery result не перезаписывает новую revision.

## Критерии

- Ни один catalog response не выбирает arbitrary base URL и не меняет policy.
- Generic-compatible providers не получают дублированный client; native
  adapter появляется только при зафиксированном feature gap.
- Catalog failure/restart/cancellation/TTL дают объяснимый bounded state.
- One call использует один immutable provider/model snapshot до конца stream.

## Non-goals

Обязательные live provider checks, автоматическая покупка credits, silent
fallback на другую модель и выполнение внешних activation actions.
