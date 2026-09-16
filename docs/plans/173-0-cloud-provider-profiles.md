# План 173.0 — Cloud Provider Profiles

Статус: предложено по [issue #105](https://github.com/rkfsociety/EvoHime/issues/105). Это implementation contract; функционал этим документом не считается реализованным.

## Цель

Добавить Core-owned profiles и bounded model catalog для OpenRouter, Groq,
Google Gemini, Mistral, Cloudflare Workers AI, NVIDIA NIM, Cerebras и Hugging
Face Inference Providers. Provider identity, catalog, capabilities, limits,
privacy metadata и credential references должны быть отделены от уже
существующего wire transport. `ModelGateway` остаётся единственной границей
исполнения.

Текущий checkout содержит `ProviderKind`, `ModelRouteConfig`, общий
`OpenAICompatibleProvider`, LiteRouter/Ollama catalog и минимальный
`free_provider_reliability_routing` contract. План расширяет эти владельцы, не
создавая копий HTTP-клиента, второго gateway или renderer-owned routing.

## Архитектурная граница

~~~text
Core profile/catalog registry -> ModelGateway route resolution -> transport
-> normalized response/usage/health -> authenticated IPC -> redacted UI
~~~

Затронутые источники истины: `crates/model-gateway/src/{config.rs,lib.rs,
provider_contract.rs,providers/}`, `crates/evohime-core/src/
free_provider_reliability_routing.rs` и существующие model catalog IPC paths.
Renderer (`ProviderForm`, `ModelPicker`, `provider-state`) только отображает
Core projection.

## Этапы

- [Этап 1 — Core-контракт, schema и storage](./173-1-cloud-provider-profiles.md)
- [Этап 2 — runtime, discovery и recovery](./173-2-cloud-provider-profiles.md)
- [Этап 3 — IPC, projection и UI](./173-3-cloud-provider-profiles.md)
- [Этап 4 — verification, release evidence и закрытие](./173-4-cloud-provider-profiles.md)

## Зависимости

### Блокирующие

- Реализованные Model Gateway, credential boundary, capability/privacy policy,
  resilience/reliability routing (#125), authenticated IPC и SQLite migration/
  backup primitives.
- Текущие `ProviderKind`/`ModelRouteConfig` и LiteRouter/Ollama catalog paths;
  точные новые schema/IPC revisions назначаются после сверки свободных номеров.

### Опциональные

- Provider-native adapter только после доказанного capability gap;
  deterministic mock fixtures и CI contract tests не зависят от live credentials.

## Критерии готовности

- [ ] Provider identity отделена от transport и имеет version/hash provenance.
- [ ] Все восемь initial provider profiles имеют bounded status/configuration;
  generic-compatible transport переиспользуется там, где это возможно.
- [ ] Catalog snapshots имеют freshness, lifecycle, capability provenance и
  deterministic deduplication; stale/unknown не превращаются в supported/free.
- [ ] Credential references не раскрываются в renderer, prompt, logs или artifacts.
- [ ] Capability preflight и provider health участвуют в Core routing до execution.
- [ ] IPC/UI показывают только redacted provider/model metadata.
- [ ] Контракты, malformed input, provider failures, recovery и security
  покрыты тестами; после реализации contract/state/evidence перенесены в docs,
  а комплект плана удалён.

## Non-goals

Billing/payment, автоматический signup, публичный proxy, полный зеркальный
каталог провайдера, silent model replacement, arbitrary catalog URLs и новый
HTTP stack на каждого провайдера.

## Связанный issue

- [#105 Cloud Provider Profiles](https://github.com/rkfsociety/EvoHime/issues/105)
