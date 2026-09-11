# LLM Providers

| Provider | Status | Docs |
| --- | --- | --- |
| **LiteRouter** | ✅ Active (current default) | [literouter.md](literouter.md) |
| OpenAI-compatible (`openai_compatible`) | ✅ Active | — |
| OpenAI Responses (`openai_responses`) | ✅ Active | [openai-and-codex.md](openai-and-codex.md) |
| Mock (`mock`) | ✅ Только для тестов | — |
| Anthropic | Planned | — |
| Ollama (`ollama`) | ✅ Active (локально) | [ollama.md](ollama.md) |

**Правило:** первый и текущий облачный провайдер в EvoHime — **LiteRouter**;
Ollama работает как локальный провайдер без API-ключа.

**Примечание:** Core уже поддерживает несколько маршрутов модели и выбор на уровне задачи. Любой маршрут может указывать на OpenAI-compatible endpoint с отдельным ключом, базовым URL и моделью.

Для GPT-5-Codex и других Responses-моделей используйте `openai_responses`.

## Настройка

В приложении провайдер, модель, base URL и ключ задаются в одном блоке настроек (шестерёнка рядом с аккаунтом). Выбор уже настроенного провайдера автоматически сохраняется и сразу применяет профиль через перезапуск Core — отдельная кнопка для этого не нужна. Ключ шифруется ОС и хранится в `%LOCALAPPDATA%\EvoHime\shell\provider.json`; изменение ключа или base URL сохраняется кнопкой и также автоматически перезапускает Core. Выбор API-модели прямо в чате действует со следующего запроса без перезапуска. Выбор модели Codex CLI сохраняется отдельно в `shell\codex.json` и перезапускает Core.

Вкладка «Ревью планов» использует тот же каталог доступных моделей: 2–8
reviewer-моделей и отдельная synthesis-модель. Выбор применяется только к
этим вызовам и не меняет модель обычного чата; review-запросы не имеют tools и
ограничены Markdown-файлом до 512 КБ.

Self-repair также требует явного выбора provider и model до запуска; выбор
передаётся через весь repair-run и не меняет модель обычного чата.

Base URL принимается только по `https` либо по `http` на loopback. Для локальной разработки те же значения можно задать в `.env` (см. `.env.example` в корне).
