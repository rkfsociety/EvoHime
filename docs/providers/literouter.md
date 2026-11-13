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
# Необязательные дедлайны; 0 означает без ограничения времени.
EVOHIME_MODEL_TIMEOUT_SECS=0
EVOHIME_TASK_TIMEOUT_SECONDS=0
```

Каталог и файл автоматически получают права `0700` и `0600`; ключ хранится в
этом локальном текстовом файле. Переменные, заданные в окружении процесса,
имеют приоритет над значениями файла. Core читает конфигурацию при запуске.
Если файла нет, `eva` создаст закрытый шаблон, подскажет путь и дождётся ключа
до запуска Core. Переменные окружения также поддерживаются для локальной
разработки и CI secrets. Не записывайте ключ в Git, SQLite, task events или
diagnostics.

После настройки ключа задачу можно запустить прямо в выбранной папке:

```sh
eva run --workspace "$HOME/Projects/my-app" 'Создай небольшой Python-проект с README и hello.py'
```

Если инструменту нужно изменить файл или выполнить команду, CLI покажет
разрешение, область действия и краткое описание операции в терминале. Введите
`д` или `y`, чтобы разрешить действие; пустой ответ или любой другой текст
отклоняет его. При запуске без терминала CLI сразу отклоняет запрос разрешения,
останавливает задачу и возвращает код 3, вместо того чтобы ждать ввода.
При `--json` события остаются в stdout как NDJSON, а запрос подтверждения
показывается в stderr.

В Linux CLI общий дедлайн model request и дедлайн задачи отключены по
умолчанию: `EVOHIME_MODEL_TIMEOUT_SECS=0` и
`EVOHIME_TASK_TIMEOUT_SECONDS=0`. Их можно задать в `provider.env` или
окружении, например положительным числом секунд. Если Core запускается отдельно
от Linux CLI, его собственные значения по умолчанию — 120 секунд на model
request и 900 секунд на задачу. HTTP-клиент LiteRouter оставляет 15-секундный
лимит только на установление соединения; общего дедлайна ответа нет.

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

## Pace и rate limit

Перед каждым запросом, включая повторы, провайдер выдерживает минимальный интервал
между стартами: не менее 18,948 с для моделей с суффиксом `:free` (≈190 запросов
в час) и не менее 8 с для остальных моделей. Интервал общий для всех потоков
одного экземпляра провайдера.

LiteRouter сообщает о превышении лимита тарифа ответом HTTP `403` с текстом
`[LiteRouter] Rate limit exceeded for your tier (N seconds between messages)`.
Такой ответ считается временным rate limit, а не исчерпанной квотой: Core ждёт
указанные `N` секунд плюс запас 250 мс, повторяет запрос и запоминает интервал,
поэтому следующие запросы уже не попадают в то же окно. Если в ответе нет
подсказки и заголовка `Retry-After`, используется пауза не меньше 8 с.
Исчерпанная квота (`quota`, `daily limit`, `billing` и подобные маркеры)
по-прежнему сразу возвращает `provider quota exhausted` без повторов.

## Retry-параметры

| Переменная | По умолчанию | Назначение |
| --- | --- | --- |
| `EVOHIME_LLM_MAX_RETRIES` | `3` | повторов после первой попытки |
| `EVOHIME_LLM_RETRY_BASE_MS` | `250` | база exponential backoff |
| `EVOHIME_LLM_RETRY_MAX_MS` | `5000` | верхняя граница backoff |

Повторяются transport errors, HTTP `408/429/500/502/503/504` и временный rate limit
LiteRouter (`403` с `rate limit` в теле); для rate limit пауза берётся из подсказки
провайдера и перекрывает backoff. Mid-stream запрос после начала токенов не
повторяется. Если повторы исчерпаны, ошибка rate limit сохраняет классификацию
`rate limit`, поэтому Core применяет cooldown к маршруту, а не завершает задачу
как фатальную.
