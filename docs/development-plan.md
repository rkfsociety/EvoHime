# План разработки EvoHime

> Web-first AI-агент. Только браузер. Без Electron, desktop и мобильных клиентов.

## Цель

Создать AI-агента для работы через браузер: чат, файлы, терминал, Git, инструменты, MCP — всё в едином рабочем пространстве. Сервер на Rust оркестрирует агента, инструменты и хранение. Frontend только отображает события.

---

## Стек

| Компонент | Технология |
| --- | --- |
| Frontend | React + TypeScript + Vite |
| Backend | Rust (Axum) |
| Real-time | WebSocket |
| API | HTTP/REST |
| База данных | PostgreSQL |
| AI/ML workers | Изолированные Python workers |
| Развёртывание | Native Windows launcher + Dev Container/Compose |

---

## Архитектура

```text
Browser
   │
   ├── HTTP API
   └── WebSocket
          │
          ▼
EvoHime Server — Rust
├── Agent Runtime       (crates/agent-runtime)
├── Task Orchestrator   (crates/task-engine)
├── Model Gateway       (crates/model-gateway)
├── Tool Runtime        (crates/tool-runtime)
├── Permission Engine   (crates/permissions)
├── Project Index       (crates/project-index)
├── Event Bus           (WebSocket + storage)
└── Storage             (crates/storage)
```

---

## Основные возможности

1. Чат с AI-агентом с потоковой передачей ответа
2. Работа с проектами и директориями на сервере
3. Просмотр и редактирование файлов через Monaco Editor
4. Встроенный терминал через xterm.js
5. Выполнение shell-команд
6. Git: status, diff, commit, branch, pull и push
7. История задач, сообщений и действий агента
8. Остановка, продолжение и повторный запуск задач
9. Подтверждение опасных действий через web-интерфейс
10. Поддержка нескольких моделей и провайдеров (первый: **LiteRouter**)
11. MCP-интеграции
12. Индексация проекта для поиска релевантного контекста
13. Параллельное выполнение независимых инструментов
14. Восстановление задач после перезапуска сервера

## Фактический статус на 2026-07-30

Этапы 1–6 завершены на уровне foundations. **Stage 7 полностью завершён:** hardening/product waves `7.1`–`7.116` выполнены; `7.99` (cloud sync) закрыт целиком — owner-only push/pull операторского backup на remote endpoint, идемпотентный restore и фоновый авто-push. `7.105` Voice input / TTS, `7.106` Diff review UI и `7.107` Worktree-aware multi-checkout agent (параллельные задачи изолируются в detached-HEAD git worktree и мёржатся обратно под per-workspace локом) также реализованы.

- tools: filesystem / shell / Git / browser / MCP в `tool-runtime`;
- `agent-runtime`: native ReAct tool call → observation → next action; bounded limits; checkpoints; structured memory в prompt;
- `crates/memory`: redact / normalize / dedupe / conflict / retrieve / extract / decision gate / experience playbooks / feedback / hybrid embeddings;
- `storage` + `/api/memory`: CRUD/override; frontend MemoryPanel + MemoryAskModal;
- workers: health/stall + summarize/chunk/similarity/entities/diff/classify/language/redact handlers;
- Stage 7: auth/permissions, recovery, memory quality, product honesty для Sites/Scheduled, CI gates, Dev Container/Compose, OpenAPI contract и experimental feature flags.

**Текущий сквозной приоритет:** Stage 7 закрыт; следующий шаг — Stage 8 (Agent Intelligence, Plugin Runtime 2.0 & Local Excellence, [roadmap.md](roadmap.md) § Этап 8).

---

## Web-интерфейс

Единое рабочее пространство:

| Панель | Назначение | Этап |
| --- | --- | --- |
| Chat | Чат с агентом | 1 |
| Files | Дерево файлов | 4 |
| Editor | Monaco Editor | 4 |
| Terminal | xterm.js | 3 |
| Git | Diff, status, операции | 4 |
| Tasks | Список задач + план/паузы/retries | 5–6, deep UI ✅ |
| Actions | Журнал действий агента | 5–6, deep UI ✅ |
| Settings | Модели, разрешения, MCP | 2+ ✅ |
| Pull Requests | List/detail/create | 6 ✅ (`6.14`) |
| Memory | Override / candidates / feedback | 6 ✅ (`6.22`–`6.24`) |

