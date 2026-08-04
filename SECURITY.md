# Политика безопасности EvoHime

EvoHime — локальный single-user Windows-клиент. Пользовательский интерфейс не открывает сетевой порт: WinUI общается с Rust Core через защищённый versioned Windows named pipe.

## Защищаемые границы

- workspace path проверяется Core и ограничивается выбранным workspace;
- опасные tools требуют approval через UI;
- shell-команды получают таймаут, cancellation и ограничения вывода;
- дочерние процессы находятся в Windows Job Object и завершаются вместе с Core;
- SQLite и event journal принадлежат только Core;
- provider credentials хранятся через Windows Credential Manager/DPAPI и не попадают в логи;
- JSONL diagnostics редактируют секреты;
- IPC использует major/minor compatibility и bounded frames;
- supervisor ограничивает single-instance запуск и восстанавливает Core после сбоя.

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
.\scripts\native-workflow.tests.ps1
.\scripts\native-package.tests.ps1
cargo test --locked -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
dotnet test desktop\EvoHime.Tests\EvoHime.Tests.csproj -p:Platform=x64
```

Релизный установщик собирается только отдельным job после успешных CI-проверок.
