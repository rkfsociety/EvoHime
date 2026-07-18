# model-gateway

LLM provider abstraction. **Current default route: LiteRouter.**

## Configuration

```env
MODEL_DEFAULT_ROUTE=default
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_BASE_URL=https://api.literouter.com/v1
LITEROUTER_MODEL=deepseek:free
```

## API

- `ModelGateway::stream_chat(messages)` — token stream
- `ModelGateway::stream_chat_for_route(route, messages)` — named route token stream
- `GET /api/models/config` — exposed by server
- `MODEL_ROUTES_JSON` — optional JSON map for multiple task-scoped routes

## Status

- [x] Provider trait + `TokenStream`
- [x] LiteRouter HTTP client + SSE parsing
- [x] Separate OpenAI-compatible provider identity and `OPENAI_*` environment configuration
- [x] `MockProvider` for tests
- [x] Route-based gateway and task-scoped model selection
- [x] Integration with `agent-runtime`

## Docs

- [docs/providers/literouter.md](../../docs/providers/literouter.md)