---

## WebSocket-протокол

Единый типизированный протокол событий. Rust и TypeScript генерируются из JSON Schema.

### События (реализованные)

```text
session.created
task.started
agent.message.delta
agent.plan.updated
tool.started
tool.output
tool.completed
task.completed
task.failed
file.changed
git.diff.changed
task.status.changed
task.step.changed
action.logged
```

Событие `approval.required` реализовано и используется для паузы защищённых операций; события thinking и `tool.output.delta` также поддерживаются актуальным протоколом.

### Команды клиента

```text
user.message          — отправить сообщение (этап 1)
approval.granted      — подтвердить действие (этап 3)
approval.denied       — отклонить действие (этап 3)
task.cancel           — отменить задачу (этап 5)
task.resume           — продолжить задачу (этап 5)
task.retry            — повторить failed-задачу (этап 5)
```

---

## Система инструментов

Каждый инструмент обязан иметь:

- уникальное имя
- описание
- JSON Schema входных параметров
- список необходимых разрешений
- таймаут
- поддержку отмены
- структурированный результат
- журнал выполнения

### Начальный набор инструментов

| Инструмент | Этап | Статус |
| --- | --- | --- |
| `filesystem.read` | 1 | ✅ |
| `filesystem.write` | 3 | ✅ Backend |
| `filesystem.patch` | 3 | ✅ Backend |
| `filesystem.search` | 3 | ✅ Backend |
| `shell.execute` | 3 | ✅ Backend |
| `git.status` | 4 | ✅ Backend |
| `git.diff` | 4 | ✅ Backend |
| `git.commit` | 4 | ✅ Backend |
| `git.pull` | 4 | ✅ Backend |
| `git.push` | 4 | ✅ Backend |
| `browser.open` | 6 | ✅ |
| `browser.extract` | 6 | ✅ |
| `mcp.call` | 6 | ✅ |

---

## Структура репозитория

```text
evohime/
├── frontend/
│   └── web/                    # React workspace UI
├── crates/
│   ├── server/                 # HTTP + WebSocket entrypoint
│   ├── agent-runtime/          # Agent orchestration
│   ├── task-engine/            # Task lifecycle
│   ├── model-gateway/          # LLM providers
│   ├── tool-runtime/           # Tool registry + execution
│   ├── permissions/            # Permission engine
│   ├── project-index/          # Semantic search
│   ├── protocol/               # Shared event schema
│   └── storage/                # PostgreSQL access
├── workers/
│   └── python/                 # ML workers
├── migrations/                 # SQL migrations
├── docs/                       # Documentation
├── scripts/                    # Codegen, utilities
└── .cursor/rules/              # AI agent rules
```

---

## Порядок реализации

### Приоритет: минимальный вертикальный сценарий

Перед расширением функциональности должен стабильно работать:

```text
Пользователь пишет сообщение
  → сервер создаёт задачу
  → модель отвечает потоково
  → агент вызывает filesystem.read
  → результат отображается в браузере
  → история сохраняется в PostgreSQL
```

**Статус: реализован: реальный LiteRouter-провайдер подключён, demo-flow заменён `agent_loop.rs`.**

---

### Этап 1 — Фундамент ✅

Monorepo, Rust-сервер, React-интерфейс, PostgreSQL, native launcher, базовый WebSocket-протокол.

**Результат:** вертикальный сценарий работает end-to-end.

### Этап 2 — Чат с моделью ✅

**Первый провайдер: [LiteRouter](providers/literouter.md)** (OpenAI-compatible API).

- Интеграция `model-gateway` с LiteRouter
- Потоковая генерация ответа (SSE → `agent.message.delta`)
- Сессии и сохранение истории сообщений
- UI настроек моделей

Env:

```env
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_BASE_URL=https://api.literouter.com/v1
LITEROUTER_MODEL=deepseek:free
```

**Результат:** агент отвечает через LiteRouter потоково, сохраняет историю сообщений и восстанавливает контекст при следующем сообщении.

