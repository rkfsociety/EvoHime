# LiteRouter — LLM-провайдер Евы

Статус: активный OpenAI-compatible provider для локального Rust Core и текущий
default route.

Ева работает как локальный Windows-клиент, а Core при необходимости обращается к LiteRouter по HTTPS. LiteRouter не является частью установочного файла и не заменяет локальный Core.

## Конфигурация

```env
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_BASE_URL=https://api.literouter.com/v1
# Модель выбирается из актуального ответа GET /models.
LITEROUTER_MODEL=
```

В пользовательском приложении ключ вводится в настройках, шифруется ОС (DPAPI через Electron `safeStorage`) и хранится в `%LOCALAPPDATA%\EvoHime\shell\provider.json`; Core получает его окружением от supervisor.

В headless Linux CLI `eva` автоматически читает пользовательский файл
`$XDG_CONFIG_HOME/evohime/provider.env`; если `XDG_CONFIG_HOME` не задан, путь
по умолчанию — `~/.config/evohime/provider.env`. Пример содержимого:

```env
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_MODEL=deepseek:free
```

Каталог и файл автоматически получают права `0700` и `0600`; ключ хранится в
этом локальном текстовом файле. Переменные, заданные в окружении процесса,
имеют приоритет над значениями файла. Core читает конфигурацию при запуске.
Если файла нет, `eva` создаст закрытый шаблон, подскажет путь и дождётся ключа
до запуска Core. Переменные окружения также поддерживаются для локальной
разработки и CI secrets. Не записывайте ключ в Git, SQLite, task events или
diagnostics.

Общее время model request задаёт Core: `EVOHIME_MODEL_TIMEOUT_SECS` по
умолчанию равен 120 секундам, а значение `0` отключает дедлайн; дедлайн задачи
по умолчанию равен 900 секундам и отключается через
`EVOHIME_TASK_TIMEOUT_SECONDS=0`. HTTP-клиент LiteRouter не задаёт отдельный
общий дедлайн ответа и оставляет 15-секундный лимит только на установление
соединения.

Список моделей берётся динамически. Выбор API-модели из чата применяется к
следующему запросу без перезапуска Core; переключение API-профиля или сохранение
ключа обновляет окружение и перезапускает Core. Для self-repair provider и model
выбираются явно до запуска repair-run.

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
