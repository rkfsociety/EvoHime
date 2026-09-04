# EvoHime — Agent Guide

EvoHime — локальный Windows AI-agent. Текущий продукт — Electron desktop application с Rust Core, SQLite и Windows supervisor.

## Общение

- Отвечай только на русском языке.
- Обращайся к пользователю «Роман».
- Используй мужской род.

## Архитектура

```text
EvoHime.exe               Electron main + bundled renderer
        │ named pipe, desktop-ipc-v1
evohime-core.exe           Rust agent runtime, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, lifecycle, recovery, logs
```

UI не обращается к workspace, SQLite или model provider напрямую. Core — единственный владелец состояния и выполнения инструментов. Supervisor запускает core, ограничивает дерево процессов Job Object и восстанавливает core после аварийного завершения.

## Требования

- Windows 10 2004+ или Windows 11, x64;
- PowerShell 7 или новее: скрипты сборки и запуска в Windows PowerShell 5.1 не
  работают и останавливаются с явной ошибкой;
- Rust MSVC toolchain;
- Node.js 22 LTS — только для разработки Electron shell; в продукт внешний
  Node.js не входит.

## Команды

```powershell
# Сборка и запуск desktop-приложения
pwsh -File .\start-dev.ps1

# Запуск уже собранного desktop-пакета
pwsh -File .\start-dev.ps1 -SkipBuild

# Сборка переносимого Windows-пакета
.\scripts\build-windows-native.ps1

# Запуск без терминала (double-click): debug-сборка пакета и старт UI
.\start-agent.cmd

# Разовый прогон одной задачи через консольный режим Core
.\scripts\test-agent.ps1 -Prompt 'проверь репозиторий'

# Ревью плана и правка по нему без UI (ключ берётся тем же DPAPI-путём)
.\scripts\test-agent.ps1 -ListModels
.\scripts\test-agent.ps1 -ReviewPlan docs\plans\03-4-child-ui-and-observability.md -Reviewers 'модель-1,модель-2' -Synthesis 'модель-3' -Revise -Out C:\temp\plan.md

# Официальный headless Core-клиент (Windows companion binary)
cargo run -p evohime-cli -- doctor --json
cargo run -p evohime-cli -- run --json 'проверь репозиторий'
cargo run -p evohime-cli -- status <run-id> --json

# Поставка движка распознавания: whisper.dll, модели лестницы и манифест
# (нужны CMake и MSVC Build Tools; самому продукту CMake не требуется)
pwsh -File .\scripts\build-listener-runtime.ps1
pwsh -File .\scripts\build-listener-runtime.ps1 -Rungs tiny   # быстрый прогон без тяжёлых ступеней

# Smoke-тест упаковки
$pwsh = Join-Path $PSHOME 'pwsh.exe'
& $pwsh -NoProfile -File scripts\native-package.tests.ps1

# Rust Core foundation
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cargo check -p evohime-supervisor

# Electron shell (desktop\evohime-electron)
cd desktop\evohime-electron
npm run bootstrap        # npm ci без lifecycle-скриптов + allow-list installers
npm run check:protocol   # генерируемые IPC-типы совпадают с каноническим proto
npm run typecheck
npm test                 # adapter, security policy, preload, real-Core E2E (skip без Core)
$env:EVOHIME_UPDATE_E2E='1'; npx vitest run tests/e2e/source-update.e2e.test.ts   # реальное обновление: клон, пересборка, подмена пакета (~7 мин)
npm run build; npm run check:bundle   # статические security-проверки production bundles
npm run dev              # dev-запуск оболочки
npm run package          # распакованный Windows package в release\win-unpacked
```

Real-Core E2E тесты требуют собранный Core: `cargo build -p evohime-core`
(или `--release`); без бинарника они помечаются как пропущенные.

Для текущих desktop-задач используй Electron shell в `desktop/evohime-electron`, Rust crates и Windows packaging scripts. Подробные решения завершённой миграции и активные работы описаны в `docs/development-plan.md`.
Electron renderer — встроенная часть desktop-приложения. Отдельный сетевой web-runtime и внешний Node.js runtime не являются частью продукта.

Если Rust-сборка останавливается на `prost-build` или другом crate, сначала проверь доступ Cargo к crates.io:

```powershell
Resolve-DnsName index.crates.io
Test-NetConnection index.crates.io -Port 443
```

## IPC

Протокол редактируется в `crates/desktop-ipc/proto/evohime.desktop.proto`. Rust transport и Electron main/preload adapter должны сохранять совместимость major-версии, sequence replay и bounded frame size. Изменение протокола требует обновить обе стороны и IPC regression tests. Генерируемые TypeScript-типы проверяются `npm run check:protocol`.

Команды `workspace.*`, `chat.*`, `provider.*`, `identity.get` и `repository.get` обслуживает main-процесс: это локальное состояние оболочки, а не права. Всё, что доходит до Core, Core проверяет заново.

Подключение к Core аутентифицируется: supervisor выдаёт launch context (`%LOCALAPPDATA%/EvoHime/runtime/session.json`, owner-only DACL) с именем pipe и session secret, Core выдаёт одноразовый nonce, клиент отвечает `HMAC-SHA256(secret, role | client_id | nonce)`. Роли: `shell` (Electron), `listener` и `cli`. Общий known-answer вектор proof продублирован в Rust и Electron тестах — менять его можно только в обеих реализациях сразу.

