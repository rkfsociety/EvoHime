# LLM Providers

| Provider | Status | Docs |
| --- | --- | --- |
| **LiteRouter** | ✅ Active (current default) | [literouter.md](literouter.md) |
| OpenAI | Planned (stage 6) | — |
| Anthropic | Planned (stage 6) | — |
| Ollama | Planned (stage 6) | — |

**Правило:** первый и текущий провайдер в EvoHime — **LiteRouter** (OpenAI-compatible API).

**Примечание:** в stage 6 сервер уже умеет держать несколько маршрутов модели и выбирать их на уровне задачи. Любой маршрут может указывать на OpenAI-compatible endpoint с отдельными ключом, базовым URL и моделью.
