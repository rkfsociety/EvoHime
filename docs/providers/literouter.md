# LiteRouter — первый LLM-провайдер EvoHime

> **Статус:** активен (этап 2, Milestone 1 завершён)
> **Приоритет:** первый провайдер для `model-gateway`

LiteRouter — OpenAI-compatible API. EvoHime использует его как основной провайдер на этапе 2.

---

## Конфигурация для EvoHime

### Переменные окружения

```env
LITEROUTER_API_KEY=lr_...          # API key из раздела "API Keys"
LITEROUTER_BASE_URL=https://api.literouter.com/v1
LITEROUTER_MODEL=deepseek:free     # или mistral:free, llama:free и т.д.
```

### HTTP

| Параметр | Значение |
| --- | --- |
| Base URL | `https://api.literouter.com/v1` |
| Chat completions | `POST /v1/chat/completions` |
| Auth | `Authorization: Bearer <api_key>` |
| Format | OpenAI-compatible |

### Рекомендуемые модели (free tier)

- `deepseek:free`
- `mistral:free`
- `llama:free`

Актуальный список — в меню **Available Models** на сайте LiteRouter.

---

## Интеграция в EvoHime

```text
agent-runtime
    → model-gateway
        → providers/literouter.rs
            → POST https://api.literouter.com/v1/chat/completions
            → SSE stream → agent.message.delta
```

### Crate

`crates/model-gateway/src/providers/literouter.rs`

### Задачи (Milestone 1)

- [ ] `LiteRouterConfig` из env
- [ ] OpenAI-compatible chat completions client
- [ ] Streaming (SSE) → токены
- [ ] Обработка ошибок API
- [ ] Тесты с mock HTTP

---

## Справка: настройка на других платформах

Ниже — оригинальная документация LiteRouter для внешних клиентов.  
В EvoHime эти настройки **не используются напрямую** — только как справка по API.

### Basic Configuration

- **Base URL:** `https://api.literouter.com`
- **API Key:** из раздела «API Keys» → `Authorization: Bearer <key>`
- **Models:** `deepseek:free`, `mistral:free`, `llama:free`, …

### Cursor

- Base URL: `https://api.literouter.com/v1`
- API Key: ваш ключ
- Model: `deepseek:free` / `mistral:free` / `llama:free`

### Generic (OpenAI-compatible)

```text
Base URL:  https://api.literouter.com/v1
API Key:   Bearer <your_key>
Model:     deepseek:free
Endpoint:  /v1/chat/completions
```

### Janitor AI

- Proxy URL: `https://api.literouter.com/v1/chat/completions`
- API Key: ваш ключ LiteRouter
- Model: `deepseek:free`, `mistral:free`, `llama:free`, …

### SillyTavern

- API Type: Custom (OpenAI-compatible)
- Server URL: `https://api.literouter.com/v1`
- Model: `deepseek:free`, …
- Bypass status check: при необходимости

### Другие платформы

Wyvern, Agnaistic, Risu AI, Chub AI, Sophia's LoreBary — все используют тот же endpoint:

```text
https://api.literouter.com/v1/chat/completions
```

---

## Ссылки

- Сайт: https://literouter.com
- API Keys: раздел «API Keys» в личном кабинете
- Available Models: меню моделей на сайте
