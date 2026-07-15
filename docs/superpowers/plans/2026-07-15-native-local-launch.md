# Нативный локальный запуск EvoHime — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Запускать EvoHime на Windows без Docker с нативным PostgreSQL, локальными Rust/Vite процессами и доступом backend к авторизации `gh` хоста.

**Architecture:** `scripts/setup-local.ps1` подготавливает PostgreSQL 16, роль, базу и миграции. `start-dev.ps1` проверяет инструменты и базу, затем управляет локальными server/web процессами. Docker Compose остаётся альтернативным deployment-путём.

**Tech Stack:** PowerShell 5.1+, PostgreSQL 16, Rust/Cargo, Node.js/npm, React/Vite, Axum/SQLx.

## Global Constraints

- Не удалять и не ломать Docker Compose.
- Не затирать `LITEROUTER_API_KEY`, заданный пользователем.
- Не запускать frontend/backend, если PostgreSQL недоступен.
- Все команды и сообщения launcher должны работать в Windows PowerShell.
- После завершения выполнить `cargo fmt --all -- --check`, `cargo test --workspace` и frontend build.

### Task 1: Добавить подготовку локального PostgreSQL

**Files:**
- Create: `scripts/setup-local.ps1`
- Modify: `start-dev.ps1`
- Test: `scripts/setup-local.tests.ps1`

**Interfaces:**
- `setup-local.ps1` принимает `-InstallPostgres`, `-ApplyMigrations` и использует `localhost:5432`, базу `evohime`, роль `evohime`, пароль `evohime`.
- Launcher вызывает setup без повторной установки уже работающего PostgreSQL.

- [ ] **Step 1: Write the failing checks** для отсутствующего `psql`, недоступного порта и успешного подключения к базе.
- [ ] **Step 2: Run checks** и убедиться, что чистая машина получает диагностируемый FAIL.
- [ ] **Step 3: Реализовать установку PostgreSQL 16** через официальный Windows installer, если `psql.exe` и служба PostgreSQL 16 отсутствуют; после установки добавить `bin` в текущий PATH.
- [ ] **Step 4: Создать роль и базу** через `psql`, используя `CREATE ROLE ...`, `CREATE DATABASE ...` с идемпотентной проверкой существования.
- [ ] **Step 5: Применить миграции** последовательно из `migrations/*.sql` через `psql` к `DATABASE_URL`.
- [ ] **Step 6: Повторно выполнить проверки** и убедиться, что setup идемпотентен.
- [ ] **Step 7: Commit** `feat: add native postgres setup`.

### Task 2: Укрепить локальный launcher

**Files:**
- Modify: `start-dev.ps1`
- Modify: `start-dev.bat`
- Modify: `start-dev.vbs`

**Interfaces:**
- `start-dev.ps1 -Setup` подготавливает PostgreSQL и завершает работу.
- Обычный `start-dev.ps1` вызывает подготовку, проверяет `cargo`, `npm`, `gh`, порты `3000` и `5173`, затем запускает оба процесса.

- [ ] **Step 1: Добавить проверки зависимостей** с понятными сообщениями для Cargo, Node/npm, gh и PostgreSQL.
- [ ] **Step 2: Загружать `.env` поверх defaults** так, чтобы заданный `LITEROUTER_API_KEY` сохранялся, а значения `DATABASE_URL`, `BIND_ADDR`, `WORKSPACE_ROOT` и `DEMO_FILE_PATH` имели локальные defaults.
- [ ] **Step 3: Добавить `-Setup` и запуск setup-скрипта** перед созданием server/web процессов.
- [ ] **Step 4: Добавить проверки health и завершения дочерних процессов** до показа панели пользователю.
- [ ] **Step 5: Обновить BAT/VBS-обёртки** для явного вызова локального PowerShell launcher.
- [ ] **Step 6: Commit** `feat: harden native local launcher`.

### Task 3: Обновить документацию

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/current-state.md`

- [ ] **Step 1: Описать первичный запуск**: `.\scripts\setup-local.ps1 -InstallPostgres -ApplyMigrations`, затем `.\start-dev.ps1`.
- [ ] **Step 2: Описать проверку GitHub** через `gh auth status` и `/api/auth/github`.
- [ ] **Step 3: Переместить Docker Compose в раздел альтернативного запуска**, сохранив команды compose.
- [ ] **Step 4: Проверить документацию на противоречия** с реальными скриптами.
- [ ] **Step 5: Commit** `docs: document native local launch`.

### Task 4: Полная верификация

**Files:**
- Test: `scripts/setup-local.tests.ps1`
- Test: `start-dev.ps1 -Setup`

- [ ] **Step 1: Запустить** `cargo fmt --all -- --check`.
- [ ] **Step 2: Запустить** `cargo test --workspace`.
- [ ] **Step 3: Запустить** `npm run generate:protocol`, затем выполнить `Push-Location frontend/web; npm run build; Pop-Location`.
- [ ] **Step 4: Запустить** нативный setup и проверить `pg_isready`, `/health`, `/api/auth/github` и frontend `http://localhost:5173`.
- [ ] **Step 5: Проверить** совпадение `gh api user --jq .login` и поля `login` API.
- [ ] **Step 6: Убедиться**, что `git status` содержит только намеренные изменения и Docker Compose не повреждён.
- [ ] **Step 7: Commit** `test: verify native local launch` только если появились отдельные тестовые изменения.
