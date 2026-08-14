# LLM Providers

| Provider | Status | Docs |
| --- | --- | --- |
| **LiteRouter** | ✅ Active (current default) | [literouter.md](literouter.md) |
| OpenAI-compatible (`openai_compatible`) | ✅ Active | — |
| Mock (`mock`) | ✅ Только для тестов | — |
| Anthropic | Planned | — |
| Ollama | Planned | — |

**Правило:** первый и текущий провайдер в EvoHime — **LiteRouter** (OpenAI-compatible API).

**Примечание:** Core уже поддерживает несколько маршрутов модели и выбор на уровне задачи. Любой маршрут может указывать на OpenAI-compatible endpoint с отдельным ключом, базовым URL и моделью.

## Настройка

В приложении провайдер, модель, base URL и ключ задаются в одном блоке настроек (шестерёнка рядом с аккаунтом). Ключ шифруется ОС и хранится в `%LOCALAPPDATA%\EvoHime\shell\provider.json`; его сохранение перезапускает Core, поскольку gateway собирается из окружения при старте. Модель дополнительно выбирается прямо в чате — это применяется без перезапуска.

Base URL принимается только по `https` либо по `http` на loopback. Для локальной разработки те же значения можно задать в `.env` (см. `.env.example` в корне).
