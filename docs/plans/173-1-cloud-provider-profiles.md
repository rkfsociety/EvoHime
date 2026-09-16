# План 173.1 — Cloud Provider Profiles: Core, schema и storage

## Зависимости

### Блокирующие

- [Обзор плана 173](./173-0-cloud-provider-profiles.md).
- Существующие `ModelRouteConfig`, `ProviderKind`, credential references,
  Core-owned metadata migrations и #125 reliability metadata.

### Опциональные

- Existing provenance/diagnostics projection helpers.

## Реализация

- В `crates/model-gateway` ввести versioned `ProviderProfile`, opaque
  credential binding, `ProviderModelDescriptor`, normalized capability flags с
  provenance, typed limits/privacy/usage metadata и catalog snapshot/lifecycle.
- Разделить provider family, transport kind и native capability gaps; не
  передавать secrets, raw response или arbitrary endpoint из catalog payload.
- Зарегистрировать bounded built-in profiles для OpenRouter, Groq, Gemini,
  Mistral, Cloudflare Workers AI, NVIDIA NIM, Cerebras и Hugging Face, сохранив
  LiteRouter/OpenAI/Ollama/Local/Mock обратную совместимость.
- Добавить transactional metadata storage через существующий Core/local
  storage owner с revision fence, content hash, idempotency, backup/recovery и
  bounded catalog size/count. Не создавать второй registry или CRUD store.

## Критерии

- Duplicate/oversized/malformed profiles и model descriptors отклоняются
  детерминированно; unknown capability остаётся unknown.
- Catalog/profile revision и hash входят в immutable route/model provenance.
- Credential refs metadata-only и scoped к provider; secrets отсутствуют в
  serialized snapshots и storage projection.
- Existing route config и old providers продолжают загружаться без миграции
  пользовательских secrets.

## Non-goals

Network discovery, live probes, routing policy execution и UI относятся к
следующим этапам.