Реализовано:

- абстракция model gateway и LiteRouter SSE-адаптер;
- bootstrap-конфигурация модели через env и рабочее редактирование маршрутов через `/api/models/config` из web-панели;
- `session_messages` для истории диалога;
- agent loop с вызовом `filesystem.read` и потоковыми `agent.message.delta`;
- UI панели Settings с текущей конфигурацией модели;
- тесты model-gateway и agent-runtime.

### Этап 3 — Инструменты, shell, терминал, разрешения ✅

- `filesystem.write`, `filesystem.patch`, `filesystem.search`, `shell.execute` подключены к `tool-runtime`
- сервер публикует `approval.required`, принимает `approval.granted` / `approval.denied` и переводит задачу в `paused`
- xterm.js терминал и поток shell output доступны в UI
- UI настроек разрешений доступен в браузере

**Результат:** агент читает и пишет файлы, выполняет shell-команды и запрашивает подтверждение опасных действий в UI.

### Этап 4 — Редактор, файлы, Git ✅

Git backend реализован: `git.status`, `git.diff`, `git.commit`, `git.pull`, `git.push`.

- Monaco Editor
- Дерево файлов
- Git: status, diff, commit, branch, pull, push
- Просмотр diff в UI
- События `file.changed`, `git.diff.changed`

**Результат:** полноценная работа с кодом и Git через браузер.

### Этап 5 — Планирование задач и оркестрация ✅

Task lifecycle реализован: start/complete/fail/cancel/resume/retry. Storage содержит `task_steps` и `task_checkpoints`, а `agent-runtime` исполняет native ReAct calls и публикует tool/task events.

- Реальное планирование (`agent.plan.updated`) с fallback-парсингом JSON, fenced JSON и wrapper-объектов
- Параллельное выполнение независимых инструментов через dependency batching
- Отмена инструментов и задач
- Остановка / продолжение / повторный запуск
- Восстановление задач после перезапуска сервера

**Результат:** надёжный task orchestrator с recovery и видимыми шагами в UI.

### Этап 6 — Индексация, MCP, память, workers ✅

- Project index для контекстного поиска
- MCP-интеграции (`mcp.call`) уже есть на уровне tool-runtime и UI управления серверами
- Память агента
- Multi-model routing с task-scoped `model_route` и OpenAI-compatible маршрутами — после LiteRouter
- HTTP/queue Python worker для изолированных задач; reliability и специализированные ML handlers реализованы
- Rust server должен владеть persistence/retry policy worker jobs, сохраняя выполнение изолированным в Python worker
- `browser.open`, `browser.extract`
- UI управления MCP и инструментами

**Результат:** foundations Stage 6 завершены: агент поддерживает расширяемую экосистему, multi-model routing, project index, memory и worker integrations.

### Этап 7 — Hardening + Product ✅

- Waves A–D закрыли trust/security, recovery, agent quality и product honesty для Sites/Scheduled;
- Wave E `7.84`–`7.98` закрыла PostgreSQL/frontend/protocol/docs CI, Dev Container, OpenAPI и feature flags;
- `7.99`–`7.116` завершены, включая `7.107` — Stage 7 полностью закрыт.

---

## Требования к качеству

- Строгая типизация (Rust + TypeScript, общая JSON Schema)
- Отсутствие бизнес-логики во frontend
- Модульная архитектура (один concern — один crate)
- Тесты для ядра и инструментов
- Структурированные логи (`tracing`)
- Наблюдаемость: correlation id, метрики задач, approval latency, retries
- Миграции базы данных
- Обработка ошибок без падения сервера
- Ограничения ресурсов и таймауты на инструменты
- Безопасность shell и файловых операций
- Никакого Electron, desktop или Android-кода

---

## Связанные документы

- [roadmap.md](roadmap.md) — дорожная карта с milestones
- [providers/literouter.md](providers/literouter.md) — первый LLM-провайдер
- [current-state.md](current-state.md) — текущий статус реализации
- [architecture.md](architecture.md) — диаграмма компонентов
- [../AGENTS.md](../AGENTS.md) — гайд для AI-агентов
