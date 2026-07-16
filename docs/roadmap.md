# EvoHime — Дорожная карта

> Обновлено: 2026-07-16

## Обзор

```text
Этап 1 ✅ ──→ Этап 2 ──→ Этап 3 ──→ Этап 4 ──→ Этап 5 ──→ Этап 6
Фундамент     Модель      Tools       Editor      Tasks       Advanced
```

| Этап | Название | Статус | Ключевой результат |
| --- | --- | --- | --- |
| 1 | Фундамент | ✅ Готово | Vertical slice работает |
| 2 | Чат с моделью | ✅ Готово | LiteRouter streaming |
| 3 | Tools + shell | ✅ Готово | Sandbox, filesystem tools, shell, approvals, terminal, permissions |
| 4 | Editor + Git | ✅ Done | Browser file tree, Monaco editor, Git status/diff/actions, and synchronization events |
| 5 | Оркестрация | ✅ Complete | Lifecycle, команды, storage и recovery готовы |
| 6 | Advanced | 🟡 В процессе | Foundations complete; ML handlers, Rust-owned worker jobs, and production hardening remain |

---

## Milestone 0 — Vertical slice ✅

**Цель:** доказать, что весь pipeline работает.

```text
Сообщение → задача → stream → filesystem.read → UI → PostgreSQL
```

### Чеклист

- [x] `POST /api/sessions` — создание сессии
- [x] `WS /ws/:session_id` — real-time события
- [x] `user.message` — команда клиента
- [x] `task.started` / `task.completed` / `task.failed`
- [x] `agent.message.delta` — потоковый ответ
- [x] `agent.plan.updated` — план агента
- [x] `tool.started` / `tool.output` / `tool.completed`
- [x] `filesystem.read` с защитой path traversal
- [x] История в `session_events` (PostgreSQL)
- [x] Frontend: чат + timeline событий
- [x] Native local launcher: PostgreSQL + server + web
- [x] JSON Schema → TypeScript codegen

### Критерий готовности

Пользователь отправляет сообщение в браузере, видит потоковый ответ, результат `filesystem.read`, и после перезагрузки история доступна через API.

---

## Milestone 1 — Этап 2: Чат с моделью ✅

**Зависимости:** Milestone 0 ✅

**Цель:** заменить демо-агент на реальную LLM.

### Deliverables

| # | Задача | Статус |
| --- | --- | --- |
| 2.1 | Model gateway: абстракция провайдера | ✅ |
| 2.2 | LiteRouter адаптер (OpenAI-compatible) | ✅ |
| 2.3 | Streaming токенов → `agent.message.delta` | ✅ |
| 2.4 | Конфигурация модели через env / API | ✅ |
| 2.5 | История сообщений в БД | ✅ |
| 2.6 | `agent_loop.rs` вместо demo vertical slice | ✅ |
| 2.7 | UI: панель Settings | ✅ |
| 2.8 | Тесты model-gateway + agent-runtime | ✅ |

### Критерий готовности

Агент отвечает через выбранную LLM с потоковой передачей. История сообщений сохраняется и восстанавливается при переподключении.

---

## Milestone 2 — Этап 3: Tools, shell, permissions

**Зависимости:** Milestone 1

**Цель:** агент работает с файловой системой, shell и запрашивает разрешения.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 3.1 | `filesystem.write` | ✅ `crates/tool-runtime/` |
| 3.2 | `filesystem.patch` | ✅ `crates/tool-runtime/` |
| 3.3 | `filesystem.search` (ripgrep) | ✅ `crates/tool-runtime/` |
| 3.4 | `shell.execute` с песочницей | ✅ `crates/tool-runtime/` |
| 3.5 | Permission engine: check + request | ✅ `crates/permissions/` |
| 3.6 | `approval.required` event + UI modal | ✅ `protocol`, `server`, `frontend/web/` |
| 3.7 | `approval.granted` / `approval.denied` commands | ✅ `protocol`, `server` |
| 3.8 | xterm.js терминал (панель Terminal) | ✅ `frontend/web/` |
| 3.9 | Таймауты и отмена инструментов | ✅ `crates/tool-runtime/` |
| 3.10 | UI: настройки разрешений | ✅ `frontend/web/` |
| 3.11 | Тесты: sandbox, permissions, каждый tool | ✅ `crates/tool-runtime/`, `crates/permissions/`, `crates/server/` |

### Критерий готовности

Агент читает/пишет файлы, выполняет shell-команды. Опасные действия требуют подтверждения в UI. Терминал показывает вывод.

---

## Milestone 3 — Этап 4: Editor, files, Git ✅

**Зависимости:** Milestone 2

**Цель:** полноценная работа с кодом через браузер.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 4.1 | API: список файлов / содержимое / сохранение | ✅ `crates/server/` |
| 4.2 | Дерево файлов (панель Files) | ✅ `frontend/web/` |
| 4.3 | Monaco Editor (панель Editor) | ✅ `frontend/web/` |
| 4.4 | `git.status`, `git.diff` tools | ✅ `crates/tool-runtime/` |
| 4.5 | `git.commit`, `git.pull`, `git.push` tools | ✅ `crates/tool-runtime/` |
| 4.6 | Git diff viewer (панель Git) | ✅ `frontend/web/` |
| 4.7 | `file.changed` event | ✅ protocol, server |
| 4.8 | `git.diff.changed` event | ✅ protocol, server |
| 4.9 | Тесты git tools | `crates/tool-runtime/` |

### Критерий готовности

