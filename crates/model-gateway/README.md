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
- model route configuration is owned by Core and exposed to the desktop UI through versioned IPC
- `MODEL_ROUTES_JSON` — optional JSON map for multiple task-scoped routes
- the model is resolved per call, so the desktop `SelectModelRequest` takes effect without restarting Core; an empty value falls back to the model the route is configured with
- credentials come from the process environment: the shell hands them to the supervisor that owns Core (see [docs/architecture.md](../../docs/architecture.md)), and `.env` is the developer-only equivalent

## Status

- [x] Provider trait + `TokenStream`
- [x] LiteRouter HTTP client + SSE parsing
- [x] Separate OpenAI-compatible provider identity and `OPENAI_*` environment configuration
- [x] `MockProvider` for tests
- [x] Route-based gateway and task-scoped model selection
- [x] Integration with `agent-runtime`

## Docs

- [docs/providers/literouter.md](../../docs/providers/literouter.md)
