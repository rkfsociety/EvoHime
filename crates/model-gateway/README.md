# model-gateway

LLM provider abstraction. **First provider: LiteRouter.**

## Configuration

```env
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_BASE_URL=https://api.literouter.com/v1
LITEROUTER_MODEL=deepseek:free
```

## API

- `ModelGateway::stream_chat(messages)` — token stream
- `GET /api/models/config` — exposed by server

## Status (Milestone 1)

- [x] Provider trait + `TokenStream`
- [x] LiteRouter HTTP client + SSE parsing
- [x] `MockProvider` for tests
- [x] Integration with `agent-runtime`

## Docs

- [docs/providers/literouter.md](../../docs/providers/literouter.md)
