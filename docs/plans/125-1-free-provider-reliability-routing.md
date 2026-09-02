# План 125.1 — Provider contract, catalog и storage

Статус: этап 1 для [плана 125.0](./125-0-free-provider-reliability-routing.md); issue: [#106](https://github.com/rkfsociety/EvoHime/issues/106). Реализация начинается после evidence review.

## Зависимости

### Блокирующие

- План 125.0 и существующие provider contract/catalog, credential binding, SQLite migration/backup и redaction primitives.
- Canonical Model Gateway identity, provenance и resilience policy; второй transport/secret store запрещён.

### Опциональные

- Agent Benchmark Matrix, Diagnostics Bundle и будущие provider-specific UI screens; без них сохраняется metadata-only degraded projection.

## Реализация

1. Определить bounded `ProviderProfile`/`ProviderModelSnapshot` с provider/upstream identity, transport, endpoint/region, credential/access mode, capabilities, privacy, lifecycle и profile hash. `Local Ollama` и `Ollama Cloud` не могут совпасть по identity.
2. Ввести `FreeAccessState`: `Free`, `FreeTierLimited`, `TrialCredits`, `Paid`, `Unknown`, `Experimental`, `UnknownNeedsRefresh`; хранить source, observed/expiry/TTL, account/region scope, confidence и zero-price proof отдельно от capability.
3. Ввести catalog refresh contract с immutable revision/hash, bounded model count/field sizes, conservative invalidation и no-cache-on-secret/error semantics. Dynamic discovery не превращается в hardcoded model list.
4. Зарегистрировать Wave A/B profiles через общий OpenAI-compatible adapter там, где подтверждена совместимость; native adapter разрешён только при material gap. У каждого профиля отдельные parsers для rate limit/usage и lifecycle/deprecation.
5. Ввести `QuotaSnapshot`, `ProviderLimitSnapshot` и `ProbePolicy` с request/token ceilings, quota floor, Retry-After, reset time, scope (model/account/endpoint), anonymous/authenticated mode и bounded backoff.
6. Добавить transactional metadata-only storage/migration для profiles, catalog revisions, access state, quota/limit snapshots и probe policy. Не хранить raw provider payloads, prompts, credentials или бесконечную telemetry.
7. Зафиксировать optimistic/idempotent writes, corruption/rollback, expiry/size/count caps и recovery к последней валидной profile/catalog revision.

## Критерии выхода

- [ ] Invalid, stale, undocumented or experimental profile cannot become automatic `FreeOnly`.
- [ ] Provider/model/upstream identities and credential bindings cannot cross-contaminate.
- [ ] Catalog refresh and quota snapshots are revisioned, bounded and recoverable.

## Не входит

Reliability aggregation, route scoring, probe scheduling execution, IPC/UI, provider calls and automatic onboarding without evidence.
