# LiteRouter — LLM-провайдер Евы

Статус: активный OpenAI-compatible provider для локального Rust Core.

Ева работает как локальный Windows-клиент, а Core при необходимости обращается к LiteRouter по HTTPS. LiteRouter не является частью установочного файла и не заменяет локальный Core.

## Конфигурация

```env
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_BASE_URL=https://api.literouter.com/v1
LITEROUTER_MODEL=deepseek:free
```

Ключ должен передаваться через Windows Credential Manager/DPAPI в пользовательском приложении; переменные окружения допустимы только для локальной разработки и CI secrets. Не записывайте ключ в Git, SQLite, task events или diagnostics.

## Поток данных

```text
EvoHime.exe → named pipe → evohime-core.exe
                         → model-gateway
                         → LiteRouter HTTPS/SSE
                         → Core event journal
                         → EvoHime.exe timeline
```

Реализация находится в `crates/model-gateway/src/providers/literouter.rs`. Поддерживаются streaming, native tool calls, ошибки API и bounded retry/backoff. Список моделей определяется самим LiteRouter и может меняться.

## Retry-параметры

| Переменная | По умолчанию | Назначение |
| --- | --- | --- |
| `EVOHIME_LLM_MAX_RETRIES` | `3` | повторов после первой попытки |
| `EVOHIME_LLM_RETRY_BASE_MS` | `250` | база exponential backoff |
| `EVOHIME_LLM_RETRY_MAX_MS` | `5000` | верхняя граница backoff |

Повторяются transport errors и HTTP `408/429/500/502/503/504`; mid-stream запрос после начала токенов не повторяется.
