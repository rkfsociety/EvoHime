# Политика безопасности EvoHime

EvoHime — локальный single-user Windows-клиент. Пользовательский интерфейс не открывает сетевой порт: Electron main process общается с Rust Core через защищённый versioned Windows named pipe. WinUI сохранён только как compatibility runtime.

## Защищаемые границы

- workspace path проверяется Core и ограничивается выбранным workspace;
- опасные tools требуют approval через UI;
- shell-команды получают таймаут, cancellation и ограничения вывода;
- запуск ограничен неизменяемым бюджетом (`run_policy`): итерации, wall clock, tool calls, токены и стоимость проверяются Core перед каждым эффектом и не поднимаются из UI;
- дочерние процессы находятся в Windows Job Object и завершаются вместе с Core;
- SQLite и event journal принадлежат только Core;
- ключ провайдера шифруется ОС (DPAPI через Electron `safeStorage`), хранится в `%LOCALAPPDATA%\EvoHime\shell\provider.json` с режимом `600` и передаётся Core только через окружение supervisor; renderer видит лишь признак «ключ задан», в логи значение не попадает. Если ОС не может зашифровать, ключ не сохраняется;
- base URL провайдера принимается только по `https` либо по `http` на loopback, чтобы ключ не ушёл на произвольный хост;
- JSONL diagnostics редактируют секреты;
- IPC использует major/minor compatibility и bounded frames;
- локальное состояние оболочки (`shell\workspaces.json`, `shell\chats.json`) — только UI-группировка: оно не выдаёт прав, и Core заново проверяет каждую команду;
- supervisor ограничивает single-instance запуск и восстанавливает Core после сбоя; локальный Pulse digest не маскирует пропущенный или неуспешный запуск успехом.

## Ограничения

- скомпрометированная локальная Windows-машина вне scope;
- prompt injection из текста репозитория и внешних данных остаётся риском модели;
- сетевые model providers могут видеть отправленный им контекст согласно их политике;
- multi-user, SaaS и server deployment не поддерживаются;
- MCP и внешние сетевые tools требуют отдельного permission/approval контроля.

## Данные и диагностика

Локальные данные находятся в `%LOCALAPPDATA%\EvoHime`. Перед миграциями и обновлениями должен создаваться backup. Для диагностики используйте JSONL-логи Core/supervisor и штатный export diagnostics; не прикладывайте секреты, API keys или исходники целиком.

## Сообщение об уязвимости

Не создавайте публичный issue с рабочим exploit. Отправьте описание, сценарий воспроизведения и, если возможно, безопасный PoC на `romankuzminvital@gmail.com`.

## Проверки перед релизом

```powershell
.\scripts\native-package.tests.ps1
cargo test --locked -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cd desktop\evohime-electron; npm run check:protocol; npm test; npm run build; npm run check:bundle
dotnet test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
```

Релизный установщик собирается только отдельным job после успешных CI-проверок.
