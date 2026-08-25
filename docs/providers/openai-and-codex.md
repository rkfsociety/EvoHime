# OpenAI API и Codex CLI

EvoHime поддерживает два связанных, но независимых способа работы с OpenAI.

## OpenAI Responses API

В настройках провайдера выберите `OpenAI Responses / Codex`, укажите API-ключ и
модель. По умолчанию используется `https://api.openai.com/v1` и
`gpt-5-codex`. Ключ шифруется Electron `safeStorage`, не попадает в renderer,
логи, SQLite или Git и передаётся Core только через окружение supervisor.

Responses API используется для потокового ответа и function calling. Для новых
Codex-моделей этот маршрут предпочтительнее OpenAI-compatible Chat Completions.

## Codex с входом через ChatGPT

Codex CLI — отдельный локальный исполнитель. Если он установлен и пользователь
выполнил штатный вход `codex` → `Sign in with ChatGPT`, EvoHime может вызвать его
из Core через уже существующий инструмент `shell.execute` и `core.terminalExecute`.
Запуск должен идти в выбранном Git-репозитории, с рабочим каталогом проекта и
ограничением `read-only` либо `workspace-write`. Любая запись, команда сборки,
удаление или публикация требует обычного подтверждения EvoHime.

Нельзя переносить cookies, внутренние токены ChatGPT или содержимое каталога
Codex в настройки EvoHime. ChatGPT-вход остаётся собственностью Codex CLI.

## Биллинг и безопасность

Подписка ChatGPT и использование OpenAI API являются разными продуктами и
учитываются раздельно. API-ключ никогда не должен находиться в renderer или в
аргументах командной строки.