## Данные и диагностика

- SQLite и backup: `%LOCALAPPDATA%\EvoHime` или `EVOHIME_DATA_DIR`;
- core log: `%LOCALAPPDATA%\EvoHime\logs\core.jsonl`;
- supervisor log: `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`;
- локальное состояние оболочки: `%LOCALAPPDATA%\EvoHime\shell\` — `workspaces.json`, `chats.json`, `provider.json`;
- обновление: `%LOCALAPPDATA%\EvoHime\update.json` (пишет установщик), `source\` — git checkout, `update-staging\` — собранный пакет, `update-state\` — журнал транзакции. В dev-запуске обновление выключено; для отладки есть `EVOHIME_UPDATE_ENABLED`, `EVOHIME_UPDATE_BRANCH`, `EVOHIME_UPDATE_SOURCE_DIR`, `EVOHIME_UPDATE_INSTALL_DIR`;
- экспорт событий — JSONL через `LocalDatabase::export_events_jsonl`.

Миграции SQLite выполняются транзакционно; перед изменением схемы создаётся backup. Секреты не попадают в исходники и логи: ключ провайдера шифруется через Electron `safeStorage` (DPAPI) в `shell\provider.json` и доходит до Core только окружением supervisor; сохранение ключа перезапускает Core. Для локальной разработки используется `.env` рядом с `start-dev.ps1` по allow-list из `.env.example`.

## Правила разработки

1. Не выноси runtime-состояние из Rust Core в Electron UI.
2. Не добавляй бизнес-логику в renderer: UI отображает состояние IPC.
3. Новые Rust-функции и исправления покрывай тестами.
4. Соблюдай sandbox, таймауты, отмену и approval для опасных инструментов.
5. Перед заявлением о готовности запускай свежие проверки и проверяй `git diff --check`.
6. При крупном изменении кода актуализируй соответствующую документацию: пользовательское описание, архитектуру, текущее состояние, планы, release evidence и проектные правила — по затронутой области. Не оставляй устаревшие описания действующей архитектуры.
7. После изменений создавай task-only git-коммит в текущей ветке `main`.
8. Если во время задачи обнаружены связанные ошибки в коде, тестах, CI или правилах проекта, исправляй их в рамках той же задачи без отдельного подтверждения; не оставляй такие ошибки на потом. Если задача исправляет ошибки CI текущего проекта, после task-only коммита автоматически выполняй push, дожидайся запуска CI и проверяй его итог. Для остальных изменений исходного кода push выполняй только по прямому запросу пользователя. Если задача затрагивает только документацию или текстовые файлы, не относящиеся к исходному коду проекта, после task-only коммита выполняй push автоматически.
9. После сборки очищай `target/`, `bin/`, `obj/` и временные package artifacts, если они больше не нужны.
10. Установленную на рабочей машине версию EvoHime нельзя запускать через installer/transaction worker, переустанавливать, обновлять, останавливать или изменять для диагностики, разработки и тестов. Такие действия допустимы только после отдельного прямого запроса пользователя, явно называющего применение к установленному клиенту. Диагностика установленной версии — только чтение состояния, логов и метаданных; проверки выполняются на исходниках, временных каталогах или CI.
11. `git push` выполняй как `GIT_TERMINAL_PROMPT=0 git push origin main`: без этого флага credential manager может ждать ввода, недоступного в неинтерактивной сессии, и команда виснет до таймаута. Результат подтверждай сравнением `git ls-remote origin main` с `git rev-parse HEAD`, а не выводом самой команды — прерванный по таймауту push ничего не доказывает.
12. Если пользователь присылает ревью плана или реализации, сначала сопоставь замечания с текущим кодом и связанными документами. Обоснованные замечания исправляй сразу в рамках той же задачи, а не только пересказывай ревью; противоречивые замечания разрешай по источнику истины и явно фиксируй причину отклонения неподтверждённых замечаний. После правок проверь внутренние ссылки, зависимости, критерии готовности и согласованность с соседними этапами.

## Документы

- `docs/README.md` — карта документации и правило источника истины;
- `docs/architecture.md` — архитектура, runtime, IPC и упаковка;
- `docs/current-state.md` — подтверждённое состояние checkout;
- `docs/development-plan.md` — актуальный implementation plan;
- `docs/roadmap.md` — долгосрочные направления без деталей реализации;
- `docs/features/`, `docs/providers/`, `docs/security/` — справочные разделы;

Все планы лежат в `docs/plans/`; порядок и правило нумерации описаны в `docs/plans/README.md`. Один файл — один этап (`NN-M-slug.md`, где `M = 0` — обзор плана), поэтому этап ревьюется и выпускается отдельно. Новый план получает следующий свободный номер и обязан разделить зависимости на блокирующие и опциональные. Блокирующая зависимость от более позднего этапа означает, что номера расставлены неверно. Реализованный план удаляется из каталога: его контракт переезжает в `docs/architecture.md`, а состояние — в `docs/current-state.md`.