Пользователь навигирует по файлам, редактирует в Monaco, видит Git status/diff, агент может коммитить через инструменты.

---

## Milestone 4 — Этап 5: Task orchestration ✅

**Зависимости:** Milestone 3

**Цель:** надёжный оркестратор задач с recovery.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 5.1 | Реальное планирование (LLM → plan steps) | ✅ `crates/agent-runtime/` |
| 5.2 | Параллельное выполнение независимых tools | ✅ `crates/tool-runtime/`, `crates/task-engine/` |
| 5.3 | `task.cancel` command | ✅ protocol, server |
| 5.4 | Отмена running tools | ✅ `crates/tool-runtime/`, `crates/server/` |
| 5.5 | `task.resume` — продолжение с checkpoint | ✅ `crates/task-engine/`, `crates/server/` |
| 5.6 | Повторный запуск failed tasks | ✅ `crates/task-engine/` |
| 5.7 | Recovery после restart сервера | ✅ `crates/task-engine/`, `crates/storage/`, `crates/server/` |
| 5.8 | Панель Tasks — список и статусы | ✅ `frontend/web/` |
| 5.9 | Панель Actions — журнал действий | ✅ `frontend/web/` |
| 5.10 | Тесты: cancel, resume, recovery | ✅ `crates/task-engine/`, `crates/agent-runtime/`, `crates/server/` |

### Критерий готовности

Задачи можно остановить, продолжить и перезапустить. После рестарта сервера running tasks восстанавливаются. Независимые tools выполняются параллельно, а план шагов сохраняется в `task_steps` и отображается в history/events.

**Статус:** Milestone 4 завершён.

---

## Milestone 5 — Этап 6: Advanced

**Зависимости:** Milestone 4

**Цель:** production-ready агент с расширяемой экосистемой.

### Deliverables

| # | Задача | Crate / Path |
| --- | --- | --- |
| 6.1 | Project index (embedding / ripgrep) | ✅ `crates/project-index/` |
| 6.2 | Контекстный поиск для агента | ✅ `crates/agent-runtime/` |
| 6.3 | `mcp.call` tool | ✅ `crates/tool-runtime/` |
| 6.4 | MCP server management UI | ✅ `frontend/web/`, `crates/server/` |
| 6.5 | Agent memory (persistent context) | ✅ `crates/storage/`, `crates/agent-runtime/` |
| 6.6 | Multi-model routing: task-scoped routes + OpenAI-compatible providers | ✅ `crates/model-gateway/`, `frontend/web/`, `crates/server/` |
| 6.7 | Python workers (HTTP/queue) | ✅ baseline; ML handlers next | `workers/python/` |
| 6.8 | `browser.open`, `browser.extract` | ✅ `crates/tool-runtime/` |
| 6.9 | Settings: models, permissions, MCP, tools | ✅ `frontend/web/`, `crates/server/` |
| 6.10 | Тесты: index, MCP, workers | ✅ respective crates / `workers/python/` |

### Критерий готовности

Агент использует project index для контекста, вызывает MCP-инструменты, работает с несколькими моделями. Python workers принимают структурированные HTTP jobs; следующий шаг — специализированные ML handlers, Rust-owned persistence/retries и production hardening.

---

## Матрица: панели UI × этапы

| Панель | Этап | Milestone | Статус |
| --- | --- | --- | --- |
| Chat | 1 | M0 | ✅ Активна |
| Events (timeline) | 1 | M0 | ✅ Активна |
| Files | 4 | M3 | ✅ Complete |
| Editor | 4 | M3 | ✅ Complete |
| Terminal | 3 | M2 | ✅ Active |
| Git | 4 | M3 | ✅ Complete |
| Tasks | 5 | M4 | ✅ Активна, базовая |
| Actions | 5 | M4 | ✅ Активна, базовая |
| Settings | 2 | M1 | ✅ Активна, модели + permissions |

---

## Матрица: инструменты × этапы

| Tool | Этап | Milestone | Статус |
| --- | --- | --- | --- |
| `filesystem.read` | 1 | M0 | ✅ |
| `filesystem.write` | 3 | M2 | ✅ Backend |
| `filesystem.patch` | 3 | M2 | ✅ Backend |
| `filesystem.search` | 3 | M2 | ✅ Backend |
| `shell.execute` | 3 | M2 | ✅ Backend |
| `git.status` | 4 | M3 | ✅ Backend |
| `git.diff` | 4 | M3 | ✅ Backend |
| `git.commit` | 4 | M3 | ✅ Backend |
| `git.pull` | 4 | M3 | ✅ Backend |
| `git.push` | 4 | M3 | ✅ Backend |
| `browser.open` | 6 | M5 | ✅ |
| `browser.extract` | 6 | M5 | ✅ |
| `mcp.call` | 6 | M5 | ✅ |

---

## Принципы разработки

1. **Сначала vertical slice** — каждый этап начинается с минимального рабочего сценария
2. **Сервер — источник истины** — frontend только рендерит события
3. **Один PR — один milestone-deliverable** — не смешивать этапы
4. **Тесты до merge** — каждый tool и crate покрыт тестами
5. **Миграции обязательны** — любое изменение схемы БД через `migrations/`
6. **Протокол first** — сначала JSON Schema, потом Rust + TS

---

## Связанные документы

- [development-plan.md](development-plan.md) — полный план разработки
- [current-state.md](current-state.md) — что реализовано сейчас
- [architecture.md](architecture.md) — архитектура компонентов
